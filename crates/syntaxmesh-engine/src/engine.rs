//! The embeddable SyntaxMesh application engine.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use syntaxmesh_core::{
    AcceptanceTime, ConsequenceDelta, Edge, EvidenceClass, ExtensionPayload, FileVersion,
    GenerationId, GenerationManifest, GenerationStatus, GraphDelta, GraphDeltaWithConsequences,
    GraphDeltaWithLineage, GraphSnapshot, IndexRunId, Node, NodeId, NodeKind, Provenance,
    ProvenanceId, RepositoryId, WorktreeId,
};
use syntaxmesh_extension_sdk::{ExtensionError, FactBatch};
use syntaxmesh_indexer::{IndexError, Indexer, SourceSyntaxPolicy};
use syntaxmesh_integration_penelope::PenelopePublisher;
use syntaxmesh_integration_statechronicle::StateChronicleVerifier;
use syntaxmesh_language_sdk::{LanguageExtractor, SourceFile};
use syntaxmesh_query::{Query, QueryError};
use syntaxmesh_resolver::ModuleResolutionProvider;
use syntaxmesh_semantic::SemanticError;
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError, graph_snapshot_root_v2};
use syntaxmesh_workflow::{
    DurableIndexWorkflow, GenerationVerifier, WorkflowError, WorkflowReceipt,
    WorkflowRejectionPage, WorkflowStatus,
};

mod integrity;
mod semantic;
#[cfg(test)]
mod tests;

/// Errors returned by the application engine.
#[derive(Debug)]
pub enum EngineError {
    /// Extraction or delta construction failed.
    Index(IndexError),
    /// Durable publication or verification failed.
    Workflow(WorkflowError),
    /// A generation-scoped query failed.
    Query(QueryError),
    /// Direct store read or verification-status update failed.
    Store(StoreError),
    /// An extension manifest or fact batch was invalid.
    Extension(ExtensionError),
    /// An observation could not be deterministically encoded.
    ObservationEncoding(serde_json::Error),
    /// Observation payload or provenance IDs collided with different facts.
    RuntimeObservationIdentityConflict,
    /// An extension batch contained no graph facts to publish.
    EmptyExtensionBatch,
    /// A generation's document facts could not form a valid semantic request.
    Semantic(SemanticError),
    /// Semantic batching requires non-zero limits and each chunk to fit one batch.
    InvalidSemanticBatchLimits,
    /// The injected wall clock could not provide an acceptance timestamp.
    Clock(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extension(error) => write!(formatter, "extension validation failed: {error}"),
            Self::Index(_)
            | Self::Workflow(_)
            | Self::Query(_)
            | Self::Store(_)
            | Self::ObservationEncoding(_)
            | Self::RuntimeObservationIdentityConflict
            | Self::EmptyExtensionBatch
            | Self::Semantic(_)
            | Self::InvalidSemanticBatchLimits
            | Self::Clock(_) => write!(formatter, "{self:?}"),
        }
    }
}

/// Upper bounds for provider-neutral semantic requests assembled from a graph
/// generation. Byte counts include exact chunk text and authored headings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticBatchLimits {
    pub max_chunks: usize,
    pub max_bytes: usize,
}

impl std::error::Error for EngineError {}

impl From<IndexError> for EngineError {
    fn from(error: IndexError) -> Self {
        Self::Index(error)
    }
}

impl From<WorkflowError> for EngineError {
    fn from(error: WorkflowError) -> Self {
        Self::Workflow(error)
    }
}

impl From<QueryError> for EngineError {
    fn from(error: QueryError) -> Self {
        Self::Query(error)
    }
}

impl From<StoreError> for EngineError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ExtensionError> for EngineError {
    fn from(error: ExtensionError) -> Self {
        Self::Extension(error)
    }
}

impl From<SemanticError> for EngineError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<serde_json::Error> for EngineError {
    fn from(error: serde_json::Error) -> Self {
        Self::ObservationEncoding(error)
    }
}

/// Result of one publication and optional generation verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineReceipt {
    /// Durable publication receipt, including the accepted generation.
    pub publication: WorkflowReceipt,
    /// Effective verification status recorded for the accepted generation.
    pub verification: WorkflowStatus,
}

