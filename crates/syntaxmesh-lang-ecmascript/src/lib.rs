//! Oxc-backed TypeScript and JavaScript language packs.

mod oxc_extractor;
mod resolution;

pub use oxc_extractor::{JavaScriptExtractor, TypeScriptExtractor};
pub use resolution::{ModuleResolutionError, OxcModuleResolver};
