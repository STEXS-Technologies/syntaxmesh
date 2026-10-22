//! Shared operator binding and admission; not a transport or workflow engine.

mod admission;
mod conversion;

pub use admission::{AdmittedContractHistory, AdmittedInferencePage, InferenceReadBindings};
pub use conversion::{inference_checkpoint_record, inference_page_response};
pub use syntaxmesh_inference_sqlite::ContractHistoryCursor;
