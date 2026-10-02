//! Generation-scoped query services over the store port.

mod context;
mod diagnostics;
mod historical_neighbor_export;
mod identifier_index;
mod identifier_normalization;
mod packing_plan;
mod ranked_candidates;
mod ranked_context;
mod service;
mod sizing;
mod term_statistics;

pub use context::{ContextSelectionReport, ContextSourceProvider, ContextTokenCounter};
pub use diagnostics::ResolutionDiagnosticPage;
#[cfg(feature = "benchmark-instrumentation")]
pub use identifier_index::IdentifierIndexBuildMetrics;
pub use identifier_index::lexical::{LexicalCandidateRequest, LexicalChannel, LexicalFamily};
pub use identifier_index::{
    DiscoveryPackingPreference, GenerationIdentifierIndex, LexicalPlanRequest,
    SourceRolePackingRequest, SourceRolePreference,
};
pub use identifier_normalization::{IDENTIFIER_NORMALIZATION_REVISION, identifier_terms};
pub use ranked_candidates::{RankedCandidate, reference_ranked_candidates};
pub use service::*;
pub use sizing::{HistoricalNeighborhoodLimits, HistoricalNeighborhoodOutputSizer};
pub use term_statistics::{GenerationTermStatistics, generation_term_statistics};
