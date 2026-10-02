//! Runtime-neutral application composition for indexing and querying.

mod engine;
mod freshness;
mod module_inputs;
mod processing;
mod source_planning;

pub use engine::{
    Clock, EngineError, EngineIndexFreshness, EngineIntegrity, EngineReceipt, EngineStatus,
    EngineWorkflowDiagnostics, SemanticBatchLimits, SyntaxMeshEngine, SystemClock,
};
pub use module_inputs::{load_module_inputs, save_module_inputs};
pub use processing::SourceProcessingCoverage;
pub use source_planning::{prepare_source_index, source_inventory_fingerprint};
pub use syntaxmesh_indexer::SourceSyntaxPolicy;
pub use syntaxmesh_workflow::{WorkflowRejection, WorkflowRejectionPage, WorkflowRejectionReason};