/// Runtime-neutral source of acceptance timestamps for graph publication.
pub trait Clock: Send + Sync {
    /// Return current Unix time in nanoseconds, or an actionable clock error.
    ///
    /// # Errors
    /// Returns an error when the host clock is unavailable or outside the supported range.
    fn now(&self) -> Result<AcceptanceTime, String>;
}

/// System wall clock used by default by embedded engine hosts.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Result<AcceptanceTime, String> {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock precedes Unix epoch: {error}"))?;
        let nanos = u64::try_from(elapsed.as_nanos())
            .map_err(|error| format!("system clock timestamp exceeds u64 nanoseconds: {error}"))?;
        Ok(AcceptanceTime(nanos))
    }
}

/// Current canonical-state summary for one engine scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineStatus {
    /// Current accepted generation manifest.
    pub manifest: GenerationManifest,
    /// Number of file versions in the accepted generation.
    pub files: usize,
    /// Number of canonical nodes in the accepted generation.
    pub nodes: usize,
    /// Number of canonical edges in the accepted generation.
    pub edges: usize,
    /// Number of provenance records in the accepted generation.
    pub provenance: usize,
    /// Result of the read-only logical graph integrity checks.
    pub integrity: EngineIntegrity,
}

/// Logical consistency checks for one accepted graph generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineIntegrity {
    /// Whether canonical nodes and edges reproduce the manifest graph root.
    pub graph_root_matches: bool,
    /// Whether fact provenance references and edge endpoints are present.
    pub references_valid: bool,
}

/// Difference between an accepted index and a host-supplied source inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineIndexFreshness {
    /// Current source files that do not have indexed facts yet.
    pub unindexed_files: usize,
    /// Existing paths whose indexed file version differs from current source.
    pub changed_files: usize,
    /// Indexed source files no longer present in the current inventory.
    pub removed_files: usize,
}

impl EngineIndexFreshness {
    /// Returns whether the indexed file inventory matches the supplied source.
    #[must_use]
    pub const fn is_current(self) -> bool {
        self.unindexed_files == 0 && self.changed_files == 0 && self.removed_files == 0
    }
}

impl EngineIntegrity {
    /// Returns whether every logical integrity check passed.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.graph_root_matches && self.references_valid
    }
}

/// Read-only workflow counts for operator-facing recovery diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineWorkflowDiagnostics {
    /// Prepared operations awaiting reconciliation.
    pub prepared_operations: usize,
    /// Successfully completed operations.
    pub completed_operations: usize,
    /// Terminally rejected operations.
    pub rejected_operations: usize,
}

/// The embeddable SyntaxMesh engine. Hosts provide source files and consume
/// generation-scoped queries; no transport or runtime dependency is required.
pub struct SyntaxMeshEngine<S, E> {
    indexer: Indexer<S, E>,
    verifier: StateChronicleVerifier,
    verification_enabled: bool,
    repository: RepositoryId,
    worktree: WorktreeId,
    clock: Arc<dyn Clock>,
}

