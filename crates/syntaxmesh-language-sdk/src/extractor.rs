//! Host-independent contracts for deterministic language extraction.

use syntaxmesh_core::{
    Edge, ExportRecord, FileVersion, ImportRecord, Node, NodeId, Provenance, ProvenanceId,
    RelationKind, SourceLocation, StableId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub file: FileVersion,
    pub content: String,
}

/// Stable identity for one language producer, used to detect stale extracted
/// facts when an extractor implementation changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractorIdentity {
    pub namespace: String,
    pub version: String,
}

impl ExtractorIdentity {
    /// Creates a namespaced producer identity.
    #[must_use]
    pub fn new(namespace: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            version: version.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extraction {
    pub provenance: Provenance,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Raw symbol references, left unresolved by the language extractor.
    pub references: Vec<Reference>,
    /// Typed source import occurrences, not yet resolved to modules or symbols.
    pub imports: Vec<ImportRecord>,
    /// Typed source export and re-export occurrences.
    pub exports: Vec<ExportRecord>,
}

/// One source-backed reference occurrence emitted before repository resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Producer constraint retained through publication and re-resolution.
    pub resolution: crate::ReferenceResolution,
    /// Stable identity for this occurrence, independent of byte offsets.
    pub id: NodeId,
    /// Definition or containing symbol from which the reference originates.
    pub source: NodeId,
    /// Extracted target spelling, which may be qualified or ambiguous.
    pub target: String,
    /// Semantic edge kind to create if the target resolves uniquely.
    pub relation: RelationKind,
    /// Source location of the reference expression.
    pub source_location: SourceLocation,
    /// Producer evidence attached to any canonical facts derived from it.
    pub provenance: ProvenanceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractorError {
    UnsupportedFile(String),
    /// Source grammar rejection, not an internal extraction invariant failure.
    SyntaxError(String),
    InvalidInput(String),
}

impl std::fmt::Display for ExtractorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ExtractorError {}

/// A deterministic extractor is pure with respect to one source file.
pub trait LanguageExtractor {
    fn language(&self) -> &'static str;

    /// Stable producer namespace and implementation/configuration version.
    fn producer_identity(&self) -> ExtractorIdentity;

    /// Identity selected for this file. Routers override this to choose the
    /// identity of the child extractor that would parse the source.
    ///
    /// # Errors
    /// Returns an error when this extractor does not support the source file.
    fn producer_identity_for(
        &self,
        _source: &SourceFile,
    ) -> Result<ExtractorIdentity, ExtractorError> {
        Ok(self.producer_identity())
    }

    /// Stable fingerprint of the extractor configuration used to derive host
    /// generation identities. Routers include their sorted extension map.
    #[must_use]
    fn configuration_fingerprint(&self) -> [u8; 32] {
        let identity = self.producer_identity();
        StableId::derive(
            "language-extractor-config-v1",
            &[identity.namespace.as_bytes(), identity.version.as_bytes()],
        )
        .0
    }

    /// # Errors
    /// Returns an error when the file is unsupported or malformed for the extractor.
    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError>;
}
