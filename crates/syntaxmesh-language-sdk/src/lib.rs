//! Public language-extraction contracts.

mod composite;
mod diagnostic;
mod extractor;
mod processing;
mod reference_resolution;
mod source_role;
mod targets;

pub use composite::{
    CompositeExtractor, ExtractorRegistry, ExtractorRegistryError, SendCompositeExtractor,
};
pub use diagnostic::{
    MAX_SYNTAX_DIAGNOSTIC_BYTES, SYNTAX_DIAGNOSTIC_NAMESPACE, SyntaxDiagnostic,
    SyntaxDiagnosticError,
};
pub use extractor::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, Reference, SourceFile,
};
pub use processing::{SOURCE_PROCESSING_NAMESPACE, SourceProcessing, SourceProcessingError};
pub use reference_resolution::{
    REFERENCE_RESOLUTION_NAMESPACE, ReferenceResolution, ReferenceResolutionError,
};
pub use source_role::{SOURCE_ROLE_NAMESPACE, SourceRole, SourceRoleError};
pub use syntaxmesh_core::{
    ExportKind, ExportRecord, ExportRecordId, ImportKind, ImportRecord, ImportRecordId,
};
pub use targets::{SOURCE_ANCHOR_NAMESPACE, is_reference_target_kind, is_source_anchor};
