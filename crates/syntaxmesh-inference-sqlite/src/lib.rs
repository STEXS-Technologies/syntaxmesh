//! Opt-in downstream derived-result storage; never participates in canonical publication.

mod contract_history;
mod lifecycle;
mod materializer;
mod migrations;
mod read;
mod stream;
#[cfg(test)]
mod test_fixture;

pub use contract_history::{
    ContractHistoryCursor, ContractHistoryPage, ContractHistoryQuery, ContractHistoryRequest,
    ContractHistoryWatermark, ContractOutcomeEpisode, ContractOutcomeEpisodeQuery,
    ContractOutcomeTimelineCursor, ContractOutcomeTimelinePage, ContractOutcomeTimelineQuery,
    RetainedContractEvaluation, RetainedContractSet,
};
pub use lifecycle::{ClaimLifecycle, ClosureReason, LifecycleReceipt, LifecycleRequest};
pub use materializer::{InferenceMaterializer, RebuildRequest, Result, Watermark};
pub use stream::{
    StreamCheckpoint, StreamClaim, StreamClaims, StreamCursor, StreamOrigin, StreamPage,
    StreamPageRequest, StreamSyncRequest,
};
