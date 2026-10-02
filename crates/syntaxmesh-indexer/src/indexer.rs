//! Runtime-neutral indexing orchestration over an extractor and a graph store.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use syntaxmesh_core::{
    Edge, EdgeId, ExportKind, ExportRecord, FileId, GenerationId, GraphDelta, ImportKind,
    ImportRecord, IndexRunId, ModuleResolutionDiagnosticStatus, Node, NodeId, NodeKind,
    ProvenanceId, RelationKind, RepositoryId, SourceLocation, SourceSpan, WorktreeId,
};
use syntaxmesh_language_sdk::{
    ExtractorError, LanguageExtractor, Reference, SourceFile, SourceProcessing, SyntaxDiagnostic,
};
use syntaxmesh_resolver::{
    ModuleResolutionMode, ModuleResolutionOutcome, ModuleResolutionProvider,
    ModuleResolutionRequest, ModuleResolverIdentity, resolve_references,
};
use syntaxmesh_store::{GraphStore, StoreError};

#[cfg(test)]
#[path = "indexer/processing/tests.rs"]
mod processing_tests;
#[cfg(test)]
#[path = "indexer/refresh/tests.rs"]
mod refresh_tests;
#[cfg(test)]
#[path = "indexer/resolution_constraints/tests.rs"]
mod resolution_constraint_tests;
#[cfg(test)]
mod trait_tests;

/// Preparation policy; host-level planning and reporting must opt in separately.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SourceSyntaxPolicy {
    #[default]
    Strict,
    RecordFailures,
}

