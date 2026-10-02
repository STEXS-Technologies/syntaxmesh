//! Thin generation-pinned access to the existing indexed lexical composer.

use crate::{
    DiscoveryPackingPreference, GenerationIdentifierIndex, LexicalPlanRequest, Query, QueryError,
    SourceRolePackingRequest,
};
use std::collections::BTreeSet;
use syntaxmesh_core::NodeId;
use syntaxmesh_store::GraphStore;

impl<S: GraphStore + ?Sized> Query<'_, S> {
    /// Order lexical seeds before discoveries only when explicitly requested.
    ///
    /// # Errors
    /// Preserves index identity, membership/capacity and posting-work checks.
    pub fn lexical_packing_plan_with_discovery(
        &self,
        index: &GenerationIdentifierIndex,
        request: &LexicalPlanRequest<'_>,
        roles: &SourceRolePackingRequest<'_>,
        preference: DiscoveryPackingPreference,
    ) -> Result<Vec<NodeId>, QueryError> {
        index.lexical_packing_plan_with_discovery(
            self.store,
            self.generation,
            request,
            roles,
            preference,
        )
    }
    /// Build an explicit generation-pinned source-content planner index.
    ///
    /// # Errors
    /// Preserves canonical build budgets and bounded historical scan failures.
    pub fn build_source_content_planner_index(
        &self,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
    ) -> Result<GenerationIdentifierIndex, QueryError> {
        GenerationIdentifierIndex::build_source_content_for_planner(
            self.store,
            self.generation,
            node_budget,
            posting_budget,
            payload_budget,
        )
    }

    /// Order the complete plan using explicit, generation-pinned role evidence.
    ///
    /// # Errors
    /// Preserves index identity, membership/capacity and posting-work checks.
    pub fn lexical_packing_plan_with_roles(
        &self,
        index: &GenerationIdentifierIndex,
        request: &LexicalPlanRequest<'_>,
        roles: &SourceRolePackingRequest<'_>,
    ) -> Result<Vec<NodeId>, QueryError> {
        index.lexical_packing_plan_with_roles(self.store, self.generation, request, roles)
    }
    /// Build an opt-in exact/stemmed index with family metadata.
    /// Budgets retain the index's logical node, posting and payload accounting.
    ///
    /// # Errors
    /// Rejects exhausted budgets, unavailable generations and malformed pages.
    pub fn build_planner_identifier_index(
        &self,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
    ) -> Result<GenerationIdentifierIndex, QueryError> {
        GenerationIdentifierIndex::build_for_planner(
            self.store,
            self.generation,
            node_budget,
            posting_budget,
            payload_budget,
        )
    }

    /// Compose bounded seed IDs without canonical payload hydration.
    ///
    /// # Errors
    /// Rejects foreign indexes, missing capability and exhausted posting work.
    pub fn lexical_seed_plan(
        &self,
        index: &GenerationIdentifierIndex,
        request: &LexicalPlanRequest<'_>,
    ) -> Result<Vec<NodeId>, QueryError> {
        index.lexical_seed_plan(self.store, self.generation, request)
    }

    /// Order the complete selected set without another graph traversal.
    ///
    /// # Errors
    /// Rejects foreign indexes, invalid membership/capacity and exhausted work.
    pub fn lexical_packing_plan(
        &self,
        index: &GenerationIdentifierIndex,
        request: &LexicalPlanRequest<'_>,
        selected: &BTreeSet<NodeId>,
        original: &BTreeSet<NodeId>,
    ) -> Result<Vec<NodeId>, QueryError> {
        index.lexical_packing_plan(self.store, self.generation, request, selected, original)
    }
}
