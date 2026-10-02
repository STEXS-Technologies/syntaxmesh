//! Structured Rust queries and durable materialization for SyntaxMesh fact history.

mod analytics;
mod materializer;

pub use analytics::{
    AnalyticsDuckDbError, FactHistoryDuckDb, GenerationFactCount, MAX_GENERATION_COUNT_ROWS,
};
pub use materializer::{
    FactHistoryMaterializationReceipt, FactHistoryMaterializationWatermark,
    FactHistoryMaterializer, FactHistoryMaterializerError,
};
