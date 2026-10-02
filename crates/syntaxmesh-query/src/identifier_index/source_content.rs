use super::GenerationIdentifierIndex;
use crate::QueryError;
use std::collections::BTreeSet;
use syntaxmesh_core::{GenerationId, NodeKind};
use syntaxmesh_language_sdk::SOURCE_PROCESSING_NAMESPACE;
use syntaxmesh_store::GraphStore;

impl GenerationIdentifierIndex {
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
        if let Some(planner) = &mut self.planner {
            planner.remove_candidates(&excluded);
        }
        Ok(self)
    }
}
