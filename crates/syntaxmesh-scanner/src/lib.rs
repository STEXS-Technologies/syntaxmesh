//! Host-neutral filesystem discovery for SyntaxMesh source inputs.

mod scanner;

pub use scanner::{SUPPORTED_SOURCE_EXTENSIONS, ScanError, ScanMetrics, ScanReport, scan};
