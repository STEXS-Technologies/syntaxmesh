//! Host-owned process supervision for external SyntaxMesh producers.

mod supervisor;

pub use supervisor::{
    ProcessLimits, ProcessReceipt, ProcessSupervisor, ProducerCommand, SupervisionError,
};