impl<S, E> SyntaxMeshEngine<S, E>
where
    S: GraphStore + DurableRecordStore,
    E: LanguageExtractor,
{
    /// Creates an engine with optional verification disabled by default.
    #[must_use]
    pub fn new(store: S, extractor: E, repository: RepositoryId, worktree: WorktreeId) -> Self {
        Self {
            indexer: Indexer::new(store, extractor, repository, worktree),
            verifier: StateChronicleVerifier::new(),
            verification_enabled: false,
            repository,
            worktree,
            clock: Arc::new(SystemClock),
        }
    }

    /// Prepare the shared durable source transition policy for this Engine scope.
    /// Callers supply ordered source/configuration fingerprints and must recover
    /// pending workflows first. A true flag requires publication via `index`.
    /// The persisted marker records planning intent, not publication completion.
    ///
    /// # Errors
    /// Returns store or invalid-marker encoding errors.
    pub fn prepare_source_index(
        &mut self,
        files: &[SourceFile],
        extractor_fingerprint: &[u8],
        source_fingerprint: &[u8],
    ) -> Result<(GenerationId, bool), StoreError> {
        let policy_fingerprint =
            if self.indexer.source_syntax_policy() == SourceSyntaxPolicy::RecordFailures {
                Some(
                    syntaxmesh_core::StableId::derive(
                        "source-syntax-policy-v1",
                        &[extractor_fingerprint, b"record-failures"],
                    )
                    .0,
                )
            } else {
                None
            };
        let effective_fingerprint = policy_fingerprint
            .as_ref()
            .map_or(extractor_fingerprint, |fingerprint| fingerprint.as_slice());
        crate::prepare_source_index(
            self.indexer.store_mut(),
            self.repository,
            self.worktree,
            files,
            effective_fingerprint,
            source_fingerprint,
        )
    }

    /// Consumes the engine and returns its store, discarding engine configuration.
    #[must_use]
    pub fn into_store(self) -> S {
        self.indexer.into_parts().0
    }

    /// Opaque host paths consulted by the installed module resolver.
    /// Hosts own fingerprinting; reading this inventory does not publish facts.
    ///
    /// # Errors
    /// Returns a diagnostic if provider observations cannot be read.
    pub fn observed_module_inputs(&self) -> Result<Vec<String>, String> {
        self.indexer.observed_module_inputs()
    }

    /// Load persisted resolver input coverage for this repository/worktree policy.
    ///
    /// # Errors
    /// Returns store or malformed inventory errors.
    pub fn persisted_module_inputs(
        &self,
        resolver_fingerprint: &[u8],
    ) -> Result<Vec<String>, StoreError> {
        crate::load_module_inputs(
            self.indexer.store(),
            self.repository,
            self.worktree,
            resolver_fingerprint,
        )
    }

    /// Merge installed resolver observations into durable policy-scoped coverage.
    /// Retains earlier paths so provider replacement cannot narrow invalidation.
    /// Hosts must supply the fingerprint of the installed provider policy.
    ///
    /// # Errors
    /// Returns observation, encoding, store, or CAS errors.
    pub fn persist_module_inputs(&mut self, resolver_fingerprint: &[u8]) -> Result<(), StoreError> {
        let mut inputs = self.persisted_module_inputs(resolver_fingerprint)?;
        inputs.extend(self.observed_module_inputs().map_err(StoreError::Backend)?);
        crate::save_module_inputs(
            self.indexer.store_mut(),
            self.repository,
            self.worktree,
            resolver_fingerprint,
            &inputs,
        )
    }

    /// Replaces the system wall clock with a deterministic or host-provided clock.
    #[must_use]
    pub fn with_clock<C: Clock + 'static>(mut self, clock: C) -> Self {
        self.clock = Arc::new(clock);
        self
    }

    /// Injects a host-owned module resolver for typed imports and re-exports.
    /// The default engine remains runtime-neutral and leaves those occurrences unresolved.
    #[must_use]
    pub fn with_module_resolution_provider(
        mut self,
        provider: Arc<dyn ModuleResolutionProvider>,
    ) -> Self {
        self.indexer = self.indexer.with_module_resolution_provider(provider);
        self
    }

    /// Select source syntax handling for future preparation. Defaults to strict.
    /// Hosts must include this policy in planning and report incomplete coverage.
    #[must_use]
    pub fn with_source_syntax_policy(mut self, policy: SourceSyntaxPolicy) -> Self {
        self.indexer = self.indexer.with_source_syntax_policy(policy);
        self
    }

    #[must_use]
    pub const fn source_syntax_policy(&self) -> SourceSyntaxPolicy {
        self.indexer.source_syntax_policy()
    }

    /// Enables StateChronicle verification for future index publications.
    #[must_use]
    pub const fn with_statechronicle_verification(mut self) -> Self {
        self.verification_enabled = true;
        self
    }

    /// Replace source policies under exclusive host ownership for future runs.
    ///
    /// This does not publish, recover, reopen the store, or rewrite history.
    /// Hosts must update their source-planning fingerprints in the same critical
    /// section. Enabling verification does not retroactively verify an existing
    /// unverified base; the normal publication verification rules still apply.
    pub fn reconfigure_source_policies(
        &mut self,
        provider: Option<Arc<dyn ModuleResolutionProvider>>,
        verification_enabled: bool,
    ) {
        self.indexer.set_module_resolution_provider(provider);
        self.verification_enabled = verification_enabled;
    }

    /// Publishes one generation through Penelope and, when enabled, verifies
    /// it through StateChronicle without exposing either integration to callers.
    ///
    /// # Errors
    /// Returns an indexing, publication, or verification error.
    pub fn index(
        &mut self,
        files: &[SourceFile],
        run_id: IndexRunId,
        next_generation: GenerationId,
    ) -> Result<EngineReceipt, EngineError> {
        let delta = self.prepare_source_delta(files, run_id, next_generation)?;
        self.publish_delta(delta)
    }

    /// Prepare source facts without publishing the new graph transition.
    ///
    /// Recovers pending workflows first, which can publish earlier operations.
    /// Hosts hold exclusive writer ownership and may validate observed resolver
    /// inputs before publishing through `publish_prepared_with_lineage`.
    /// Discarding the returned delta does not accept its generation. This is not
    /// a filesystem snapshot or a guarantee that external inputs stayed stable.
    ///
    /// # Errors
    /// Returns recovery, extraction, resolution, or delta construction errors.
    pub fn prepare_source_delta(
        &mut self,
        files: &[SourceFile],
        run_id: IndexRunId,
        next_generation: GenerationId,
    ) -> Result<GraphDelta, EngineError> {
        self.recover_pending_workflows()?;
        Ok(self.indexer.prepare_delta(files, run_id, next_generation)?)
    }

    /// Publishes an already prepared graph transition together with explicit
    /// ChangeSet evidence through Penelope and the configured verifier.
    ///
    /// The Change Engine may prepare this request; SyntaxMesh only validates,
    /// durably accepts, and queries the supplied evidence. It does not own the
    /// mutation workflow that produced the graph delta.
    ///
    /// # Errors
    /// Returns an indexing, publication, or verification error. The request is
    /// rejected if it belongs to another repository/worktree scope.
    pub fn publish_prepared_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
    ) -> Result<EngineReceipt, EngineError> {
        if request.graph.repository != self.repository || request.graph.worktree != self.worktree {
            return Err(EngineError::Store(StoreError::InvalidDelta(
                "prepared delta belongs to another engine repository/worktree".to_owned(),
            )));
        }
        self.recover_pending_workflows()?;
        self.publish_request(GraphDeltaWithConsequences {
            publication: request,
            consequences: ConsequenceDelta::default(),
        })
    }

    /// Publishes a prepared graph transition with explicit historical consequence assertions.
    ///
    /// # Errors
    /// Returns publication or verification errors; rejects requests outside this engine scope.
    pub fn publish_prepared_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
    ) -> Result<EngineReceipt, EngineError> {
        request
            .validate()
            .map_err(|error| EngineError::Store(StoreError::InvalidDelta(error.to_string())))?;
        let graph = &request.publication.graph;
        if graph.repository != self.repository || graph.worktree != self.worktree {
            return Err(EngineError::Store(StoreError::InvalidDelta(
                "prepared delta belongs to another engine repository/worktree".to_owned(),
            )));
        }
        self.recover_pending_workflows()?;
        self.publish_request(request)
    }

    /// Publishes an extension fact batch through the engine's durable workflow.
    ///
    /// The batch's facts and provenance become part of the same canonical graph
    /// generation as existing source facts. Extensions do not receive a store
    /// handle. Runtime observations become distinct runtime-evidence nodes with
    /// preserved protocol payloads; they are never inferred into static edges.
    ///
    /// # Errors
    /// Returns validation, recovery, publication, or verification errors.
    pub fn ingest_extension_facts(
        &mut self,
        batch: FactBatch,
        run_id: IndexRunId,
        next_generation: GenerationId,
    ) -> Result<EngineReceipt, EngineError> {
        batch.validate()?;
        if batch.nodes.is_empty() && batch.edges.is_empty() && batch.observations.is_empty() {
            return Err(EngineError::EmptyExtensionBatch);
        }

        let mut observation_nodes = BTreeMap::new();
        let mut observation_provenance = BTreeMap::new();
        for observation in &batch.observations {
            let bytes = serde_json::to_vec(observation)?;
            let provenance = Provenance {
                id: ProvenanceId::derive(&[
                    b"runtime-observation-v1",
                    observation.producer_namespace.as_bytes(),
                    observation.producer_version.as_bytes(),
                ]),
                producer_namespace: observation.producer_namespace.clone(),
                producer_version: observation.producer_version.clone(),
                evidence_class: EvidenceClass::RuntimeObserved,
                source: None,
            };
            let node = Node {
                id: NodeId::derive(&[b"runtime-observation-v1", &bytes]),
                kind: NodeKind::RuntimeObservation,
                name: format!(
                    "{} {} {}",
                    observation.subject, observation.relation, observation.object
                ),
                owner_file: None,
                source: None,
                provenance: provenance.id,
                extension_payload: Some(ExtensionPayload {
                    namespace: observation.producer_namespace.clone(),
                    schema_version: observation.schema_version,
                    bytes,
                }),
            };
            if observation_nodes
                .get(&node.id)
                .is_some_and(|existing| existing != &node)
                || observation_provenance
                    .get(&provenance.id)
                    .is_some_and(|existing| existing != &provenance)
            {
                return Err(EngineError::RuntimeObservationIdentityConflict);
            }
            observation_nodes.insert(node.id, node);
            observation_provenance.insert(provenance.id, provenance);
        }
        let mut provenance = batch.provenance;
        for runtime_provenance in observation_provenance.into_values() {
            if let Some(existing) = provenance
                .iter()
                .find(|existing| existing.id == runtime_provenance.id)
            {
                if existing != &runtime_provenance {
                    return Err(EngineError::RuntimeObservationIdentityConflict);
                }
            } else {
                provenance.push(runtime_provenance);
            }
        }
        let mut nodes = batch.nodes;
        nodes.extend(observation_nodes.into_values());
        self.recover_pending_workflows()?;
        let expected_base = self
            .indexer
            .store()
            .current_generation(self.repository, self.worktree)?
            .map(|manifest| manifest.generation);
        self.publish_delta(GraphDelta {
            repository: self.repository,
            worktree: self.worktree,
            run_id,
            expected_base,
            next_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: provenance,
            upsert_nodes: nodes,
            upsert_edges: batch.edges,
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })
    }

    fn publish_delta(&mut self, delta: GraphDelta) -> Result<EngineReceipt, EngineError> {
        self.publish_request(GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: delta,
                lineage: syntaxmesh_core::ChangeSetDelta::default(),
            },
            consequences: ConsequenceDelta::default(),
        })
    }

    fn publish_request(
        &mut self,
        request: GraphDeltaWithConsequences,
    ) -> Result<EngineReceipt, EngineError> {
        let accepted_at = self.clock.now().map_err(EngineError::Clock)?;
        let publication = {
            let mut workflow = PenelopePublisher::new(self.indexer.store_mut());
            workflow.publish_with_consequences(request, accepted_at)?
        };
        let verification = if self.verification_enabled {
            self.indexer.store_mut().set_generation_status(
                publication.generation.generation,
                GenerationStatus::VerificationPending,
            )?;
            match self
                .verifier
                .verify(&publication.generation, self.indexer.store_mut())
            {
                Ok(status) => {
                    if status == WorkflowStatus::Verified {
                        self.indexer.store_mut().set_generation_status(
                            publication.generation.generation,
                            GenerationStatus::Verified,
                        )?;
                    }
                    status
                }
                Err(error) => {
                    self.indexer.store_mut().set_generation_status(
                        publication.generation.generation,
                        GenerationStatus::VerificationFailed,
                    )?;
                    return Err(EngineError::Workflow(error));
                }
            }
        } else {
            match publication.generation.status {
                GenerationStatus::Durable => WorkflowStatus::Durable,
                GenerationStatus::VerificationPending => WorkflowStatus::VerificationPending,
                GenerationStatus::Verified => WorkflowStatus::Verified,
                GenerationStatus::VerificationFailed => WorkflowStatus::Failed,
            }
        };
        Ok(EngineReceipt {
            publication,
            verification,
        })
    }

    /// Opens a generation-scoped query view.
    #[must_use]
    pub const fn query(&self, generation: GenerationId) -> Query<'_, S> {
        Query::new(self.indexer.store(), generation)
    }

    /// Returns the latest generation for this repository/worktree scope.
    ///
    /// # Errors
    /// Returns the store error if the scope cannot be read.
    pub fn current_generation(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<syntaxmesh_core::GenerationManifest>, StoreError> {
        self.indexer
            .store()
            .current_generation(repository, worktree)
    }

    /// Replays the complete opt-in StateChronicle chain for this engine scope.
    ///
    /// Routine generation verification checks only the hash-checked head;
    /// hosts can call this explicit, read-only audit when they need to
    /// revalidate every retained verification record. Returns `None` when no
    /// StateChronicle history has been recorded for the scope.
    ///
    /// # Errors
    /// Returns a store or workflow error if current state cannot be read or
    /// any retained history record fails validation.
    pub fn verify_statechronicle_history(&mut self) -> Result<Option<String>, EngineError> {
        let Some(scope) = self
            .indexer
            .store()
            .current_generation(self.repository, self.worktree)?
        else {
            return Ok(None);
        };
        Ok(self.verifier.verify_history(&scope, self.indexer.store())?)
    }

    /// Returns the current generation manifest and canonical fact counts.
    ///
    /// # Errors
    /// Returns a store error if the current snapshot cannot be read.
    pub fn status(&self) -> Result<Option<EngineStatus>, EngineError> {
        let Some(manifest) = self
            .indexer
            .store()
            .current_generation(self.repository, self.worktree)?
        else {
            return Ok(None);
        };
        self.summarize_status(manifest).map(Some)
    }

    /// Read explicit processing coverage at a retained generation in this scope.
    /// Materializes its full historical snapshot; missing evidence is unclassified.
    ///
    /// # Errors
    /// Rejects foreign scopes and malformed processing facts, or returns store errors.
    pub fn source_processing_coverage_at(
        &self,
        generation: GenerationId,
    ) -> Result<crate::SourceProcessingCoverage, EngineError> {
        let manifest = self.indexer.store().manifest(generation)?;
        if manifest.repository != self.repository || manifest.worktree != self.worktree {
            return Err(EngineError::Store(StoreError::Integrity(
                "processing coverage belongs to another engine scope".to_owned(),
            )));
        }
        let snapshot = self.indexer.store().historical_snapshot(generation)?;
        Ok(crate::processing::summarize(generation, &snapshot)?)
    }

    fn summarize_status(&self, manifest: GenerationManifest) -> Result<EngineStatus, EngineError> {
        let generation = manifest.generation;
        let files = self.indexer.store().files(generation)?;
        let nodes = self.indexer.store().nodes(generation)?;
        let edges = self.indexer.store().edges(generation)?;
        let provenance = self.indexer.store().provenance(generation)?;
        let integrity = logical_integrity(&manifest, &files, &nodes, &edges, &provenance)?;
        Ok(EngineStatus {
            manifest,
            files: files.len(),
            nodes: nodes.len(),
            edges: edges.len(),
            provenance: provenance.len(),
            integrity,
        })
    }

    /// Compare indexed file versions with a current host-supplied inventory.
    ///
    /// The caller must provide files from this engine's repository/worktree
    /// scope. The engine does not scan paths or infer freshness from timestamps.
    ///
    /// # Errors
    /// Returns a store error if the current generation or indexed files cannot
    /// be read.
    pub fn index_freshness(
        &self,
        source_files: &[FileVersion],
    ) -> Result<Option<EngineIndexFreshness>, EngineError> {
        let Some(manifest) = self
            .indexer
            .store()
            .current_generation(self.repository, self.worktree)?
        else {
            return Ok(None);
        };
        let indexed_files = self.indexer.store().files(manifest.generation)?;
        Ok(Some(EngineIndexFreshness::compare(
            &indexed_files,
            source_files,
        )))
    }

    /// Inspect durable workflow state without mutating it.
    ///
    /// # Errors
    /// Returns a workflow error if operation records cannot be enumerated or
    /// decoded.
    pub fn workflow_diagnostics(&self) -> Result<EngineWorkflowDiagnostics, EngineError> {
        let diagnostics = PenelopePublisher::<S>::inspect(self.indexer.store())?;
        Ok(EngineWorkflowDiagnostics {
            prepared_operations: diagnostics.prepared_operations,
            completed_operations: diagnostics.completed_operations,
            rejected_operations: diagnostics.rejected_operations,
        })
    }

    /// Reads a bounded page of durable terminal indexing rejections without
    /// recovering or mutating workflow records.
    ///
    /// Results are ordered by run ID. `after` is an exclusive continuation
    /// cursor returned by the preceding page.
    ///
    /// # Errors
    /// Returns an error if durable workflow records cannot be read or decoded.
    pub fn workflow_rejections(
        &self,
        after: Option<IndexRunId>,
        limit: usize,
    ) -> Result<WorkflowRejectionPage, EngineError> {
        Ok(PenelopePublisher::<S>::inspect_rejections(
            self.indexer.store(),
            after,
            limit,
        )?)
    }

    /// Replays and reconciles any incomplete Penelope indexing operations.
    ///
    /// Index publication also runs this recovery before starting a new
    /// operation; hosts may call it at startup to report recovery explicitly.
    ///
    /// # Errors
    /// Returns an indexing-workflow or store error when recovery cannot safely
    /// reconcile a pending operation with the current generation.
    pub fn recover_pending_workflows(&mut self) -> Result<usize, EngineError> {
        let mut workflow = PenelopePublisher::new(self.indexer.store_mut());
        Ok(workflow.recover_pending()?)
    }
}

