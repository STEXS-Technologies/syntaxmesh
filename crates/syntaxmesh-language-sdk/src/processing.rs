//! Generation-independent construction of file processing evidence.

use syntaxmesh_core::{
    EvidenceClass, FileVersion, Node, NodeId, NodeKind, Provenance, ProvenanceId, SourceLocation,
    SourceSpan,
};

use crate::{ExtractorIdentity, SyntaxDiagnostic};

pub const SOURCE_PROCESSING_NAMESPACE: &str = "syntaxmesh.source-processing";

/// Observed processing outcome, never inferred from the number of declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceProcessing {
    Completed,
    SyntaxFailed(SyntaxDiagnostic),
}

impl SourceProcessing {
    /// Construct one file-owned fact and its separately identified provenance.
    /// The caller must publish these with the observed file version atomically.
    #[must_use]
    pub fn facts(&self, file: &FileVersion, producer: &ExtractorIdentity) -> (Node, Provenance) {
        let (kind, payload) = match self {
            Self::Completed => ("completed.v1", None),
            Self::SyntaxFailed(diagnostic) => ("syntax_failed.v1", Some(diagnostic.payload())),
        };
        let source = SourceLocation {
            file_id: file.file_id,
            content_hash: file.content_hash,
            span: SourceSpan {
                start_byte: 0,
                end_byte: file.size_bytes,
            },
        };
        let diagnostic_bytes = payload
            .as_ref()
            .map_or(&[][..], |payload| payload.bytes.as_slice());
        let provenance = Provenance {
            id: ProvenanceId::derive(&[
                SOURCE_PROCESSING_NAMESPACE.as_bytes(),
                &file.file_id.0.0,
                &file.content_hash,
                producer.namespace.as_bytes(),
                producer.version.as_bytes(),
                kind.as_bytes(),
                diagnostic_bytes,
            ]),
            producer_namespace: format!("{SOURCE_PROCESSING_NAMESPACE}.{}", producer.namespace),
            producer_version: producer.version.clone(),
            evidence_class: EvidenceClass::SourceFact,
            source: Some(source.clone()),
        };
        let node = Node {
            id: NodeId::derive(&[SOURCE_PROCESSING_NAMESPACE.as_bytes(), &file.file_id.0.0]),
            kind: NodeKind::External {
                namespace: SOURCE_PROCESSING_NAMESPACE.to_owned(),
                kind: kind.to_owned(),
            },
            name: file.normalized_path.clone(),
            owner_file: Some(file.file_id),
            source: Some(source),
            provenance: provenance.id,
            extension_payload: payload,
        };
        (node, provenance)
    }

    /// Validate recognized processing evidence against its observed file.
    /// Unrelated node kinds carry no processing evidence.
    ///
    /// # Errors
    /// Rejects unknown kinds, malformed diagnostics and noncanonical fact pairs.
    pub fn from_facts(
        file: &FileVersion,
        node: &Node,
        provenance: &Provenance,
    ) -> Result<Option<Self>, SourceProcessingError> {
        let NodeKind::External { namespace, kind } = &node.kind else {
            return Ok(None);
        };
        if namespace != SOURCE_PROCESSING_NAMESPACE {
            return Ok(None);
        }
        let outcome = match kind.as_str() {
            "completed.v1" if node.extension_payload.is_none() => Self::Completed,
            "syntax_failed.v1" => Self::SyntaxFailed(
                SyntaxDiagnostic::from_payload(node.extension_payload.as_ref())
                    .map_err(|_invalid_diagnostic| SourceProcessingError)?
                    .ok_or(SourceProcessingError)?,
            ),
            _ => return Err(SourceProcessingError),
        };
        let attempted_namespace = provenance
            .producer_namespace
            .strip_prefix(&format!("{SOURCE_PROCESSING_NAMESPACE}."))
            .ok_or(SourceProcessingError)?;
        let producer = ExtractorIdentity::new(attempted_namespace, &provenance.producer_version);
        let (expected_node, expected_provenance) = outcome.facts(file, &producer);
        if *node != expected_node || *provenance != expected_provenance {
            return Err(SourceProcessingError);
        }
        Ok(Some(outcome))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceProcessingError;

impl std::fmt::Display for SourceProcessingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid persisted source processing evidence")
    }
}

impl std::error::Error for SourceProcessingError {}

#[cfg(test)]
mod tests;
