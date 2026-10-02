//! Separate, host-independent symbol-resolution stage.

mod module_resolution;
mod resolver;

pub use module_resolution::{
    CompositeModuleResolutionProvider, ModuleResolutionMode, ModuleResolutionOutcome,
    ModuleResolutionProvider, ModuleResolutionRequest, ModuleResolverIdentity,
};
pub use resolver::{REFERENCE_RESOLVER_REVISION, Resolution, resolve_references};