fn logical_integrity(
    manifest: &GenerationManifest,
    files: &[FileVersion],
    nodes: &[Node],
    edges: &[Edge],
    provenance: &[Provenance],
) -> Result<EngineIntegrity, StoreError> {
    if manifest.schema_version == 2 {
        let snapshot = GraphSnapshot {
            files: files.to_vec(),
            provenance: provenance.to_vec(),
            nodes: nodes.to_vec(),
            edges: edges.to_vec(),
        };
        return Ok(EngineIntegrity {
            graph_root_matches: graph_snapshot_root_v2(manifest.generation, &snapshot)?
                == manifest.graph_root,
            references_valid: references_are_valid(nodes, edges, provenance),
        });
    }
    let mut ordered_nodes: Vec<_> = nodes.iter().collect();
    ordered_nodes.sort_by_key(|node| node.id);
    let mut ordered_edges: Vec<_> = edges.iter().collect();
    ordered_edges.sort_by_key(|edge| edge.id);
    let mut root_bytes = Vec::new();
    for node in &ordered_nodes {
        root_bytes.extend_from_slice(&serde_json::to_vec(node).map_err(|error| {
            StoreError::Backend(format!("serialize node for integrity check: {error}"))
        })?);
    }
    for edge in &ordered_edges {
        root_bytes.extend_from_slice(&serde_json::to_vec(edge).map_err(|error| {
            StoreError::Backend(format!("serialize edge for integrity check: {error}"))
        })?);
    }
    let computed_root =
        blake3::hash(&[manifest.generation.0.0.as_slice(), root_bytes.as_slice()].concat());
    Ok(EngineIntegrity {
        graph_root_matches: computed_root.as_bytes() == &manifest.graph_root,
        references_valid: references_are_valid(nodes, edges, provenance),
    })
}

fn references_are_valid(nodes: &[Node], edges: &[Edge], provenance: &[Provenance]) -> bool {
    let provenance_ids: BTreeSet<_> = provenance.iter().map(|item| item.id).collect();
    let node_ids: BTreeSet<_> = nodes.iter().map(|node| node.id).collect();
    nodes
        .iter()
        .all(|node| provenance_ids.contains(&node.provenance))
        && edges.iter().all(|edge| {
            provenance_ids.contains(&edge.provenance)
                && node_ids.contains(&edge.source)
                && node_ids.contains(&edge.target)
        })
}
