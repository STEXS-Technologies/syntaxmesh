//! Public SyntaxMesh extension contracts.

mod contract;

pub use contract::{
    Capability, EXTENSION_MANIFEST_SCHEMA_VERSION, ExtensionError, ExtensionManifest, FactBatch,
};
