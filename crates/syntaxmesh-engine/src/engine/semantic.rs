//! Generation-pinned semantic input, durable enrichment, and atomic replacement.

use std::collections::{BTreeMap, BTreeSet};

use syntaxmesh_core::{
    EvidenceClass, FileId, GenerationId, GraphDelta, IndexRunId, NodeId, NodeKind, RelationKind,
};
use syntaxmesh_extension_sdk::{Capability, ExtensionError, FactBatch};
use syntaxmesh_integration_penelope::PenelopeSemanticEnricher;
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_query::Query;
use syntaxmesh_semantic::{
    SEMANTIC_NAMESPACE, SemanticDocumentChunk, SemanticOutput, SemanticProvider,
    SemanticProviderIdentity, SemanticRequest,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};

use super::{EngineError, EngineReceipt, SemanticBatchLimits, SyntaxMeshEngine};

impl<S, E> SyntaxMeshEngine<S, E>
where
    S: GraphStore + DurableRecordStore,
    E: LanguageExtractor,
{
    /// Assemble a source-grounded semantic request from one indexed document
    /// file at the requested graph generation.
    ///
    /// This reads the existing per-file node index and reuses the exact text
    /// stored in `DocumentChunk` facts; it does not rescan the filesystem or
    /// parse Markdown a second time. Callers normally pass the result to
    /// [`Self::enrich_document_semantics`].
    ///
    /// # Errors
    /// Returns a query/store error if the generation or file facts are
    /// unavailable, or a semantic error if the file has no valid document
    /// chunks.
    pub fn semantic_request_for_file(
        &self,
        generation: GenerationId,
        file_id: FileId,
        identity: SemanticProviderIdentity,
    ) -> Result<SemanticRequest, EngineError> {
        let chunks = self.semantic_chunks_for_file(generation, file_id)?;
        Ok(SemanticRequest::new(identity, chunks)?)
    }

    /// Assemble deterministic, bounded semantic requests across all documents
    /// in one accepted generation. Partition inputs by normalized directory,
    /// file path, and source span (with node ID as a tie-breaker);
    /// each request can therefore express evidence spanning multiple documents
    /// without making one unbounded provider call. No provider is invoked.
    ///
    /// # Errors
    /// Returns a query/store/semantic error for malformed source facts, or
    /// [`EngineError::InvalidSemanticBatchLimits`] if a limit is zero or an
    /// individual chunk plus headings exceeds `max_bytes`.
    pub fn semantic_requests_for_generation(
        &self,
        generation: GenerationId,
        identity: &SemanticProviderIdentity,
        limits: SemanticBatchLimits,
    ) -> Result<Vec<SemanticRequest>, EngineError> {
        if limits.max_chunks == 0 || limits.max_bytes == 0 {
            return Err(EngineError::InvalidSemanticBatchLimits);
        }
        let mut files = self.indexer.store().files(generation)?;
        files.sort_by(|left, right| {
            semantic_parent_directory(&left.normalized_path)
                .cmp(semantic_parent_directory(&right.normalized_path))
                .then_with(|| left.normalized_path.cmp(&right.normalized_path))
                .then_with(|| left.file_id.cmp(&right.file_id))
        });
        let mut all_chunks = Vec::new();
        for file in files {
            all_chunks.extend(self.semantic_chunks_for_file(generation, file.file_id)?);
        }
        bounded_semantic_requests(identity, all_chunks, limits)
    }

    /// Assemble bounded semantic requests independently for each document in
    /// one accepted generation. Requests never mix file ownership, so a
    /// changed document invalidates only its own Penelope cache entries. Large
    /// documents are split into bounded deterministic requests. Hosts can
    /// still batch the resulting independent requests into multi-document
    /// provider calls.
    ///
    /// # Errors
    /// Returns a query/store/semantic error for malformed source facts, or
    /// [`EngineError::InvalidSemanticBatchLimits`] if a limit is zero or an
    /// individual chunk plus headings exceeds `max_bytes`.
    pub fn semantic_document_requests_for_generation(
        &self,
        generation: GenerationId,
        identity: &SemanticProviderIdentity,
        limits: SemanticBatchLimits,
    ) -> Result<Vec<SemanticRequest>, EngineError> {
        if limits.max_chunks == 0 || limits.max_bytes == 0 {
            return Err(EngineError::InvalidSemanticBatchLimits);
        }
        let mut files = self.indexer.store().files(generation)?;
        files.sort_by(|left, right| left.normalized_path.cmp(&right.normalized_path));
        let mut requests = Vec::new();
        for file in files {
            let chunks = self.semantic_chunks_for_file(generation, file.file_id)?;
            requests.extend(bounded_semantic_requests(identity, chunks, limits)?);
        }
        Ok(requests)
    }

    fn semantic_chunks_for_file(
        &self,
        generation: GenerationId,
        file_id: FileId,
    ) -> Result<Vec<SemanticDocumentChunk>, EngineError> {
        let query = self.query(generation);
        let node_ids = self.indexer.store().nodes_for_file(generation, file_id)?;
        let mut chunks = Vec::new();
        for node_id in node_ids {
            let Some(node) = query.node(node_id)? else {
                return Err(EngineError::Store(StoreError::Integrity(
                    "file index refers to a node missing from its generation".to_owned(),
                )));
            };
            if node.kind == NodeKind::DocumentChunk {
                let context = semantic_section_context(&query, node_id, file_id)?;
                chunks.push(SemanticDocumentChunk {
                    text: node.name.clone(),
                    node,
                    context,
                });
            }
        }
        chunks.sort_by_key(|chunk| {
            chunk
                .node
                .source
                .as_ref()
                .map(|source| (source.span.start_byte, source.span.end_byte, chunk.node.id))
        });
        Ok(chunks)
    }

    /// Generate or reuse source-grounded semantic facts for an explicit request.
    ///
    /// Semantic enrichment is opt-in: deterministic indexing does not call a
    /// provider or enable network access. The provider and its configuration
    /// are supplied by the host. Completed output is journaled through
    /// Penelope and returned as a normal extension fact batch for explicit
    /// publication with [`Self::ingest_extension_facts`].
    ///
    /// # Errors
    /// Returns a semantic provider, evidence validation, recovery, or durable
    /// workflow error. A rejected semantic result does not mutate graph facts.
    pub fn enrich_document_semantics<P: SemanticProvider>(
        &mut self,
        request: SemanticRequest,
        provider: &P,
    ) -> Result<FactBatch, EngineError> {
        self.recover_pending_workflows()?;
        let store = self.indexer.store_mut();
        let mut enricher = PenelopeSemanticEnricher::new(store);
        enricher.recover_pending(provider)?;
        Ok(enricher.enrich(request, provider)?)
    }

    /// Generate/reuse multiple bounded semantic requests as one durable
    /// operation. Penelope prepares every cache/job record before the provider
    /// executes any missing request, and results preserve request order.
    ///
    /// # Errors
    /// Returns a provider, evidence-validation, recovery, or durable-workflow
    /// error. Completed requests remain cached if another request fails.
    pub fn enrich_document_semantics_many<P: SemanticProvider>(
        &mut self,
        requests: Vec<SemanticRequest>,
        provider: &P,
    ) -> Result<FactBatch, EngineError> {
        self.recover_pending_workflows()?;
        let store = self.indexer.store_mut();
        let mut enricher = PenelopeSemanticEnricher::new(store);
        Ok(enricher.enrich_many(requests, provider)?)
    }

    /// Return grounded, durably cached claims for explicit semantic composition.
    /// The returned request binds the output to its complete pinned source set;
    /// callers must validate merged output before publishing graph facts.
    ///
    /// # Errors
    /// Returns provider, grounding, workflow, or cache errors. No graph is
    /// published; completed requests remain reusable after another one fails.
    pub fn enrich_document_semantic_outputs_many<P: SemanticProvider>(
        &mut self,
        requests: Vec<SemanticRequest>,
        provider: &P,
    ) -> Result<(SemanticRequest, SemanticOutput), EngineError> {
        self.recover_pending_workflows()?;
        let store = self.indexer.store_mut();
        let mut enricher = PenelopeSemanticEnricher::new(store);
        Ok(enricher.enrich_outputs_many(requests, provider)?)
    }

    /// Retry prepared semantic work compatible with the supplied provider.
    ///
    /// Prepared jobs for other provider/model configurations are left intact
    /// for an adapter with the matching identity.
    ///
    /// # Errors
    /// Returns a durable-record, provider, or evidence-validation error.
    pub fn recover_pending_semantic_workflows<P: SemanticProvider>(
        &mut self,
        provider: &P,
    ) -> Result<usize, EngineError> {
        let store = self.indexer.store_mut();
        let mut enricher = PenelopeSemanticEnricher::new(store);
        Ok(enricher.recover_pending(provider)?)
    }

    /// Atomically replace the current source-grounded semantic fact layer.
    /// Historical graph generations continue to retain earlier semantic facts.
    /// The reserved semantic namespace is the only layer this operation may
    /// retract; deterministic and other extension facts are preserved.
    ///
    /// Returns `None` when the current semantic fact set exactly matches the
    /// validated batch. An empty batch explicitly clears the current layer.
    ///
    /// # Errors
    /// Returns validation, recovery, publication, or verification errors.
    pub fn replace_semantic_facts(
        &mut self,
        batch: FactBatch,
        run_id: IndexRunId,
        next_generation: GenerationId,
    ) -> Result<Option<EngineReceipt>, EngineError> {
        batch.validate()?;
        if batch.manifest.namespace != SEMANTIC_NAMESPACE
            || !batch
                .manifest
                .capabilities
                .contains(&Capability::AnalysisFacts)
            || !batch.observations.is_empty()
        {
            return Err(EngineError::Extension(ExtensionError::NamespaceMismatch));
        }
        self.recover_pending_workflows()?;
        let store = self.indexer.store();
        let Some(current) = store.current_generation(self.repository, self.worktree)? else {
            return Err(EngineError::Store(StoreError::InvalidDelta(
                "semantic facts require an accepted source generation".to_owned(),
            )));
        };
        let current_nodes = store.nodes(current.generation)?;
        let current_edges = store.edges(current.generation)?;
        let current_provenance = store.provenance(current.generation)?;
        let semantic_provenance_ids = current_provenance
            .iter()
            .filter(|provenance| {
                provenance.producer_namespace == SEMANTIC_NAMESPACE
                    && provenance.evidence_class == EvidenceClass::SemanticInference
            })
            .map(|provenance| provenance.id)
            .collect::<BTreeSet<_>>();
        let semantic_nodes = current_nodes
            .into_iter()
            .filter(|node| {
                (matches!(&node.kind, NodeKind::External { namespace, .. }
                    if namespace == SEMANTIC_NAMESPACE)
                    || node
                        .extension_payload
                        .as_ref()
                        .is_some_and(|payload| payload.namespace == SEMANTIC_NAMESPACE))
                    && semantic_provenance_ids.contains(&node.provenance)
            })
            .collect::<Vec<_>>();
        let semantic_edges = current_edges
            .into_iter()
            .filter(|edge| {
                matches!(&edge.relation, RelationKind::External { namespace, .. }
                    if namespace == SEMANTIC_NAMESPACE)
            })
            .collect::<Vec<_>>();
        // Provenance is retained even after its facts are replaced. Only the
        // requested records participate in replacement equality; old unreferenced
        // records must not turn a cached run into another accepted generation.
        let requested_provenance_ids = batch
            .provenance
            .iter()
            .map(|record| record.id)
            .collect::<BTreeSet<_>>();
        let semantic_provenance = current_provenance
            .into_iter()
            .filter(|provenance| requested_provenance_ids.contains(&provenance.id))
            .collect::<Vec<_>>();

        let mut old_nodes = semantic_nodes;
        old_nodes.sort_by_key(|node| node.id);
        let mut old_edges = semantic_edges;
        old_edges.sort_by_key(|edge| edge.id);
        let mut old_provenance = semantic_provenance;
        old_provenance.sort_by_key(|provenance| provenance.id);
        let mut new_nodes = batch.nodes.clone();
        new_nodes.sort_by_key(|node| node.id);
        let mut new_edges = batch.edges.clone();
        new_edges.sort_by_key(|edge| edge.id);
        let mut new_provenance = batch.provenance.clone();
        new_provenance.sort_by_key(|provenance| provenance.id);
        if old_nodes == new_nodes && old_edges == new_edges && old_provenance == new_provenance {
            return Ok(None);
        }

        let replacement_node_ids = new_nodes
            .iter()
            .map(|node| node.id)
            .collect::<BTreeSet<_>>();
        let replacement_edge_ids = new_edges
            .iter()
            .map(|edge| edge.id)
            .collect::<BTreeSet<_>>();
        let remove_nodes = old_nodes
            .iter()
            .map(|node| node.id)
            .filter(|id| !replacement_node_ids.contains(id))
            .collect::<Vec<_>>();
        let mut remove_edge_ids = old_edges
            .iter()
            .map(|edge| edge.id)
            .filter(|id| !replacement_edge_ids.contains(id))
            .collect::<Vec<_>>();
        let removed_node_ids = remove_nodes.iter().copied().collect::<BTreeSet<_>>();
        remove_edge_ids.extend(
            store
                .edges(current.generation)?
                .into_iter()
                .filter(|edge| {
                    removed_node_ids.contains(&edge.source)
                        || removed_node_ids.contains(&edge.target)
                })
                .map(|edge| edge.id),
        );
        remove_edge_ids.sort_unstable();
        remove_edge_ids.dedup();
        self.publish_delta(GraphDelta {
            repository: self.repository,
            worktree: self.worktree,
            run_id,
            expected_base: Some(current.generation),
            next_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: batch.provenance,
            upsert_nodes: batch.nodes,
            upsert_edges: batch.edges,
            remove_nodes,
            remove_edges: remove_edge_ids,
        })
        .map(Some)
    }
}

