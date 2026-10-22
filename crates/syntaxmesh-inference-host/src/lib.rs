//! Shared operator binding and admission; not a transport or workflow engine.

mod admission;
mod conversion;

pub use admission::{AdmittedInferencePage, InferenceReadBindings};
pub use conversion::{inference_checkpoint_record, inference_page_response};
