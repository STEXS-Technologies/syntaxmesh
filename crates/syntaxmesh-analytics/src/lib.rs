//! Typed analytical adapters over SyntaxMesh's canonical graph store.

mod arrow_export;

pub use arrow_export::{
    AnalyticsError, FACT_HISTORY_ARROW_SCHEMA_VERSION, FactHistoryArrowBatch,
    FactHistoryBatchReader, MAX_FACT_HISTORY_PAGE_SIZE, fact_history_arrow_schema,
    fact_history_page_to_batch, fact_version_changes_page_to_batch,
};
