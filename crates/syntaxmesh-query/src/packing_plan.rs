//! Bounded ordered evidence plans without lexical lookup or graph expansion.

use syntaxmesh_api_model::{ContextPack, ContextRequest};
use syntaxmesh_store::GraphStore;

use crate::context::SeedMode;
use crate::{
    ContextSelectionReport, ContextSourceProvider, ContextTokenCounter, Query, QueryError,
};

impl<S: GraphStore + ?Sized> Query<'_, S> {
    /// Inspect an ordered plan of at most max_candidates IDs, with zero hops.
    ///
    /// # Errors
    /// Returns invalid-plan, unknown-ID and ordinary generation/read errors.
    pub fn ranked_plan_context_selection(
        &self,
        request: &ContextRequest,
    ) -> Result<ContextSelectionReport, QueryError> {
        crate::context::selection_report(self, request, false, SeedMode::RankedPlan)
    }

    /// Inspect the same bounded plan in a retained generation.
    ///
    /// # Errors
    /// Returns invalid-plan, unknown-ID and retained-generation/read errors.
    pub fn historical_ranked_plan_context_selection(
        &self,
        request: &ContextRequest,
    ) -> Result<ContextSelectionReport, QueryError> {
        crate::context::selection_report(self, request, true, SeedMode::RankedPlan)
    }

    /// Pack an ordered selected set without expanding it. All source validation
    /// and exact tokenizer budgeting remain in the shared compiler.
    ///
    /// # Errors
    /// Returns invalid-plan, ID, source, tokenizer and budget errors.
    pub fn ranked_plan_context(
        &self,
        request: &ContextRequest,
        source: &impl ContextSourceProvider,
        counter: &impl ContextTokenCounter,
    ) -> Result<ContextPack, QueryError> {
        crate::context::compile_seeded_context(
            self,
            request,
            source,
            counter,
            false,
            SeedMode::RankedPlan,
        )
    }

    /// Pack the same plan with retained-generation source verification.
    ///
    /// # Errors
    /// Returns invalid-plan and ordinary historical compiler errors.
    pub fn historical_ranked_plan_context(
        &self,
        request: &ContextRequest,
        source: &impl ContextSourceProvider,
        counter: &impl ContextTokenCounter,
    ) -> Result<ContextPack, QueryError> {
        crate::context::compile_seeded_context(
            self,
            request,
            source,
            counter,
            true,
            SeedMode::RankedPlan,
        )
    }
}