fn semantic_parent_directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn bounded_semantic_requests(
    identity: &SemanticProviderIdentity,
    chunks: Vec<SemanticDocumentChunk>,
    limits: SemanticBatchLimits,
) -> Result<Vec<SemanticRequest>, EngineError> {
    let mut requests = Vec::new();
    let mut batch = Vec::new();
    let mut batch_bytes = 0_usize;
    for chunk in chunks {
        let chunk_bytes = chunk
            .text
            .len()
            .saturating_add(chunk.context.iter().map(String::len).sum::<usize>());
        if chunk_bytes > limits.max_bytes {
            return Err(EngineError::InvalidSemanticBatchLimits);
        }
        if !batch.is_empty()
            && (batch.len() == limits.max_chunks
                || batch_bytes.saturating_add(chunk_bytes) > limits.max_bytes)
        {
            requests.push(SemanticRequest::new(
                identity.clone(),
                std::mem::take(&mut batch),
            )?);
            batch_bytes = 0;
        }
        batch_bytes = batch_bytes.saturating_add(chunk_bytes);
        batch.push(chunk);
    }
    if !batch.is_empty() {
        requests.push(SemanticRequest::new(identity.clone(), batch)?);
    }
    Ok(requests)
}

fn semantic_section_context<S: GraphStore + ?Sized>(
    query: &Query<'_, S>,
    chunk_id: NodeId,
    file_id: FileId,
) -> Result<Vec<String>, EngineError> {
    let mut context = Vec::new();
    let mut visited = BTreeSet::from([chunk_id]);
    let mut current = chunk_id;
    loop {
        let mut parents = BTreeMap::new();
        for edge in query.incoming_edges(current)? {
            if edge.relation != RelationKind::Contains {
                continue;
            }
            if !visited.insert(edge.source) {
                return Err(EngineError::Store(StoreError::Integrity(
                    "document containment hierarchy contains a cycle".to_owned(),
                )));
            }
            let parent = query.node(edge.source)?.ok_or_else(|| {
                EngineError::Store(StoreError::Integrity(
                    "document containment edge refers to a missing parent".to_owned(),
                ))
            })?;
            if parent.owner_file != Some(file_id) {
                return Err(EngineError::Store(StoreError::Integrity(
                    "document containment parent belongs to another file".to_owned(),
                )));
            }
            parents.insert(parent.id, parent);
        }
        match parents.len() {
            0 => {
                let level = if current == chunk_id {
                    "chunk"
                } else {
                    "section"
                };
                return Err(EngineError::Store(StoreError::Integrity(format!(
                    "document {level} has no indexed containment parent"
                ))));
            }
            1 => {}
            _ => {
                return Err(EngineError::Store(StoreError::Integrity(
                    "document node has multiple containment parents".to_owned(),
                )));
            }
        }
        let parent = parents.into_values().next().ok_or_else(|| {
            EngineError::Store(StoreError::Integrity(
                "document containment parent disappeared".to_owned(),
            ))
        })?;
        if parent.kind == NodeKind::Section {
            context.push(parent.name);
            current = parent.id;
        } else if parent.kind == NodeKind::Document {
            if query
                .incoming_edges(parent.id)?
                .iter()
                .any(|edge| edge.relation == RelationKind::Contains)
            {
                return Err(EngineError::Store(StoreError::Integrity(
                    "document root has an invalid containment parent".to_owned(),
                )));
            }
            context.reverse();
            return Ok(context);
        } else {
            return Err(EngineError::Store(StoreError::Integrity(
                "document containment hierarchy has an invalid parent kind".to_owned(),
            )));
        }
    }
}
