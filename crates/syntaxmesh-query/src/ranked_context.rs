//! Additive ordered-plan composition; all work remains in the shared compiler.

use crate::{
    ContextSelectionReport, ContextSourceProvider, ContextTokenCounter, Query, QueryError,
};
use syntaxmesh_api_model::{ContextPack, ContextRequest};
use syntaxmesh_store::GraphStore;

impl<S: GraphStore + ?Sized> Query<'_, S> {
    /// Inspect ordered explicit seeds without lexical lookup. Duplicate IDs keep
    /// first-distinct priority; neighbors keep zero relevance.
    ///
    /// # Errors
    /// Returns ordinary request, unknown-seed and generation/read errors.
    pub fn ranked_seeded_context_selection(
        &self,
        request: &ContextRequest,
    ) -> Result<ContextSelectionReport, QueryError> {
        crate::context::selection_report(
            self,
            request,
            false,
            crate::context::SeedMode::RankedExplicit,
        )
    }

    /// Inspect the same ordered plan in a retained generation.
    ///
    /// # Errors
    /// Returns ordinary request, unknown-seed and retained-generation/read errors.
    pub fn historical_ranked_seeded_context_selection(
        &self,
        request: &ContextRequest,
    ) -> Result<ContextSelectionReport, QueryError> {
        crate::context::selection_report(
            self,
            request,
            true,
            crate::context::SeedMode::RankedExplicit,
        )
    }

    /// Pack an ordered explicit plan using the existing source-first exact-budget
    /// compiler. Priority is caller intent, not a relevance probability.
    ///
    /// # Errors
    /// Returns ordinary request, seed, source, tokenizer and budget errors.
    pub fn ranked_seeded_context(
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
            crate::context::SeedMode::RankedExplicit,
        )
    }

    /// Pack an ordered plan with hash-verified retained-generation source bytes.
    ///
    /// # Errors
    /// Returns ordinary historical request, seed, source, tokenizer and budget errors.
    pub fn historical_ranked_seeded_context(
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
            crate::context::SeedMode::RankedExplicit,
        )
    }
}
