//! Penelope-backed publication integration.

mod adapter;
mod analytics_export;
#[cfg(feature = "analytics-duckdb")]
mod analytics_materializer;
#[cfg(feature = "inference-sqlite")]
mod inference_materializer;
mod semantic;

pub use adapter::{PenelopeDiagnostics, PenelopePublisher, PenelopeWorkflow};
pub use analytics_export::{AnalyticsExportReceipt, PenelopeAnalyticsExporter};
#[cfg(feature = "analytics-duckdb")]
pub use analytics_materializer::{
    MaterializationActionKind, PenelopeAnalyticsMaterializer, PenelopeMaterializationReceipt,
};
#[cfg(feature = "inference-sqlite")]
pub use inference_materializer::{
    ContractHistoryRequest, PenelopeInferenceLifecycle, PenelopeInferenceMaterializer,
    PenelopeInferenceSetup, PenelopeInferenceStream, RegisteredInferenceAdmission,
    StreamBeginRequest,
};
pub use semantic::PenelopeSemanticEnricher;
