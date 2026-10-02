//! Penelope-backed publication integration.

mod adapter;
mod analytics_export;
#[cfg(feature = "analytics-duckdb")]
mod analytics_materializer;
mod semantic;

pub use adapter::{PenelopeDiagnostics, PenelopePublisher, PenelopeWorkflow};
pub use analytics_export::{AnalyticsExportReceipt, PenelopeAnalyticsExporter};
#[cfg(feature = "analytics-duckdb")]
pub use analytics_materializer::{
    MaterializationActionKind, PenelopeAnalyticsMaterializer, PenelopeMaterializationReceipt,
};
pub use semantic::PenelopeSemanticEnricher;