#[derive(Debug)]
pub enum IndexError {
    Extractor(ExtractorError),
    Store(StoreError),
    NoFiles,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for IndexError {}

impl From<ExtractorError> for IndexError {
    fn from(error: ExtractorError) -> Self {
        Self::Extractor(error)
    }
}

impl From<StoreError> for IndexError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// A small, embeddable indexing service. Workflow durability is supplied by
/// the caller's store and can later be wrapped by the Penelope adapter.
pub struct Indexer<S, E> {
    store: S,
    extractor: E,
    repository: RepositoryId,
    worktree: WorktreeId,
    module_resolution: Option<Arc<dyn ModuleResolutionProvider>>,
    syntax_policy: SourceSyntaxPolicy,
}

impl<S, E> Indexer<S, E>
where
    S: GraphStore,
    E: LanguageExtractor,
{
    #[must_use]
    pub const fn new(
        store: S,
        extractor: E,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Self {
        Self {
            store,
            extractor,
            repository,
            worktree,
            module_resolution: None,
            syntax_policy: SourceSyntaxPolicy::Strict,
        }
    }

    /// Opt in to syntax-failure coverage during preparation. Internal errors
    /// remain fatal. This does not change host inventory/no-op planning.
    #[must_use]
    pub const fn with_source_syntax_policy(mut self, policy: SourceSyntaxPolicy) -> Self {
        self.syntax_policy = policy;
        self
    }

    #[must_use]
    pub const fn source_syntax_policy(&self) -> SourceSyntaxPolicy {
        self.syntax_policy
    }

    /// Adds a host-owned module resolver. Without one, typed module occurrences remain unresolved.
    #[must_use]
    pub fn with_module_resolution_provider(
        mut self,
        provider: Arc<dyn ModuleResolutionProvider>,
    ) -> Self {
        self.module_resolution = Some(provider);
        self
    }

    /// Replace the provider for future preparation; `None` restores name-only resolution.
    /// Does not publish or change stored generations.
    pub fn set_module_resolution_provider(
        &mut self,
        provider: Option<Arc<dyn ModuleResolutionProvider>>,
    ) {
        self.module_resolution = provider;
    }

    #[must_use]
    pub const fn store(&self) -> &S {
        &self.store
    }

    /// Host paths consulted by the installed module provider, not graph IDs.
    ///
    /// # Errors
    /// Returns a provider diagnostic if observation inventory cannot be read.
    pub fn observed_module_inputs(&self) -> Result<Vec<String>, String> {
        self.module_resolution
            .as_ref()
            .map_or_else(|| Ok(Vec::new()), |provider| provider.observed_inputs())
    }

    /// Returns mutable access for an application workflow adapter.
    pub const fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// Returns the owned components so an application engine can route the
    /// prepared delta through a durable workflow adapter.
    #[must_use]
    pub fn into_parts(self) -> (S, E, RepositoryId, WorktreeId) {
        (self.store, self.extractor, self.repository, self.worktree)
    }

    /// Prepare one generation delta without mutating the store. All extraction
    /// happens before publication, so an extractor failure leaves the prior
    /// generation intact.
    ///
    /// # Errors
    /// Returns an extractor or store error; an empty batch is rejected.
    pub fn prepare_delta(
        &self,
        files: &[SourceFile],
        run_id: IndexRunId,
        next_generation: GenerationId,
    ) -> Result<GraphDelta, IndexError> {
        if let Some(provider) = &self.module_resolution {
            provider.refresh().map_err(|message| {
                IndexError::Extractor(ExtractorError::InvalidInput(format!(
                    "module resolver refresh failed: {message}"
                )))
            })?;
        }
        let expected_base = self
            .store
            .current_generation(self.repository, self.worktree)?
            .map(|manifest| manifest.generation);
        if files.is_empty() && expected_base.is_none() {
            return Err(IndexError::NoFiles);
        }
        let mut remove_nodes = Vec::new();
        let mut remove_edges = Vec::new();
        let mut removed_files: Vec<FileId> = Vec::new();
        let mut existing_files = BTreeMap::new();
        let mut existing_provenance = BTreeMap::<FileId, Vec<syntaxmesh_core::Provenance>>::new();
        let mut old_definitions = Vec::new();
        let mut files_to_extract = BTreeSet::new();
        if let Some(generation) = expected_base {
            let incoming_files = files
                .iter()
                .map(|source| source.file.file_id)
                .collect::<BTreeSet<_>>();
            for existing in self.store.files(generation)? {
                existing_files.insert(existing.file_id, existing);
            }
            for item in self.store.provenance(generation)? {
                if let Some(source) = &item.source {
                    existing_provenance
                        .entry(source.file_id)
                        .or_default()
                        .push(item);
                }
            }
            let mut changed_file_ids = BTreeSet::new();
            for existing in existing_files.values() {
                if !incoming_files.contains(&existing.file_id) {
                    removed_files.push(existing.file_id);
                    changed_file_ids.insert(existing.file_id);
                }
            }
            for source in files {
                let unchanged = existing_files
                    .get(&source.file.file_id)
                    .is_some_and(|existing| existing == &source.file);
                let identity = self.extractor.producer_identity_for(source)?;
                let identity_matches =
                    existing_provenance
                        .get(&source.file.file_id)
                        .is_some_and(|items| {
                            items.iter().any(|item| {
                                item.producer_namespace == identity.namespace
                                    && item.producer_version == identity.version
                            })
                        });
                let coverage_matches = if self.syntax_policy == SourceSyntaxPolicy::RecordFailures
                    && let Some(file) = existing_files.get(&source.file.file_id)
                {
                    let coverage_id = SourceProcessing::Completed.facts(file, &identity).0.id;
                    if let Some(node) = self.store.node(generation, coverage_id)? {
                        let evidence = existing_provenance
                            .get(&file.file_id)
                            .and_then(|items| items.iter().find(|item| item.id == node.provenance))
                            .ok_or_else(|| {
                                IndexError::Store(StoreError::Integrity(
                                    "processing evidence has no provenance".to_owned(),
                                ))
                            })?;
                        let coverage = SourceProcessing::from_facts(file, &node, evidence)
                            .map_err(|error| {
                                IndexError::Store(StoreError::Integrity(error.to_string()))
                            })?
                            .ok_or_else(|| {
                                IndexError::Store(StoreError::Integrity(
                                    "processing identity has unrelated node kind".to_owned(),
                                ))
                            })?;
                        coverage == SourceProcessing::Completed
                    } else {
                        false
                    }
                } else {
                    true
                };
                if !unchanged || !identity_matches || !coverage_matches {
                    changed_file_ids.insert(source.file.file_id);
                    files_to_extract.insert(source.file.file_id);
                }
            }
            for file_id in changed_file_ids {
                for node_id in self.store.nodes_for_file(generation, file_id)? {
                    if let Some(node) = self.store.node(generation, node_id)?
                        && is_definition_kind(&node.kind)
                    {
                        old_definitions.push(node);
                    }
                    remove_nodes.push(node_id);
                }
            }
        }
        let mut changed_files = Vec::with_capacity(files.len());
        let mut provenance = Vec::with_capacity(files.len());
        let mut nodes: Vec<Node> = Vec::new();
        let mut edges = Vec::new();
        let mut references = Vec::new();
        for source in files {
            if expected_base.is_some() && !files_to_extract.contains(&source.file.file_id) {
                continue;
            }
            let expected_identity = self.extractor.producer_identity_for(source)?;
            let extraction_result = self.extractor.extract(source);
            if self.syntax_policy == SourceSyntaxPolicy::RecordFailures
                && let Err(ExtractorError::SyntaxError(message)) = &extraction_result
            {
                let (node, evidence) =
                    SourceProcessing::SyntaxFailed(SyntaxDiagnostic::new(message))
                        .facts(&source.file, &expected_identity);
                if let Some(generation) = expected_base
                    && existing_files.get(&source.file.file_id) == Some(&source.file)
                    && self.store.node(generation, node.id)?.as_ref() == Some(&node)
                    && existing_provenance
                        .get(&source.file.file_id)
                        .is_some_and(|items| items.contains(&evidence))
                    && self.store.nodes_for_file(generation, source.file.file_id)? == [node.id]
                {
                    remove_nodes.retain(|id| *id != node.id);
                    continue;
                }
                changed_files.push(source.file.clone());
                nodes.push(node);
                provenance.push(evidence);
                continue;
            }
            let extraction = extraction_result.map_err(|error| {
                let message = format!("{}: {error}", source.file.normalized_path);
                IndexError::Extractor(match error {
                    ExtractorError::SyntaxError(_) => ExtractorError::SyntaxError(message),
                    ExtractorError::UnsupportedFile(_) | ExtractorError::InvalidInput(_) => {
                        ExtractorError::InvalidInput(message)
                    }
                })
            })?;
            if extraction.provenance.producer_namespace != expected_identity.namespace
                || extraction.provenance.producer_version != expected_identity.version
                || extraction
                    .provenance
                    .source
                    .as_ref()
                    .is_none_or(|source_location| {
                        source_location.file_id != source.file.file_id
                            || source_location.content_hash != source.file.content_hash
                    })
            {
                return Err(IndexError::Extractor(ExtractorError::InvalidInput(
                    "extractor output provenance must match its declared producer identity and source file"
                        .to_owned(),
                )));
            }
            changed_files.push(source.file.clone());
            if self.syntax_policy == SourceSyntaxPolicy::RecordFailures {
                let (node, evidence) =
                    SourceProcessing::Completed.facts(&source.file, &expected_identity);
                nodes.push(node);
                provenance.push(evidence);
            }
            let extraction_provenance = extraction.provenance.id;
            for record in extraction.imports {
                validate_source_occurrence(
                    &record,
                    source.file.file_id,
                    source.file.content_hash,
                    source.file.size_bytes,
                    extraction_provenance,
                    &extraction.nodes,
                )?;
                let (node, edge) = import_graph_facts(record);
                nodes.push(node);
                edges.push(edge);
            }
            for record in extraction.exports {
                validate_source_occurrence(
                    &record,
                    source.file.file_id,
                    source.file.content_hash,
                    source.file.size_bytes,
                    extraction_provenance,
                    &extraction.nodes,
                )?;
                let (node, edge) = export_graph_facts(record);
                nodes.push(node);
                edges.push(edge);
            }
            provenance.push(extraction.provenance);
            nodes.extend(extraction.nodes);
            edges.extend(extraction.edges);
            references.extend(extraction.references);
        }

        if let Some(generation) = expected_base {
            let removed_node_ids = remove_nodes.iter().copied().collect::<BTreeSet<_>>();
            let changed_definition_names = old_definitions
                .iter()
                .map(|node| node.name.clone())
                .chain(
                    nodes
                        .iter()
                        .filter(|node| is_definition_kind(&node.kind))
                        .map(|node| node.name.clone()),
                )
                .chain(
                    changed_files
                        .iter()
                        .filter(|file| is_supported_code_file_path(&file.normalized_path))
                        .map(|file| file.normalized_path.clone()),
                )
                .collect::<BTreeSet<_>>();
            let (exact_names, terminal_names) = lookup_names(changed_definition_names);
            let changed_target_candidates =
                self.store
                    .symbol_candidates(generation, &exact_names, &terminal_names)?;
            let mut impacted_sources = BTreeSet::new();
            for candidate in changed_target_candidates
                .iter()
                .filter(|node| is_reference_kind(&node.kind))
                .filter(|node| !removed_node_ids.contains(&node.id))
            {
                let source_edge = self
                    .store
                    .incoming(generation, candidate.id)?
                    .into_iter()
                    .find(|edge| edge.relation == RelationKind::References)
                    .ok_or_else(|| {
                        IndexError::Store(StoreError::Integrity(
                            "persisted reference node has no source edge".to_owned(),
                        ))
                    })?;
                if !removed_node_ids.contains(&source_edge.source) {
                    impacted_sources.insert(source_edge.source);
                }
            }

            for source_id in impacted_sources {
                for source_reference_edge in self.store.outgoing(generation, source_id)? {
                    if source_reference_edge.relation != RelationKind::References {
                        continue;
                    }
                    let reference_node = self
                        .store
                        .node(generation, source_reference_edge.target)?
                        .ok_or_else(|| {
                            IndexError::Store(StoreError::Integrity(
                                "reference edge targets a missing node".to_owned(),
                            ))
                        })?;
                    if !is_reference_kind(&reference_node.kind) {
                        continue;
                    }
                    let source_location = reference_node.source.clone().ok_or_else(|| {
                        IndexError::Store(StoreError::Integrity(
                            "persisted reference node has no source location".to_owned(),
                        ))
                    })?;
                    let relation = reference_relation(&reference_node.kind).ok_or_else(|| {
                        IndexError::Store(StoreError::Integrity(
                            "persisted reference node has an unsupported kind".to_owned(),
                        ))
                    })?;
                    references.push(Reference {
                        resolution: syntaxmesh_language_sdk::ReferenceResolution::from_payload(
                            reference_node.extension_payload.as_ref(),
                        )
                        .map_err(|error| {
                            IndexError::Store(StoreError::Integrity(error.to_string()))
                        })?,
                        id: reference_node.id,
                        source: source_id,
                        target: reference_node.name.clone(),
                        relation: relation.clone(),
                        source_location,
                        provenance: reference_node.provenance,
                    });
                    remove_nodes.push(reference_node.id);

                    for incoming_edge in self.store.incoming(generation, reference_node.id)? {
                        remove_edges.push(incoming_edge.id);
                    }
                    for resolution_edge in self.store.outgoing(generation, reference_node.id)? {
                        remove_edges.push(resolution_edge.id);
                        if resolution_edge.relation == RelationKind::ResolvesTo {
                            remove_edges.extend(
                                self.store
                                    .outgoing(generation, source_id)?
                                    .into_iter()
                                    .filter(|semantic| {
                                        semantic.target == resolution_edge.target
                                            && semantic.relation == relation
                                    })
                                    .map(|semantic| semantic.id),
                            );
                        }
                    }
                }
            }
        }

        // Markdown source links target file-level facts, not language-specific
        // module nodes. Only synthesize File nodes for exact in-scope source
        // paths that were actually linked; ordinary indexing stays unchanged.
        // Documentation's authored `decides`/`proposes` relations use this same
        // exact-path target projection as ordinary Markdown references.
        let linked_source_paths = references
            .iter()
            .filter(|reference| {
                reference.relation == RelationKind::References
                    || matches!(
                        &reference.relation,
                        RelationKind::External { namespace, relation }
                            if namespace == "syntaxmesh.documentation"
                                && matches!(relation.as_str(), "decides" | "proposes")
                    )
            })
            .map(|reference| reference.target.as_str())
            .collect::<BTreeSet<_>>();
        for source in files.iter().filter(|source| {
            linked_source_paths.contains(source.file.normalized_path.as_str())
                && is_supported_code_file_path(&source.file.normalized_path)
        }) {
            let file_node_id = NodeId::derive(&[
                &source.file.file_id.0.0,
                b"syntaxmesh-indexed-source-file-v1",
            ]);
            let prior_file_node_exists = if let Some(generation) = expected_base {
                self.store.node(generation, file_node_id)?.is_some()
            } else {
                false
            };
            if prior_file_node_exists && !remove_nodes.contains(&file_node_id) {
                continue;
            }
            let file_provenance = provenance
                .iter()
                .find(|item| {
                    item.source
                        .as_ref()
                        .is_some_and(|location| location.file_id == source.file.file_id)
                })
                .or_else(|| {
                    existing_provenance
                        .get(&source.file.file_id)
                        .and_then(|items| items.first())
                })
                .ok_or_else(|| {
                    IndexError::Extractor(ExtractorError::InvalidInput(format!(
                        "linked source file has no producer provenance: {}",
                        source.file.normalized_path
                    )))
                })?;
            nodes.push(indexed_source_file_node(
                source,
                file_node_id,
                file_provenance.id,
            )?);
        }

        let (exact_names, terminal_names) =
            lookup_names(references.iter().map(|reference| reference.target.clone()));
        let mut definitions = if let Some(generation) = expected_base {
            let removed_node_ids = remove_nodes.iter().copied().collect::<BTreeSet<_>>();
            self.store
                .symbol_candidates(generation, &exact_names, &terminal_names)?
                .into_iter()
                .filter(|node| {
                    is_definition_kind(&node.kind) && !removed_node_ids.contains(&node.id)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        definitions.extend(
            nodes
                .iter()
                .filter(|node| is_definition_kind(&node.kind))
                .cloned(),
        );
        let resolution = resolve_references(&definitions, &references);
        nodes.extend(resolution.reference_nodes);
        edges.extend(resolution.edges);

        if let Some(provider) = &self.module_resolution {
            let mut prior_nodes = if let Some(generation) = expected_base {
                self.store.module_resolution_nodes(generation)?
            } else {
                Vec::new()
            };
            prior_nodes.retain(|node| {
                matches!(
                    &node.kind,
                    NodeKind::Module
                        | NodeKind::Import { .. }
                        | NodeKind::Export { .. }
                        | NodeKind::ModuleResolutionDiagnostic { .. }
                ) && !remove_nodes.contains(&node.id)
            });
            let previous_diagnostic_ids = prior_nodes
                .iter()
                .filter_map(|node| {
                    matches!(node.kind, NodeKind::ModuleResolutionDiagnostic { .. })
                        .then_some(node.id)
                })
                .collect::<BTreeSet<_>>();
            let mut module_paths = BTreeMap::<String, NodeId>::new();
            let mut module_by_file = BTreeMap::<FileId, NodeId>::new();
            for node in prior_nodes.iter().chain(&nodes) {
                if node.kind == NodeKind::Module {
                    module_paths.insert(node.name.replace('\\', "/"), node.id);
                    if let Some(owner_file) = node.owner_file {
                        module_by_file.insert(owner_file, node.id);
                    }
                }
            }
            let mut exports_by_module = BTreeMap::<(NodeId, String), Vec<NodeId>>::new();
            let mut star_exports_by_module = BTreeMap::<NodeId, Vec<NodeId>>::new();
            let mut module_export_kinds = BTreeMap::<NodeId, ExportKind>::new();
            for node in prior_nodes.iter().chain(&nodes) {
                let NodeKind::Export {
                    kind,
                    exported_name,
                    ..
                } = &node.kind
                else {
                    continue;
                };
                let Some(owner_file) = node.owner_file else {
                    continue;
                };
                let Some(module_id) = module_by_file.get(&owner_file).copied() else {
                    continue;
                };
                module_export_kinds.insert(node.id, *kind);
                match kind {
                    ExportKind::Local
                    | ExportKind::Default
                    | ExportKind::NamedReExport
                    | ExportKind::NamespaceReExport => {
                        if let Some(exported_name) = exported_name {
                            exports_by_module
                                .entry((module_id, exported_name.clone()))
                                .or_default()
                                .push(node.id);
                        }
                    }
                    ExportKind::StarReExport => star_exports_by_module
                        .entry(module_id)
                        .or_default()
                        .push(node.id),
                }
            }
            let mut source_paths = existing_files
                .iter()
                .map(|(id, file)| (*id, file.normalized_path.clone()))
                .collect::<BTreeMap<_, _>>();
            for file in &changed_files {
                source_paths.insert(file.file_id, file.normalized_path.clone());
            }
            let occurrences = prior_nodes
                .iter()
                .filter(|node| is_module_occurrence(&node.kind))
                .chain(nodes.iter().filter(|node| is_module_occurrence(&node.kind)))
                .cloned()
                .collect::<Vec<_>>();
            let mut previous_resolution_edges = Vec::new();

            // Replace only resolver-owned edges. Extracted Import/Export facts and their
            // source provenance are deliberately untouched.
            for occurrence in &occurrences {
                if let Some(generation) = expected_base
                    && prior_nodes.iter().any(|node| node.id == occurrence.id)
                {
                    previous_resolution_edges.extend(
                        self.store
                            .outgoing(generation, occurrence.id)?
                            .into_iter()
                            .filter(|edge| {
                                matches!(
                                    &edge.relation,
                                    RelationKind::ResolvesTo
                                        | RelationKind::HasResolutionDiagnostic
                                )
                            }),
                    );
                }
                let Some(owner_file) = occurrence.owner_file else {
                    continue;
                };
                let Some(source_path) = source_paths.get(&owner_file) else {
                    continue;
                };
                let (specifier, mode) = match &occurrence.kind {
                    NodeKind::Import {
                        specifier, kind, ..
                    } => match kind {
                        syntaxmesh_core::ImportKind::PythonModule => {
                            (Some(specifier.as_str()), ModuleResolutionMode::PythonModule)
                        }
                        syntaxmesh_core::ImportKind::PythonFrom => {
                            (Some(specifier.as_str()), ModuleResolutionMode::PythonFrom)
                        }
                        syntaxmesh_core::ImportKind::PythonStar => {
                            (Some(specifier.as_str()), ModuleResolutionMode::PythonStar)
                        }
                        syntaxmesh_core::ImportKind::RustUse
                        | syntaxmesh_core::ImportKind::RustGlob => {
                            (None, ModuleResolutionMode::Import)
                        }
                        syntaxmesh_core::ImportKind::CommonJs => {
                            (Some(specifier.as_str()), ModuleResolutionMode::CommonJs)
                        }
                        syntaxmesh_core::ImportKind::SideEffect
                        | syntaxmesh_core::ImportKind::Default
                        | syntaxmesh_core::ImportKind::Named
                        | syntaxmesh_core::ImportKind::Namespace
                        | syntaxmesh_core::ImportKind::NamespaceMember
                        | syntaxmesh_core::ImportKind::Dynamic => {
                            (Some(specifier.as_str()), ModuleResolutionMode::Import)
                        }
                    },
                    NodeKind::Export {
                        source_specifier, ..
                    } => (source_specifier.as_deref(), ModuleResolutionMode::Import),
                    NodeKind::Repository
                    | NodeKind::File
                    | NodeKind::Module
                    | NodeKind::Function
                    | NodeKind::Struct
                    | NodeKind::Enum
                    | NodeKind::Trait
                    | NodeKind::Test
                    | NodeKind::External { .. }
                    | NodeKind::Reference { .. }
                    | NodeKind::UnresolvedReference { .. }
                    | NodeKind::AmbiguousReference { .. }
                    | NodeKind::RuntimeObservation
                    | NodeKind::Class
                    | NodeKind::ModuleResolutionDiagnostic { .. }
                    | NodeKind::Script
                    | NodeKind::Document
                    | NodeKind::Section
                    | NodeKind::DocumentChunk => (None, ModuleResolutionMode::Import),
                };
                let Some(specifier) = specifier else {
                    continue;
                };
                let request = ModuleResolutionRequest {
                    occurrence: occurrence.id,
                    source_path: source_path.clone(),
                    specifier: specifier.to_owned(),
                    mode,
                };
                if !provider.supports(&request) {
                    continue;
                }
                let outcome = provider.resolve(&request);
                let diagnostic = match outcome {
                    ModuleResolutionOutcome::Resolved(path) => {
                        module_paths.get(&path.replace('\\', "/")).map_or_else(
                            || {
                                Some((
                                    ModuleResolutionDiagnosticStatus::TargetNotIndexed,
                                    Vec::new(),
                                ))
                            },
                            |target| {
                                let identity = provider.identity();
                                let source = occurrence.source.clone();
                                let provenance_id = ProvenanceId::derive(&[
                                    b"module-resolution",
                                    &occurrence.id.0.0,
                                    &target.0.0,
                                    identity.namespace.as_bytes(),
                                    identity.version.as_bytes(),
                                    &identity.settings_fingerprint,
                                ]);
                                provenance.push(syntaxmesh_core::Provenance {
                                    id: provenance_id,
                                    producer_namespace: identity.namespace.clone(),
                                    producer_version: identity.version.clone(),
                                    evidence_class:
                                        syntaxmesh_core::EvidenceClass::StaticallyResolved,
                                    source,
                                });
                                edges.push(Edge {
                                    id: EdgeId::derive(&[
                                        &occurrence.id.0.0,
                                        &target.0.0,
                                        b"module-resolution",
                                    ]),
                                    source: occurrence.id,
                                    target: *target,
                                    relation: RelationKind::ResolvesTo,
                                    provenance: provenance_id,
                                    extension_payload: None,
                                });
                                None
                            },
                        )
                    }
                    ModuleResolutionOutcome::Unresolved(_) => {
                        Some((ModuleResolutionDiagnosticStatus::Unresolved, Vec::new()))
                    }
                    ModuleResolutionOutcome::ResolvedButNotIndexed => Some((
                        ModuleResolutionDiagnosticStatus::TargetNotIndexed,
                        Vec::new(),
                    )),
                    ModuleResolutionOutcome::Ambiguous(candidates) => Some((
                        ModuleResolutionDiagnosticStatus::Ambiguous,
                        normalize_candidate_paths(candidates),
                    )),
                    ModuleResolutionOutcome::Invalid(_) => {
                        Some((ModuleResolutionDiagnosticStatus::Invalid, Vec::new()))
                    }
                };
                if let Some((status, candidate_paths)) = diagnostic {
                    let (diagnostic_node, diagnostic_provenance, diagnostic_edge) =
                        resolution_diagnostic(
                            occurrence,
                            specifier,
                            provider.identity(),
                            status,
                            candidate_paths,
                        );
                    nodes.push(diagnostic_node);
                    provenance.push(diagnostic_provenance);
                    edges.push(diagnostic_edge);
                }
            }
            let module_ids = module_by_file.values().copied().collect::<BTreeSet<_>>();
            let mut resolved_star_modules = BTreeMap::<NodeId, NodeId>::new();
            for edge in &edges {
                if edge.relation == RelationKind::ResolvesTo
                    && module_export_kinds.get(&edge.source) == Some(&ExportKind::StarReExport)
                    && module_ids.contains(&edge.target)
                {
                    resolved_star_modules.insert(edge.source, edge.target);
                }
            }
            for occurrence in &occurrences {
                let NodeKind::Import {
                    kind: ImportKind::Named | ImportKind::Default | ImportKind::NamespaceMember,
                    imported_name: Some(imported_name),
                    ..
                } = &occurrence.kind
                else {
                    continue;
                };
                let Some(target_module) = edges.iter().find_map(|edge| {
                    (edge.source == occurrence.id
                        && edge.relation == RelationKind::ResolvesTo
                        && module_ids.contains(&edge.target))
                    .then_some(edge.target)
                }) else {
                    continue;
                };
                let Some(export_id) = uniquely_resolved_export(
                    target_module,
                    imported_name,
                    &exports_by_module,
                    &star_exports_by_module,
                    &resolved_star_modules,
                ) else {
                    continue;
                };
                let identity = provider.identity();
                let binding_provenance = ProvenanceId::derive(&[
                    b"syntaxmesh-ecmascript-export-binding-v3",
                    &occurrence.id.0.0,
                    &export_id.0.0,
                    identity.namespace.as_bytes(),
                    identity.version.as_bytes(),
                    &identity.settings_fingerprint,
                ]);
                provenance.push(syntaxmesh_core::Provenance {
                    id: binding_provenance,
                    producer_namespace: "syntaxmesh.ecmascript-export-binding".to_owned(),
                    producer_version: "3".to_owned(),
                    evidence_class: syntaxmesh_core::EvidenceClass::StaticallyResolved,
                    source: occurrence.source.clone(),
                });
                edges.push(Edge {
                    id: EdgeId::derive(&[
                        &occurrence.id.0.0,
                        &export_id.0.0,
                        b"ecmascript-export-binding-v3",
                    ]),
                    source: occurrence.id,
                    target: export_id,
                    relation: RelationKind::ResolvesTo,
                    provenance: binding_provenance,
                    extension_payload: None,
                });
            }
            let desired_edge_ids = edges
                .iter()
                .filter(|edge| {
                    matches!(
                        &edge.relation,
                        RelationKind::ResolvesTo | RelationKind::HasResolutionDiagnostic
                    )
                })
                .map(|edge| edge.id)
                .collect::<BTreeSet<_>>();
            remove_edges.extend(
                previous_resolution_edges
                    .into_iter()
                    .filter(|edge| !desired_edge_ids.contains(&edge.id))
                    .map(|edge| edge.id),
            );
            let desired_diagnostic_ids = nodes
                .iter()
                .filter_map(|node| {
                    matches!(&node.kind, NodeKind::ModuleResolutionDiagnostic { .. })
                        .then_some(node.id)
                })
                .collect::<BTreeSet<_>>();
            remove_nodes.extend(
                previous_diagnostic_ids
                    .difference(&desired_diagnostic_ids)
                    .copied(),
            );
        } else if let Some(generation) = expected_base {
            // Removing/disabling a provider invalidates its derived module edges just like
            // changing its settings does. Source occurrences remain canonical and unchanged.
            for occurrence in self.store.module_resolution_nodes(generation)? {
                if is_module_occurrence(&occurrence.kind) {
                    remove_edges.extend(
                        self.store
                            .outgoing(generation, occurrence.id)?
                            .into_iter()
                            .filter(|edge| {
                                matches!(
                                    &edge.relation,
                                    RelationKind::ResolvesTo
                                        | RelationKind::HasResolutionDiagnostic
                                )
                            })
                            .map(|edge| edge.id),
                    );
                } else if matches!(occurrence.kind, NodeKind::ModuleResolutionDiagnostic { .. }) {
                    remove_nodes.push(occurrence.id);
                }
            }
        }

        if let Some(generation) = expected_base {
            for node_id in &remove_nodes {
                remove_edges.extend(
                    self.store
                        .incoming(generation, *node_id)?
                        .into_iter()
                        .map(|edge| edge.id),
                );
                remove_edges.extend(
                    self.store
                        .outgoing(generation, *node_id)?
                        .into_iter()
                        .map(|edge| edge.id),
                );
            }
        }
        remove_nodes.sort_unstable();
        remove_nodes.dedup();
        remove_edges.sort_unstable();
        remove_edges.dedup();
        let delta = GraphDelta {
            repository: self.repository,
            worktree: self.worktree,
            run_id,
            expected_base,
            next_generation,
            changed_files,
            removed_files,
            upsert_provenance: provenance,
            upsert_nodes: nodes,
            upsert_edges: edges,
            remove_nodes,
            remove_edges,
        };
        Ok(delta)
    }

    /// Extract and publish one generation through the direct store port.
    /// Application engines may call [`Self::prepare_delta`] and route the
    /// resulting delta through Penelope instead.
    ///
    /// # Errors
    /// Returns an extractor or store error; an empty batch is rejected.
    pub fn index(
        &mut self,
        files: &[SourceFile],
        run_id: IndexRunId,
        next_generation: GenerationId,
    ) -> Result<syntaxmesh_core::GenerationManifest, IndexError> {
        let delta = self.prepare_delta(files, run_id, next_generation)?;
        Ok(self.store.apply_delta(delta)?)
    }
}

trait SourceOccurrence {
    fn module(&self) -> NodeId;
    fn source_file(&self) -> FileId;
    fn content_hash(&self) -> [u8; 32];
    fn source_span(&self) -> SourceSpan;
    fn provenance(&self) -> syntaxmesh_core::ProvenanceId;
}

impl SourceOccurrence for ImportRecord {
    fn module(&self) -> NodeId {
        self.module
    }

    fn source_file(&self) -> FileId {
        self.source.file_id
    }

    fn content_hash(&self) -> [u8; 32] {
        self.source.content_hash
    }

    fn source_span(&self) -> SourceSpan {
        self.source.span
    }

    fn provenance(&self) -> syntaxmesh_core::ProvenanceId {
        self.provenance
    }
}

impl SourceOccurrence for ExportRecord {
    fn module(&self) -> NodeId {
        self.module
    }

    fn source_file(&self) -> FileId {
        self.source.file_id
    }

    fn content_hash(&self) -> [u8; 32] {
        self.source.content_hash
    }

    fn source_span(&self) -> SourceSpan {
        self.source.span
    }

    fn provenance(&self) -> syntaxmesh_core::ProvenanceId {
        self.provenance
    }
}

fn validate_source_occurrence<T: SourceOccurrence>(
    occurrence: &T,
    expected_file: FileId,
    expected_hash: [u8; 32],
    expected_size: u64,
    expected_provenance: syntaxmesh_core::ProvenanceId,
    extracted_nodes: &[Node],
) -> Result<(), IndexError> {
    if occurrence.source_file() != expected_file
        || occurrence.content_hash() != expected_hash
        || occurrence.provenance() != expected_provenance
        || occurrence.source_span().start_byte > occurrence.source_span().end_byte
        || occurrence.source_span().end_byte > expected_size
        || !extracted_nodes.iter().any(|node| {
            node.id == occurrence.module()
                && node.kind == NodeKind::Module
                && node.owner_file == Some(expected_file)
                && node.provenance == expected_provenance
        })
    {
        return Err(IndexError::Extractor(ExtractorError::InvalidInput(
            "import/export output must reference this extraction's module, source bytes, and provenance"
                .to_owned(),
        )));
    }
    Ok(())
}

fn resolution_diagnostic(
    occurrence: &Node,
    specifier: &str,
    identity: &ModuleResolverIdentity,
    status: ModuleResolutionDiagnosticStatus,
    candidate_paths: Vec<String>,
) -> (Node, syntaxmesh_core::Provenance, Edge) {
    let diagnostic_id = NodeId::derive(&[
        b"module-resolution-diagnostic-v1",
        &occurrence.id.0.0,
        identity.namespace.as_bytes(),
        identity.version.as_bytes(),
        &identity.settings_fingerprint,
    ]);
    let provenance_id = syntaxmesh_core::ProvenanceId::derive(&[
        b"module-resolution-diagnostic-provenance-v1",
        &diagnostic_id.0.0,
    ]);
    let node = Node {
        id: diagnostic_id,
        kind: NodeKind::ModuleResolutionDiagnostic {
            occurrence: occurrence.id,
            status,
            candidate_paths,
        },
        name: specifier.to_owned(),
        owner_file: occurrence.owner_file,
        source: occurrence.source.clone(),
        provenance: provenance_id,
        extension_payload: None,
    };
    let provenance = syntaxmesh_core::Provenance {
        id: provenance_id,
        producer_namespace: identity.namespace.clone(),
        producer_version: identity.version.clone(),
        evidence_class: syntaxmesh_core::EvidenceClass::ResolutionDiagnostic,
        source: occurrence.source.clone(),
    };
    let edge = Edge {
        id: EdgeId::derive(&[
            &occurrence.id.0.0,
            &diagnostic_id.0.0,
            b"has-module-resolution-diagnostic-v1",
        ]),
        source: occurrence.id,
        target: diagnostic_id,
        relation: RelationKind::HasResolutionDiagnostic,
        provenance: provenance_id,
        extension_payload: None,
    };
    (node, provenance, edge)
}

fn uniquely_resolved_export(
    target_module: NodeId,
    imported_name: &str,
    exports_by_module: &BTreeMap<(NodeId, String), Vec<NodeId>>,
    star_exports_by_module: &BTreeMap<NodeId, Vec<NodeId>>,
    resolved_star_modules: &BTreeMap<NodeId, NodeId>,
) -> Option<NodeId> {
    let mut pending = vec![target_module];
    let mut visited = BTreeSet::new();
    let mut candidates = BTreeSet::new();
    while let Some(module_id) = pending.pop() {
        if !visited.insert(module_id) {
            continue;
        }
        if let Some(explicit_exports) =
            exports_by_module.get(&(module_id, imported_name.to_owned()))
        {
            candidates.extend(explicit_exports.iter().copied());
            continue;
        }
        if imported_name == "default" {
            continue;
        }
        for star_export in star_exports_by_module.get(&module_id).into_iter().flatten() {
            if let Some(target) = resolved_star_modules.get(star_export) {
                pending.push(*target);
            }
        }
    }
    match candidates.iter().copied().collect::<Vec<_>>().as_slice() {
        [export_id] => Some(*export_id),
        [] | [_, _, ..] => None,
    }
}

fn normalize_candidate_paths(candidates: Vec<String>) -> Vec<String> {
    let mut normalized = candidates
        .into_iter()
        .filter_map(|candidate| {
            let candidate = candidate.replace('\\', "/");
            if candidate.is_empty()
                || candidate.starts_with('/')
                || candidate.contains(':')
                || candidate
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                None
            } else {
                Some(candidate)
            }
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn import_graph_facts(record: ImportRecord) -> (Node, Edge) {
    let id = NodeId(record.id.0);
    let name = record
        .local_name
        .clone()
        .or_else(|| record.imported_name.clone())
        .unwrap_or_else(|| record.specifier.clone());
    let node = Node {
        id,
        kind: NodeKind::Import {
            specifier: record.specifier,
            kind: record.kind,
            imported_name: record.imported_name,
            local_name: record.local_name,
            type_only: record.type_only,
        },
        name,
        owner_file: Some(record.source.file_id),
        source: Some(record.source),
        provenance: record.provenance,
        extension_payload: None,
    };
    let edge = source_occurrence_edge(record.module, id, RelationKind::Imports, record.provenance);
    (node, edge)
}

fn export_graph_facts(record: ExportRecord) -> (Node, Edge) {
    let id = NodeId(record.id.0);
    let name = record
        .exported_name
        .clone()
        .or_else(|| record.local_name.clone())
        .unwrap_or_else(|| "*".to_owned());
    let node = Node {
        id,
        kind: NodeKind::Export {
            source_specifier: record.source_specifier,
            kind: record.kind,
            exported_name: record.exported_name,
            local_name: record.local_name,
            type_only: record.type_only,
        },
        name,
        owner_file: Some(record.source.file_id),
        source: Some(record.source),
        provenance: record.provenance,
        extension_payload: None,
    };
    let edge = source_occurrence_edge(record.module, id, RelationKind::Exports, record.provenance);
    (node, edge)
}

fn source_occurrence_edge(
    module: NodeId,
    occurrence: NodeId,
    relation: RelationKind,
    provenance: syntaxmesh_core::ProvenanceId,
) -> Edge {
    let relation_name = format!("{relation:?}");
    let id = EdgeId::derive(&[
        &module.0.0,
        &occurrence.0.0,
        b"source-occurrence",
        relation_name.as_bytes(),
    ]);
    Edge {
        id,
        source: module,
        target: occurrence,
        relation,
        provenance,
        extension_payload: None,
    }
}

const fn is_reference_kind(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Reference { .. }
            | NodeKind::UnresolvedReference { .. }
            | NodeKind::AmbiguousReference { .. }
    )
}

fn reference_relation(kind: &NodeKind) -> Option<RelationKind> {
    match kind {
        NodeKind::Reference { relation }
        | NodeKind::UnresolvedReference { relation }
        | NodeKind::AmbiguousReference { relation } => Some(relation.clone()),
        NodeKind::Repository
        | NodeKind::File
        | NodeKind::Module
        | NodeKind::Function
        | NodeKind::Struct
        | NodeKind::Enum
        | NodeKind::Trait
        | NodeKind::Test
        | NodeKind::External { .. }
        | NodeKind::RuntimeObservation
        | NodeKind::Class
        | NodeKind::Import { .. }
        | NodeKind::Export { .. }
        | NodeKind::ModuleResolutionDiagnostic { .. }
        | NodeKind::Script
        | NodeKind::Document
        | NodeKind::Section
        | NodeKind::DocumentChunk => None,
    }
}

fn is_definition_kind(kind: &NodeKind) -> bool {
    syntaxmesh_language_sdk::is_reference_target_kind(kind)
}

fn is_supported_code_file_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "sh" | "bash"
            )
        })
}

fn indexed_source_file_node(
    source: &SourceFile,
    id: NodeId,
    provenance: ProvenanceId,
) -> Result<Node, IndexError> {
    let end = u64::try_from(source.content.len())
        .map_err(|error| IndexError::Extractor(ExtractorError::InvalidInput(error.to_string())))?;
    let location = SourceLocation {
        file_id: source.file.file_id,
        content_hash: source.file.content_hash,
        span: SourceSpan::new(0, end).map_err(|error| {
            IndexError::Extractor(ExtractorError::InvalidInput(error.to_string()))
        })?,
    };
    Ok(Node {
        id,
        kind: NodeKind::File,
        name: source.file.normalized_path.clone(),
        owner_file: Some(source.file.file_id),
        source: Some(location),
        provenance,
        extension_payload: None,
    })
}

const fn is_module_occurrence(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Import { .. } | NodeKind::Export { .. })
}

fn lookup_names(names: impl IntoIterator<Item = String>) -> (Vec<String>, Vec<String>) {
    let mut exact = BTreeSet::new();
    let mut terminal = BTreeSet::new();
    for name in names {
        if let Some(last) = name.rsplit("::").next() {
            terminal.insert(last.to_owned());
        }
        exact.insert(name);
    }
    (exact.into_iter().collect(), terminal.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use syntaxmesh_core::{
        EvidenceClass, FileVersion, NodeKind, Provenance, ProvenanceId, SourceLocation, SourceSpan,
    };
    use syntaxmesh_lang_ecmascript::TypeScriptExtractor;
    use syntaxmesh_lang_rust::RustExtractor;
    use syntaxmesh_language_sdk::{
        Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, SourceFile,
    };
    use syntaxmesh_store::InMemoryGraphStore;

    struct CountingExtractor {
        calls: Cell<usize>,
        version: &'static str,
    }

    struct CountingRustExtractor {
        calls: Cell<usize>,
    }

    impl LanguageExtractor for CountingRustExtractor {
        fn language(&self) -> &'static str {
            "rust"
        }

        fn producer_identity(&self) -> ExtractorIdentity {
            RustExtractor.producer_identity()
        }

        fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
            self.calls.set(self.calls.get().saturating_add(1));
            RustExtractor.extract(source)
        }
    }

