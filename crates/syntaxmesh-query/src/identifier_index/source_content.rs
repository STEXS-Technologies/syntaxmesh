use super::{GenerationIdentifierIndex, charged_payload};
use crate::{ContextSourceProvider, QueryError, identifier_terms};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{FileId, GenerationId, NodeId, NodeKind, SourceSpan};
use syntaxmesh_language_sdk::SOURCE_PROCESSING_NAMESPACE;
use syntaxmesh_store::GraphStore;

/// Independent logical bounds for an opt-in source-span lexical index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpanIndexLimits {
    pub node_budget: usize,
    pub posting_budget: usize,
    pub payload_budget: usize,
    pub source_span_budget: usize,
    pub source_byte_budget: usize,
    pub source_posting_budget: usize,
    pub source_payload_budget: usize,
}

impl GenerationIdentifierIndex {
    /// Build a source-span-augmented planner index through a host-provided,
    /// hash-checked source reader. Source limits are additional to the ordinary
    /// canonical-index limits and are charged independently.
    ///
    /// # Errors
    /// Rejects invalid/exhausted limits, absent or stale source, invalid UTF-8
    /// or spans, unavailable historical files, and malformed graph scans.
    pub fn build_source_span_planner<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
        source: &impl ContextSourceProvider,
        limits: SourceSpanIndexLimits,
    ) -> Result<Self, QueryError> {
        if limits.source_span_budget == 0
            || limits.source_byte_budget == 0
            || limits.source_posting_budget == 0
            || limits.source_payload_budget == 0
        {
            return Err(QueryError::InvalidLimit);
        }
        #[cfg(feature = "benchmark-instrumentation")]
        let build_started = std::time::Instant::now();
        let mut index = Self::build_source_content_for_planner(
            store,
            generation,
            limits.node_budget,
            limits.posting_budget,
            limits.payload_budget,
        )?;
        let mut spans = BTreeMap::<FileId, Vec<(NodeId, SourceSpan, [u8; 32])>>::new();
        let mut span_count = 0_usize;
        let mut source_payload = 0_usize;
        let mut callback_error = None;
        let visit_result =
            store.visit_historical_nodes(generation, limits.node_budget, &mut |node| {
                if let (Some(location), Some(planner)) = (node.source.as_ref(), &mut index.planner)
                    && let Entry::Vacant(entry) = planner.source_files.entry(node.id)
                {
                    source_payload =
                        match charged_payload(source_payload, 32, limits.source_payload_budget) {
                            Ok(value) => value,
                            Err(error) => {
                                callback_error = Some(error);
                                return Err(syntaxmesh_store::StoreError::Integrity(
                                    "source ownership payload budget exhausted".to_owned(),
                                ));
                            }
                        };
                    entry.insert(location.file_id);
                }
                if !source_candidate(&node.kind) {
                    return Ok(());
                }
                let Some(location) = node.source else {
                    return Ok(());
                };
                span_count = match span_count.checked_add(1) {
                    Some(value) if value <= limits.source_span_budget => value,
                    _ => {
                        callback_error = Some(QueryError::Context(
                            "source span budget exhausted".to_owned(),
                        ));
                        return Err(syntaxmesh_store::StoreError::Integrity(
                            "source span scan stopped".to_owned(),
                        ));
                    }
                };
                spans.entry(location.file_id).or_default().push((
                    node.id,
                    location.span,
                    location.content_hash,
                ));
                Ok(())
            });
        if let Some(error) = callback_error {
            return Err(error);
        }
        let visited = visit_result?;
        if visited > limits.node_budget {
            return Err(QueryError::Context(
                "source node budget exhausted".to_owned(),
            ));
        }

        let mut source_bytes = 0_usize;
        let mut source_postings = 0_usize;
        #[cfg(feature = "benchmark-instrumentation")]
        let source_files = spans.len();
        for (file_id, facts) in spans {
            let file = store.historical_file(generation, file_id)?.ok_or_else(|| {
                QueryError::Context("source file missing from generation".to_owned())
            })?;
            let expected_size = usize::try_from(file.size_bytes)
                .map_err(|error| QueryError::Context(error.to_string()))?;
            if expected_size > limits.source_byte_budget.saturating_sub(source_bytes) {
                return Err(QueryError::Context(
                    "source byte budget exhausted".to_owned(),
                ));
            }
            if facts.iter().any(|(_, _, hash)| hash != &file.content_hash) {
                return Err(QueryError::Context(
                    "source span hash differs from the pinned file version".to_owned(),
                ));
            }
            let bytes = source
                .read_source(&file)
                .map_err(QueryError::Context)?
                .ok_or_else(|| {
                    QueryError::Context("source provider returned no file content".to_owned())
                })?;
            source_bytes = source_bytes
                .checked_add(bytes.len())
                .filter(|value| *value <= limits.source_byte_budget)
                .ok_or_else(|| QueryError::Context("source byte budget exhausted".to_owned()))?;
            if u64::try_from(bytes.len()).ok() != Some(file.size_bytes)
                || blake3::hash(&bytes).as_bytes() != &file.content_hash
            {
                return Err(QueryError::Context(
                    "source bytes do not match the pinned file version".to_owned(),
                ));
            }
            let text = std::str::from_utf8(&bytes)
                .map_err(|error| QueryError::Context(error.to_string()))?;
            for (id, span, _) in facts {
                let start = usize::try_from(span.start_byte)
                    .map_err(|error| QueryError::Context(error.to_string()))?;
                let end = usize::try_from(span.end_byte)
                    .map_err(|error| QueryError::Context(error.to_string()))?;
                let body = text.get(start..end).ok_or_else(|| {
                    QueryError::Context("source span is outside UTF-8 boundaries".to_owned())
                })?;
                let terms = identifier_terms(body);
                for term in &terms {
                    insert_source_posting(
                        &mut index,
                        id,
                        term,
                        &mut source_postings,
                        &mut source_payload,
                        limits.source_posting_budget,
                        limits.source_payload_budget,
                    )?;
                }
                if let Some(planner) = &mut index.planner {
                    planner.insert_source_terms(
                        id,
                        &terms,
                        &mut source_postings,
                        &mut source_payload,
                        limits.source_posting_budget,
                        limits.source_payload_budget,
                    )?;
                }
            }
        }
        #[cfg(feature = "benchmark-instrumentation")]
        {
            let elapsed = build_started.elapsed();
            let base_elapsed = index.build_metrics.elapsed;
            index.build_metrics.node_scans = index.build_metrics.node_scans.saturating_add(2);
            index.build_metrics.postings =
                index.build_metrics.postings.saturating_add(source_postings);
            index.build_metrics.payload_bytes = index
                .build_metrics
                .payload_bytes
                .saturating_add(source_payload);
            index.build_metrics.source_spans = span_count;
            index.build_metrics.source_files = source_files;
            index.build_metrics.source_bytes = source_bytes;
            index.build_metrics.source_elapsed = elapsed.saturating_sub(base_elapsed);
            index.build_metrics.elapsed = index
                .build_metrics
                .elapsed
                .saturating_add(elapsed.saturating_sub(base_elapsed));
        }
        index.source_span_augmented = true;
        Ok(index)
    }

    /// Build an explicit source-content index without processing metadata candidates.
    /// Canonical build budgets include excluded facts. A second bounded scan
    /// selects reserved metadata IDs; this is not a constant-time operation.
    /// Raw store search and direct-ID reads are unaffected.
    ///
    /// # Errors
    /// Returns the ordinary build/budget errors or a historical scan error.
    pub fn build_source_content<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
    ) -> Result<Self, QueryError> {
        let index = Self::build(
            store,
            generation,
            node_budget,
            posting_budget,
            payload_budget,
        )?;
        index.without_processing_candidates(store, generation, node_budget)
    }

    /// Build explicit source-content exact/stemmed channels and planner metadata.
    /// All canonical build work remains budgeted, including excluded evidence.
    ///
    /// # Errors
    /// Returns canonical build errors or a bounded historical scan error.
    pub fn build_source_content_for_planner<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
    ) -> Result<Self, QueryError> {
        let index = Self::build_for_planner(
            store,
            generation,
            node_budget,
            posting_budget,
            payload_budget,
        )?;
        index.without_processing_candidates(store, generation, node_budget)
    }

    fn without_processing_candidates<S: GraphStore + ?Sized>(
        mut self,
        store: &S,
        generation: GenerationId,
        node_budget: usize,
    ) -> Result<Self, QueryError> {
        let mut excluded = BTreeSet::new();
        store.visit_historical_nodes(generation, node_budget, &mut |node| {
            if matches!(&node.kind, NodeKind::External { namespace, .. }
                if namespace == SOURCE_PROCESSING_NAMESPACE)
            {
                excluded.insert(node.id);
            }
            Ok(())
        })?;
        for postings in self.postings.values_mut() {
            postings.retain(|id| !excluded.contains(id));
        }
        self.postings.retain(|_, postings| !postings.is_empty());
        for postings in self.source_postings.values_mut() {
            postings.retain(|id| !excluded.contains(id));
        }
        self.source_postings
            .retain(|_, postings| !postings.is_empty());
        if let Some(planner) = &mut self.planner {
            planner.remove_candidates(&excluded);
        }
        Ok(self)
    }
}

const fn source_candidate(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Function
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::Test
            | NodeKind::Class
            | NodeKind::Import { .. }
            | NodeKind::Export { .. }
            | NodeKind::Section
            | NodeKind::DocumentChunk
    )
}

fn insert_source_posting(
    index: &mut GenerationIdentifierIndex,
    id: NodeId,
    term: &str,
    count: &mut usize,
    payload: &mut usize,
    posting_limit: usize,
    payload_limit: usize,
) -> Result<(), QueryError> {
    if index
        .postings
        .get(term)
        .is_some_and(|postings| postings.binary_search(&id).is_ok())
    {
        return Ok(());
    }
    let postings = index.source_postings.entry(term.to_owned()).or_default();
    match postings.binary_search(&id) {
        Ok(_) => Ok(()),
        Err(position) => {
            if *count >= posting_limit {
                return Err(QueryError::Context(
                    "source index posting budget exhausted".to_owned(),
                ));
            }
            let term_bytes = if postings.is_empty() { term.len() } else { 0 };
            *payload = charged_payload(*payload, term_bytes, payload_limit)?;
            postings.insert(position, id);
            *count = count.saturating_add(1);
            Ok(())
        }
    }
}
