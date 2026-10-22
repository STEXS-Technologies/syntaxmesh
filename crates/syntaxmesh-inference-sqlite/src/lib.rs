//! Opt-in downstream claim storage; never participates in canonical publication.

mod lifecycle;
mod materializer;
mod migrations;
mod read;
mod stream;
#[cfg(test)]
mod test_fixture;

pub use lifecycle::{ClaimLifecycle, ClosureReason, LifecycleReceipt, LifecycleRequest};
pub use materializer::{InferenceMaterializer, RebuildRequest, Result, Watermark};
pub use stream::{
    StreamCheckpoint, StreamClaim, StreamClaims, StreamCursor, StreamOrigin, StreamPage,
    StreamPageRequest, StreamSyncRequest,
};