    impl LanguageExtractor for CountingExtractor {
        fn language(&self) -> &'static str {
            "fixture"
        }

        fn producer_identity(&self) -> ExtractorIdentity {
            ExtractorIdentity::new("syntaxmesh.test.fixture", self.version)
        }

        fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
            self.calls.set(self.calls.get().saturating_add(1));
            let provenance = Provenance {
                id: ProvenanceId::derive(&[&source.file.file_id.0.0]),
                producer_namespace: "syntaxmesh.test.fixture".to_owned(),
                producer_version: self.version.to_owned(),
                evidence_class: EvidenceClass::SourceFact,
                source: Some(SourceLocation {
                    file_id: source.file.file_id,
                    content_hash: source.file.content_hash,
                    span: SourceSpan {
                        start_byte: 0,
                        end_byte: source.file.size_bytes,
                    },
                }),
            };
            Ok(Extraction {
                provenance: provenance.clone(),
                nodes: vec![Node {
                    id: syntaxmesh_core::NodeId::derive(&[
                        &source.file.file_id.0.0,
                        b"fixture-node",
                    ]),
                    kind: NodeKind::Function,
                    name: source.content.clone(),
                    owner_file: Some(source.file.file_id),
                    source: None,
                    provenance: provenance.id,
                    extension_payload: None,
                }],
                edges: Vec::new(),
                references: Vec::new(),
                imports: Vec::new(),
                exports: Vec::new(),
            })
        }
    }

    fn fixture_source(path: &[u8], content: &str) -> SourceFile {
        SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[path]),
                normalized_path: String::from_utf8_lossy(path).into_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len()).unwrap_or_default(),
            },
            content: content.to_owned(),
        }
    }

    fn run_id(name: &[u8]) -> IndexRunId {
        IndexRunId::derive(&[name])
    }

    fn generation(name: &[u8]) -> GenerationId {
        GenerationId::derive(&[name])
    }

    fn integrity_error(message: &str) -> IndexError {
        IndexError::Store(StoreError::Integrity(message.to_owned()))
    }

    struct FixtureModuleResolver {
        identity: syntaxmesh_resolver::ModuleResolverIdentity,
    }

    impl syntaxmesh_resolver::ModuleResolutionProvider for FixtureModuleResolver {
        fn identity(&self) -> &syntaxmesh_resolver::ModuleResolverIdentity {
            &self.identity
        }

        fn resolve(
            &self,
            request: &syntaxmesh_resolver::ModuleResolutionRequest,
        ) -> syntaxmesh_resolver::ModuleResolutionOutcome {
            if request.specifier == "./target" {
                let target = if request.mode == ModuleResolutionMode::CommonJs {
                    "src/cjs.ts"
                } else {
                    "src/target.ts"
                };
                syntaxmesh_resolver::ModuleResolutionOutcome::Resolved(target.to_owned())
            } else {
                syntaxmesh_resolver::ModuleResolutionOutcome::Unresolved(
                    "not in fixture".to_owned(),
                )
            }
        }
    }

    struct OutcomeModuleResolver {
        identity: syntaxmesh_resolver::ModuleResolverIdentity,
        outcomes: BTreeMap<String, ModuleResolutionOutcome>,
    }

    impl syntaxmesh_resolver::ModuleResolutionProvider for OutcomeModuleResolver {
        fn identity(&self) -> &syntaxmesh_resolver::ModuleResolverIdentity {
            &self.identity
        }

        fn resolve(
            &self,
            request: &syntaxmesh_resolver::ModuleResolutionRequest,
        ) -> syntaxmesh_resolver::ModuleResolutionOutcome {
            self.outcomes
                .get(&request.specifier)
                .cloned()
                .unwrap_or_else(|| {
                    ModuleResolutionOutcome::Unresolved("fixture has no outcome".to_owned())
                })
        }
    }

    #[test]
    fn resolves_typed_imports_only_to_indexed_modules_and_rechecks_after_target_removal()
    -> Result<(), IndexError> {
        let repository = RepositoryId::derive(&[b"module-resolution-repo"]);
        let worktree = WorktreeId::derive(&[b"module-resolution-worktree"]);
        let resolver = FixtureModuleResolver {
            identity: syntaxmesh_resolver::ModuleResolverIdentity {
                namespace: "test.module-resolver".to_owned(),
                version: "1".to_owned(),
                settings_fingerprint: b"fixture-settings".to_vec(),
            },
        };
        let mut indexer = Indexer::new(
            InMemoryGraphStore::new(),
            TypeScriptExtractor,
            repository,
            worktree,
        )
        .with_module_resolution_provider(Arc::new(resolver));
        let importer = fixture_source(
            b"src/main.ts",
            "import { value } from './target'; const required = require('./target'); export { value };",
        );
        let target = fixture_source(b"src/target.ts", "export const value = 1;");
        let commonjs_target = fixture_source(b"src/cjs.ts", "module.exports = { value: 1 };");
        let first = indexer.index(
            &[importer.clone(), target.clone(), commonjs_target.clone()],
            run_id(b"module-resolution-run-1"),
            generation(b"module-resolution-generation-1"),
        )?;
        let first_nodes = indexer.store().nodes(first.generation)?;
        let occurrence = first_nodes
            .iter()
            .find(|node| {
                matches!(
                    &node.kind,
                    NodeKind::Import {
                        kind: syntaxmesh_core::ImportKind::Named,
                        ..
                    }
                )
            })
            .ok_or_else(|| integrity_error("typed import occurrence is missing"))?;
        let target_module = first_nodes
            .iter()
            .find(|node| node.name == "src/target.ts" && node.kind == NodeKind::Module)
            .ok_or_else(|| integrity_error("target module is missing"))?;
        let first_edges = indexer.store().edges(first.generation)?;
        let resolution_edge = first_edges
            .iter()
            .find(|edge| {
                edge.source == occurrence.id
                    && edge.target == target_module.id
                    && edge.relation == RelationKind::ResolvesTo
            })
            .ok_or_else(|| integrity_error("indexed module resolution edge is missing"))?;
        let commonjs_import = first_nodes
            .iter()
            .find(|node| {
                matches!(
                    &node.kind,
                    NodeKind::Import {
                        kind: syntaxmesh_core::ImportKind::CommonJs,
                        ..
                    }
                )
            })
            .ok_or_else(|| integrity_error("CommonJS import occurrence is missing"))?;
        let commonjs_target_module = first_nodes
            .iter()
            .find(|node| node.name == "src/cjs.ts" && node.kind == NodeKind::Module)
            .ok_or_else(|| integrity_error("CommonJS target module is missing"))?;
        if !first_edges.iter().any(|edge| {
            edge.source == commonjs_import.id
                && edge.target == commonjs_target_module.id
                && edge.relation == RelationKind::ResolvesTo
        }) {
            return Err(integrity_error(
                "CommonJS occurrence did not use require conditions",
            ));
        }
        let original_provenance = occurrence.provenance;
        if !indexer
            .store()
            .provenance(first.generation)?
            .iter()
            .any(|item| {
                item.id == resolution_edge.provenance
                    && item.evidence_class == EvidenceClass::StaticallyResolved
            })
        {
            return Err(integrity_error(
                "resolution provenance is not static evidence",
            ));
        }

        let second = indexer.index(
            std::slice::from_ref(&importer),
            run_id(b"module-resolution-run-2"),
            generation(b"module-resolution-generation-2"),
        )?;
        let second_nodes = indexer.store().nodes(second.generation)?;
        let unchanged_occurrence = second_nodes
            .iter()
            .find(|node| node.id == occurrence.id)
            .ok_or_else(|| {
                integrity_error("source import changed identity after target removal")
            })?;
        if unchanged_occurrence.provenance != original_provenance
            || indexer
                .store()
                .edges(second.generation)?
                .iter()
                .any(|edge| {
                    edge.source == occurrence.id && edge.relation == RelationKind::ResolvesTo
                })
        {
            return Err(integrity_error(
                "source evidence changed or stale module resolution survived target removal",
            ));
        }
        let target_removal_diagnostic = second_nodes.iter().find(|node| {
            matches!(
                &node.kind,
                NodeKind::ModuleResolutionDiagnostic {
                    occurrence: diagnostic_occurrence,
                    status: ModuleResolutionDiagnosticStatus::TargetNotIndexed,
                    ..
                } if *diagnostic_occurrence == occurrence.id
            )
        });
        let target_removal_diagnostic = target_removal_diagnostic
            .ok_or_else(|| integrity_error("target-removal diagnostic was not recorded"))?;
        if !indexer
            .store()
            .provenance(second.generation)?
            .iter()
            .any(|item| {
                item.id == target_removal_diagnostic.provenance
                    && item.evidence_class == EvidenceClass::ResolutionDiagnostic
            })
        {
            return Err(integrity_error(
                "target removal did not publish a typed, provenance-backed diagnostic",
            ));
        }
        let diagnostic_id = target_removal_diagnostic.id;
        let third = indexer.index(
            &[importer, target, commonjs_target],
            run_id(b"module-resolution-run-3"),
            generation(b"module-resolution-generation-3"),
        )?;
        let third_nodes = indexer.store().nodes(third.generation)?;
        if third_nodes.iter().any(|node| {
            matches!(
                &node.kind,
                NodeKind::ModuleResolutionDiagnostic {
                    occurrence: diagnostic_occurrence,
                    ..
                } if *diagnostic_occurrence == occurrence.id
            )
        }) || !indexer
            .store()
            .edges(third.generation)?
            .iter()
            .any(|edge| edge.source == occurrence.id && edge.relation == RelationKind::ResolvesTo)
        {
            return Err(integrity_error(
                "restoring the target did not replace its diagnostic with a resolution edge",
            ));
        }
        let diagnostic_history = indexer.store().node_history(diagnostic_id)?;
        if !matches!(diagnostic_history.as_slice(), [version]
            if version.valid_from == second.generation
                && version.valid_until == Some(third.generation))
        {
            return Err(integrity_error(
                "resolution diagnostic history did not retain its exact active generation",
            ));
        }
        Ok(())
    }

    #[test]
    fn persists_typed_module_resolution_outcomes_and_portable_candidates() -> Result<(), IndexError>
    {
        let repository = RepositoryId::derive(&[b"resolution-diagnostics-repo"]);
        let worktree = WorktreeId::derive(&[b"resolution-diagnostics-worktree"]);
        let mut outcomes = BTreeMap::new();
        outcomes.insert(
            "./missing".to_owned(),
            ModuleResolutionOutcome::Unresolved("filesystem-specific detail".to_owned()),
        );
        outcomes.insert(
            "./ambiguous".to_owned(),
            ModuleResolutionOutcome::Ambiguous(vec![
                "src/z.ts".to_owned(),
                "../outside.ts".to_owned(),
                "src/a.ts".to_owned(),
                "src/a.ts".to_owned(),
                "/absolute/path.ts".to_owned(),
            ]),
        );
        outcomes.insert(
            "./invalid".to_owned(),
            ModuleResolutionOutcome::Invalid("provider-private detail".to_owned()),
        );
        outcomes.insert(
            "./external".to_owned(),
            ModuleResolutionOutcome::Resolved("node_modules/external/index.js".to_owned()),
        );
        let resolver = OutcomeModuleResolver {
            identity: syntaxmesh_resolver::ModuleResolverIdentity {
                namespace: "test.module-outcome-resolver".to_owned(),
                version: "1".to_owned(),
                settings_fingerprint: b"stable-test-settings".to_vec(),
            },
            outcomes,
        };
        let mut indexer = Indexer::new(
            InMemoryGraphStore::new(),
            TypeScriptExtractor,
            repository,
            worktree,
        )
        .with_module_resolution_provider(Arc::new(resolver));
        let source = fixture_source(
            b"src/main.ts",
            "import './missing'; import './ambiguous'; import './invalid'; import './external';",
        );
        let manifest = indexer.index(
            &[source],
            run_id(b"resolution-diagnostics-run"),
            generation(b"resolution-diagnostics-generation"),
        )?;
        let nodes = indexer.store().nodes(manifest.generation)?;
        let diagnostics = nodes
            .iter()
            .filter(|node| matches!(&node.kind, NodeKind::ModuleResolutionDiagnostic { .. }))
            .collect::<Vec<_>>();
        if diagnostics.len() != 4 {
            return Err(integrity_error(
                "not every non-success module outcome became a diagnostic fact",
            ));
        }
        let mut statuses = BTreeSet::new();
        for diagnostic in &diagnostics {
            let NodeKind::ModuleResolutionDiagnostic {
                occurrence,
                status,
                candidate_paths,
            } = &diagnostic.kind
            else {
                return Err(integrity_error(
                    "diagnostic filter returned another node kind",
                ));
            };
            statuses.insert(*status);
            let occurrence = nodes
                .iter()
                .find(|node| node.id == *occurrence)
                .ok_or_else(|| integrity_error("diagnostic has no source occurrence"))?;
            let source_specifier = match &occurrence.kind {
                NodeKind::Import { specifier, .. } => Some(specifier.as_str()),
                NodeKind::Export {
                    source_specifier, ..
                } => source_specifier.as_deref(),
                NodeKind::Repository
                | NodeKind::File
                | NodeKind::Module
                | NodeKind::Function
                | NodeKind::Struct
                | NodeKind::Enum
                | NodeKind::Trait
                | NodeKind::Test
                | NodeKind::External { .. }
                | NodeKind::Reference { .. }
                | NodeKind::UnresolvedReference { .. }
                | NodeKind::AmbiguousReference { .. }
                | NodeKind::RuntimeObservation
                | NodeKind::Class
                | NodeKind::ModuleResolutionDiagnostic { .. }
                | NodeKind::Script
                | NodeKind::Document
                | NodeKind::Section
                | NodeKind::DocumentChunk => None,
            };
            if diagnostic.source != occurrence.source
                || diagnostic.owner_file != occurrence.owner_file
                || source_specifier != Some(diagnostic.name.as_str())
            {
                return Err(integrity_error(
                    "diagnostic changed or lost its source occurrence evidence",
                ));
            }
            if *status == ModuleResolutionDiagnosticStatus::Ambiguous
                && candidate_paths != &["src/a.ts".to_owned(), "src/z.ts".to_owned()]
            {
                return Err(integrity_error(
                    "ambiguous candidates were not normalized, sorted, and deduplicated",
                ));
            }
        }
        if statuses.len() != 4 {
            return Err(integrity_error(
                "unresolved, ambiguous, invalid, and non-indexed outcomes were not distinguished",
            ));
        }
        let provenance = indexer.store().provenance(manifest.generation)?;
        let edges = indexer.store().edges(manifest.generation)?;
        for diagnostic in diagnostics {
            let expected_provenance = provenance
                .iter()
                .find(|item| item.id == diagnostic.provenance)
                .ok_or_else(|| integrity_error("diagnostic provenance is missing"))?;
            let diagnostic_occurrence =
                if let NodeKind::ModuleResolutionDiagnostic { occurrence, .. } = &diagnostic.kind {
                    Some(*occurrence)
                } else {
                    None
                };
            if expected_provenance.evidence_class != EvidenceClass::ResolutionDiagnostic
                || !diagnostic_occurrence.is_some_and(|occurrence| {
                    edges.iter().any(|edge| {
                        edge.source == occurrence
                            && edge.target == diagnostic.id
                            && edge.relation == RelationKind::HasResolutionDiagnostic
                    })
                })
            {
                return Err(integrity_error(
                    "diagnostic lacks typed provenance or its occurrence relation",
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn unchanged_source_is_reextracted_when_producer_version_changes() -> Result<(), IndexError> {
        let repository = RepositoryId::derive(&[b"producer-version-repo"]);
        let worktree = WorktreeId::derive(&[b"producer-version-worktree"]);
        let source = fixture_source(b"src/module.py", "def stable(): pass");
        let mut first_indexer = Indexer::new(
            InMemoryGraphStore::new(),
            CountingExtractor {
                calls: Cell::new(0),
                version: "1",
            },
            repository,
            worktree,
        );
        let first = first_indexer.index(
            std::slice::from_ref(&source),
            run_id(b"producer-version-run-1"),
            generation(b"producer-version-generation-1"),
        )?;
        if first_indexer.extractor.calls.get() != 1 {
            return Err(integrity_error("initial source was not extracted"));
        }

        let (store, _, stored_repository, stored_worktree) = first_indexer.into_parts();
        let mut upgraded_indexer = Indexer::new(
            store,
            CountingExtractor {
                calls: Cell::new(0),
                version: "2",
            },
            stored_repository,
            stored_worktree,
        );
        let second = upgraded_indexer.index(
            &[source],
            run_id(b"producer-version-run-2"),
            generation(b"producer-version-generation-2"),
        )?;
        let provenance = upgraded_indexer.store().provenance(second.generation)?;
        if upgraded_indexer.extractor.calls.get() != 1
            || second.parent != Some(first.generation)
            || !provenance.iter().any(|item| {
                item.producer_namespace == "syntaxmesh.test.fixture" && item.producer_version == "2"
            })
        {
            return Err(integrity_error(
                "unchanged source retained facts from the prior producer version",
            ));
        }
        Ok(())
    }

    #[test]
    fn unchanged_rescan_skips_extraction_and_changes_only_affected_files() -> Result<(), IndexError>
    {
        let repository = RepositoryId::derive(&[b"incremental-repo"]);
        let worktree = WorktreeId::derive(&[b"incremental-worktree"]);
        let extractor = CountingExtractor {
            calls: Cell::new(0),
            version: "1",
        };
        let mut indexer = Indexer::new(InMemoryGraphStore::new(), extractor, repository, worktree);
        let first_file = fixture_source(b"src/first.rs", "first-v1");
        let second_file = fixture_source(b"src/second.rs", "second-v1");
        let first_files = [first_file, second_file.clone()];
        let first = indexer.index(&first_files, run_id(b"run-1"), generation(b"gen-1"))?;
        if indexer.extractor.calls.get() != 2 {
            return Err(integrity_error("first scan did not extract both files"));
        }
        let first_nodes = indexer.store().nodes(first.generation)?;
        let first_edges = indexer.store().edges(first.generation)?;
        let first_provenance = indexer.store().provenance(first.generation)?;
        let first_versions = indexer.store().files(first.generation)?;

        let unchanged = indexer.index(&first_files, run_id(b"run-2"), generation(b"gen-2"))?;
        if indexer.extractor.calls.get() != 2
            || indexer.store().nodes(unchanged.generation)? != first_nodes
            || indexer.store().edges(unchanged.generation)? != first_edges
            || indexer.store().provenance(unchanged.generation)? != first_provenance
            || indexer.store().files(unchanged.generation)? != first_versions
            || unchanged.parent != Some(first.generation)
        {
            return Err(integrity_error(
                "unchanged rescan performed extraction or changed canonical facts",
            ));
        }

        let edited_first = fixture_source(b"src/first.rs", "first-v2");
        let edited_files = [edited_first.clone(), second_file];
        let edited = indexer.index(&edited_files, run_id(b"run-3"), generation(b"gen-3"))?;
        let first_file_id = FileId::derive(&[b"src/first.rs"]);
        let second_file_id = FileId::derive(&[b"src/second.rs"]);
        let first_node = indexer
            .store()
            .nodes_for_file(edited.generation, first_file_id)?;
        let second_node = indexer
            .store()
            .nodes_for_file(edited.generation, second_file_id)?;
        if first_node.len() != 1 || second_node.len() != 1 {
            return Err(integrity_error(
                "edited generation has incorrect file ownership",
            ));
        }
        let Some(first_node_id) = first_node.first().copied() else {
            return Err(integrity_error("edited file has no indexed node"));
        };
        let Some(second_node_id) = second_node.first().copied() else {
            return Err(integrity_error("unchanged file lost its indexed node"));
        };
        if indexer.extractor.calls.get() != 3
            || indexer
                .store()
                .node(edited.generation, first_node_id)?
                .is_none_or(|node| node.name != "first-v2")
            || indexer
                .store()
                .node(edited.generation, second_node_id)?
                .is_none_or(|node| node.name != "second-v1")
        {
            return Err(integrity_error(
                "editing one file re-extracted unaffected files or lost facts",
            ));
        }

        let deleted = indexer.index(&[edited_first], run_id(b"run-4"), generation(b"gen-4"))?;
        if indexer.extractor.calls.get() != 3
            || indexer.store().files(deleted.generation)?.len() != 1
            || indexer.store().nodes(deleted.generation)?.len() != 1
            || !indexer
                .store()
                .nodes_for_file(deleted.generation, second_file_id)?
                .is_empty()
        {
            return Err(integrity_error(
                "deleting one file performed extraction or left owned facts",
            ));
        }
        Ok(())
    }

    #[test]
    fn resolves_cross_file_calls_and_revisits_unresolved_references() -> Result<(), IndexError> {
        let repository = RepositoryId::derive(&[b"resolver-repo"]);
        let worktree = WorktreeId::derive(&[b"resolver-worktree"]);
        let source = |path: &str, content: &str| SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[path.as_bytes()]),
                normalized_path: path.to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
            },
            content: content.to_owned(),
        };
        let main = source("src/main.rs", "fn caller() { helper(); missing(); }\n");
        let library = source("src/library.rs", "fn helper() {}\n");
        let unrelated = source(
            "src/unrelated.rs",
            "fn observer() { absent_elsewhere(); }\n",
        );
        let extractor = CountingRustExtractor {
            calls: Cell::new(0),
        };
        let mut indexer = Indexer::new(InMemoryGraphStore::new(), extractor, repository, worktree);
        let first = indexer.index(
            &[main.clone(), library, unrelated.clone()],
            run_id(b"resolver-run-1"),
            generation(b"resolver-generation-1"),
        )?;
        if indexer.extractor.calls.get() != 3 {
            return Err(integrity_error(
                "initial Rust files were not each extracted once",
            ));
        }
        let first_nodes = indexer.store().nodes(first.generation)?;
        let first_edges = indexer.store().edges(first.generation)?;
        let caller = first_nodes
            .iter()
            .find(|node| node.name == "caller")
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("caller definition is missing"))?;
        let helper = first_nodes
            .iter()
            .find(|node| node.name == "helper")
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("cross-file helper definition is missing"))?;
        let unresolved_id = first_nodes
            .iter()
            .find(|node| {
                node.name == "missing" && matches!(node.kind, NodeKind::UnresolvedReference { .. })
            })
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("missing call was silently dropped"))?;
        if !first_edges.iter().any(|edge| {
            edge.source == caller && edge.target == helper && edge.relation == RelationKind::Calls
        }) || !first_edges.iter().any(|edge| {
            edge.source == caller
                && edge.target == unresolved_id
                && edge.relation == RelationKind::References
        }) {
            return Err(integrity_error(
                "cross-file call or explicit unresolved edge is missing",
            ));
        }

        let updated_library = source("src/library.rs", "fn helper() {}\nfn missing() {}\n");
        let second = indexer.index(
            &[main, updated_library, unrelated],
            run_id(b"resolver-run-2"),
            generation(b"resolver-generation-2"),
        )?;
        if indexer.extractor.calls.get() != 4 {
            return Err(integrity_error(
                "re-resolving an unchanged caller performed parser work",
            ));
        }
        let second_nodes = indexer.store().nodes(second.generation)?;
        let second_edges = indexer.store().edges(second.generation)?;
        let resolved_missing = second_nodes
            .iter()
            .find(|node| node.name == "missing" && matches!(node.kind, NodeKind::Function))
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("new missing-call target was not indexed"))?;
        let untouched_unresolved = second_nodes
            .iter()
            .find(|node| {
                node.name == "absent_elsewhere"
                    && matches!(node.kind, NodeKind::UnresolvedReference { .. })
            })
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("unrelated unresolved reference was lost"))?;
        if second_nodes.iter().any(|node| {
            node.id == unresolved_id && matches!(node.kind, NodeKind::UnresolvedReference { .. })
        }) || !second_edges.iter().any(|edge| {
            edge.source == caller
                && edge.target == resolved_missing
                && edge.relation == RelationKind::Calls
        }) || !second_edges.iter().any(|edge| {
            edge.target == untouched_unresolved && edge.relation == RelationKind::References
        }) {
            return Err(integrity_error(
                "target addition failed re-resolution or changed an unrelated reference",
            ));
        }
        Ok(())
    }

    #[test]
    fn indexes_identical_file_contents_with_distinct_provenance() -> Result<(), IndexError> {
        let content = "fn shared() {}";
        let files = [
            fixture_source(b"src/first.rs", content),
            fixture_source(b"src/second.rs", content),
        ];
        let mut indexer = Indexer::new(
            InMemoryGraphStore::new(),
            RustExtractor,
            RepositoryId::derive(&[b"identical-content-repo"]),
            WorktreeId::derive(&[b"identical-content-worktree"]),
        );
        let manifest = indexer.index(
            &files,
            run_id(b"identical-content-run"),
            generation(b"identical-content-generation"),
        )?;
        let provenance = indexer.store().provenance(manifest.generation)?;
        if provenance.len() != 2
            || provenance
                .iter()
                .filter_map(|item| item.source.as_ref().map(|source| source.file_id))
                .collect::<BTreeSet<_>>()
                .len()
                != 2
        {
            return Err(integrity_error(
                "identical source files did not retain distinct file-bound provenance",
            ));
        }
        Ok(())
    }

    #[test]
    fn resolves_a_qualified_call_to_a_generic_impl_method() -> Result<(), IndexError> {
        let content = "struct Service<T>(T); impl<T> Service<T> { fn run() {} } fn caller() { Service::<u8>::run(); }";
        let file = SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[b"src/main.rs"]),
                normalized_path: "src/main.rs".to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
            },
            content: content.to_owned(),
        };
        let repository = RepositoryId::derive(&[b"generic-resolver-repo"]);
        let worktree = WorktreeId::derive(&[b"generic-resolver-worktree"]);
        let mut indexer = Indexer::new(
            InMemoryGraphStore::new(),
            syntaxmesh_lang_rust::RustExtractor,
            repository,
            worktree,
        );
        let manifest = indexer.index(
            &[file],
            run_id(b"generic-resolver-run"),
            generation(b"generic-resolver-generation"),
        )?;
        let nodes = indexer.store().nodes(manifest.generation)?;
        let caller = nodes
            .iter()
            .find(|node| node.name == "caller")
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("generic method caller was not indexed"))?;
        let method = nodes
            .iter()
            .find(|node| node.name == "Service < T >::run")
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("generic impl method was not indexed"))?;
        if !indexer
            .store()
            .edges(manifest.generation)?
            .iter()
            .any(|edge| {
                edge.source == caller
                    && edge.target == method
                    && edge.relation == RelationKind::Calls
            })
        {
            return Err(integrity_error(
                "qualified call did not resolve to its generic impl method",
            ));
        }
        Ok(())
    }

    #[test]
    fn resolves_self_method_calls_to_their_owning_impl() -> Result<(), IndexError> {
        let content = "struct Service; impl Service { fn run(&self) { self.build(); } fn build(&self) {} } struct Other; impl Other { fn build(&self) {} }";
        let file = SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[b"src/self_method.rs"]),
                normalized_path: "src/self_method.rs".to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len()).unwrap_or_default(),
            },
            content: content.to_owned(),
        };
        let repository = RepositoryId::derive(&[b"self-method-repo"]);
        let worktree = WorktreeId::derive(&[b"self-method-worktree"]);
        let mut indexer = Indexer::new(
            InMemoryGraphStore::new(),
            syntaxmesh_lang_rust::RustExtractor,
            repository,
            worktree,
        );
        let manifest = indexer.index(
            &[file],
            run_id(b"self-method-run"),
            generation(b"self-method-generation"),
        )?;
        let nodes = indexer.store().nodes(manifest.generation)?;
        let method_id = |name: &str| {
            nodes
                .iter()
                .find(|node| node.name == name && node.kind == NodeKind::Function)
                .map(|node| node.id)
        };
        let run = method_id("Service::run")
            .ok_or_else(|| integrity_error("Service::run was not indexed"))?;
        let service_build = method_id("Service::build")
            .ok_or_else(|| integrity_error("Service::build was not indexed"))?;
        let other_build = method_id("Other::build")
            .ok_or_else(|| integrity_error("Other::build was not indexed"))?;
        if !indexer
            .store()
            .edges(manifest.generation)?
            .iter()
            .any(|edge| {
                edge.source == run
                    && edge.target == service_build
                    && edge.relation == RelationKind::Calls
            })
            || indexer
                .store()
                .edges(manifest.generation)?
                .iter()
                .any(|edge| {
                    edge.source == run
                        && edge.target == other_build
                        && edge.relation == RelationKind::Calls
                })
        {
            return Err(integrity_error(
                "self method call did not resolve uniquely to its owning impl",
            ));
        }
        Ok(())
    }

    #[test]
    fn resolves_tuple_struct_constructor_calls() -> Result<(), IndexError> {
        let content = "pub struct Widget(u32); pub fn build() { let _widget = Widget(1); }";
        let file = SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[b"src/widget.rs"]),
                normalized_path: "src/widget.rs".to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len()).unwrap_or_default(),
            },
            content: content.to_owned(),
        };
        let repository = RepositoryId::derive(&[b"struct-constructor-repo"]);
        let worktree = WorktreeId::derive(&[b"struct-constructor-worktree"]);
        let mut indexer = Indexer::new(
            InMemoryGraphStore::new(),
            syntaxmesh_lang_rust::RustExtractor,
            repository,
            worktree,
        );
        let manifest = indexer.index(
            &[file],
            run_id(b"struct-constructor-run"),
            generation(b"struct-constructor-generation"),
        )?;
        let nodes = indexer.store().nodes(manifest.generation)?;
        let struct_id = nodes
            .iter()
            .find(|node| node.name == "Widget" && node.kind == NodeKind::Struct)
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("tuple struct definition is missing"))?;
        let caller_id = nodes
            .iter()
            .find(|node| node.name == "build" && node.kind == NodeKind::Function)
            .map(|node| node.id)
            .ok_or_else(|| integrity_error("struct constructor caller is missing"))?;
        if !indexer
            .store()
            .edges(manifest.generation)?
            .iter()
            .any(|edge| {
                edge.source == caller_id
                    && edge.target == struct_id
                    && edge.relation == RelationKind::Calls
            })
        {
            return Err(integrity_error(
                "tuple struct constructor call did not resolve",
            ));
        }
        Ok(())
    }

    #[test]
    fn indexes_a_rust_file_into_a_published_generation() -> Result<(), IndexError> {
        let make_file = |content: &str| SourceFile {
            file: FileVersion {
                file_id: syntaxmesh_core::FileId::derive(&[b"src/main.rs"]),
                normalized_path: "src/main.rs".to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: content.len() as u64,
            },
            content: content.to_owned(),
        };
        let repository = RepositoryId::derive(&[b"repo"]);
        let worktree = WorktreeId::derive(&[b"worktree"]);
        let mut indexer = Indexer::new(
            InMemoryGraphStore::new(),
            RustExtractor,
            repository,
            worktree,
        );
        let manifest = indexer.index(
            &[make_file("fn helper() {}\nfn main() { helper(); }\n")],
            IndexRunId::derive(&[b"run"]),
            GenerationId::derive(&[b"generation"]),
        )?;
        if manifest.repository != repository || manifest.worktree != worktree {
            return Err(IndexError::Store(StoreError::Integrity(
                "wrong generation scope".to_owned(),
            )));
        }
        let updated = indexer.index(
            &[make_file("struct Thing {}\n")],
            IndexRunId::derive(&[b"run-2"]),
            GenerationId::derive(&[b"generation-2"]),
        )?;
        let file_id = syntaxmesh_core::FileId::derive(&[b"src/main.rs"]);
        let old_id = syntaxmesh_core::NodeId::derive(&[&file_id.0.0, b"Function", b"main"]);
        if indexer.store().node(updated.generation, old_id)?.is_some() {
            return Err(IndexError::Store(StoreError::Integrity(
                "removed declaration survived file re-index".to_owned(),
            )));
        }
        if !indexer.store().edges(updated.generation)?.is_empty() {
            return Err(IndexError::Store(StoreError::Integrity(
                "edges owned by a changed file survived re-index".to_owned(),
            )));
        }
        let deleted = indexer.index(
            &[],
            IndexRunId::derive(&[b"run-3"]),
            GenerationId::derive(&[b"generation-3"]),
        )?;
        if !indexer.store().files(deleted.generation)?.is_empty() {
            return Err(IndexError::Store(StoreError::Integrity(
                "deleted file survived empty-tree re-index".to_owned(),
            )));
        }
        Ok(())
    }

    #[test]
    fn resolves_exports_through_star_chains_without_guessing_ambiguous_or_default_names() {
        let root = NodeId::derive(&[b"root"]);
        let left = NodeId::derive(&[b"left"]);
        let right = NodeId::derive(&[b"right"]);
        let first_star = NodeId::derive(&[b"first-star"]);
        let second_star = NodeId::derive(&[b"second-star"]);
        let left_export = NodeId::derive(&[b"left-export"]);
        let right_export = NodeId::derive(&[b"right-export"]);
        let mut stars = BTreeMap::new();
        stars.insert(root, vec![first_star]);
        stars.insert(left, vec![second_star]);
        stars.insert(right, vec![]);
        let mut resolved = BTreeMap::new();
        resolved.insert(first_star, left);
        resolved.insert(second_star, right);
        let mut explicit = BTreeMap::new();
        explicit.insert((right, "value".to_owned()), vec![left_export]);

        assert_eq!(
            uniquely_resolved_export(root, "value", &explicit, &stars, &resolved),
            Some(left_export),
            "named export should resolve through a multi-hop star chain"
        );
        assert_eq!(
            uniquely_resolved_export(root, "default", &explicit, &stars, &resolved),
            None,
            "default exports are never supplied by export-star"
        );

        explicit.insert((root, "value".to_owned()), vec![right_export]);
        assert_eq!(
            uniquely_resolved_export(root, "value", &explicit, &stars, &resolved),
            Some(right_export),
            "an explicit export shadows star-export branches"
        );
        explicit.remove(&(root, "value".to_owned()));

        explicit.insert((right, "value".to_owned()), vec![left_export, right_export]);
        assert_eq!(
            uniquely_resolved_export(root, "value", &explicit, &stars, &resolved),
            None,
            "multiple distinct matching exports must remain ambiguous"
        );

        explicit.remove(&(right, "value".to_owned()));
        resolved.insert(second_star, root);
        assert_eq!(
            uniquely_resolved_export(root, "missing", &explicit, &stars, &resolved),
            None,
            "cyclic star-export chains terminate and retain missing imports"
        );
    }
}
