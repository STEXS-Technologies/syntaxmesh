//! Public capability and namespace contract for out-of-tree extensions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use syntaxmesh_core::{Edge, Node, NodeKind, Provenance, RelationKind};
use syntaxmesh_runtime_protocol::Observation;

pub const EXTENSION_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    SourceFacts,
    RuntimeObservations,
    AnalysisFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub schema_version: u32,
    pub namespace: String,
    pub producer_version: String,
    pub capabilities: Vec<Capability>,
}

impl ExtensionManifest {
    /// Validate the public manifest schema, namespace, and capabilities.
    ///
    /// # Errors
    /// Returns an error if a field is invalid or a capability is repeated.
    pub fn validate(&self) -> Result<(), ExtensionError> {
        if self.schema_version != EXTENSION_MANIFEST_SCHEMA_VERSION {
            return Err(ExtensionError::UnsupportedSchema);
        }
        let mut chars = self.namespace.chars();
        if !matches!(chars.next(), Some('a'..='z'))
            || !chars.all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_')
            })
        {
            return Err(ExtensionError::InvalidNamespace);
        }
        if self.producer_version.is_empty() || self.capabilities.is_empty() {
            return Err(ExtensionError::MissingRequiredField);
        }
        let unique: BTreeSet<_> = self.capabilities.iter().copied().collect();
        if unique.len() != self.capabilities.len() {
            return Err(ExtensionError::DuplicateCapability);
        }
        Ok(())
    }
}

/// The SDK transports typed facts; store transactions remain engine-owned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactBatch {
    pub manifest: ExtensionManifest,
    pub provenance: Vec<Provenance>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub observations: Vec<Observation>,
}

impl FactBatch {
    /// Validate that submitted facts match declared capabilities and namespace.
    ///
    /// # Errors
    /// Returns an error if the manifest or an observation is invalid, or a
    /// batch uses a capability or namespace it did not declare.
    pub fn validate(&self) -> Result<(), ExtensionError> {
        self.manifest.validate()?;
        let capabilities: BTreeSet<_> = self.manifest.capabilities.iter().copied().collect();
        if (!self.nodes.is_empty() || !self.edges.is_empty())
            && !capabilities.contains(&Capability::SourceFacts)
            && !capabilities.contains(&Capability::AnalysisFacts)
        {
            return Err(ExtensionError::UndeclaredCapability);
        }
        if !self.observations.is_empty() && !capabilities.contains(&Capability::RuntimeObservations)
        {
            return Err(ExtensionError::UndeclaredCapability);
        }
        if self.provenance.iter().any(|p| {
            p.producer_namespace != self.manifest.namespace
                || p.producer_version != self.manifest.producer_version
        }) {
            return Err(ExtensionError::NamespaceMismatch);
        }
        let node_namespace_mismatch = self.nodes.iter().any(|node| {
            matches!(&node.kind, NodeKind::External { namespace, .. } if namespace != &self.manifest.namespace)
                || node.extension_payload.as_ref().is_some_and(|payload| {
                    payload.namespace != self.manifest.namespace || payload.schema_version == 0
                })
        });
        let edge_namespace_mismatch = self.edges.iter().any(|edge| {
            matches!(&edge.relation, RelationKind::External { namespace, .. } if namespace != &self.manifest.namespace)
                || edge.extension_payload.as_ref().is_some_and(|payload| {
                    payload.namespace != self.manifest.namespace || payload.schema_version == 0
                })
        });
        if node_namespace_mismatch || edge_namespace_mismatch {
            return Err(ExtensionError::NamespaceMismatch);
        }
        let node_has_namespace = |node: &Node| {
            matches!(&node.kind, NodeKind::External { namespace, .. } if namespace == &self.manifest.namespace)
                || node.extension_payload.as_ref().is_some_and(|payload| {
                    payload.namespace == self.manifest.namespace && payload.schema_version > 0
                })
        };
        let edge_has_namespace = |edge: &Edge| {
            matches!(&edge.relation, RelationKind::External { namespace, .. } if namespace == &self.manifest.namespace)
                || edge.extension_payload.as_ref().is_some_and(|payload| {
                    payload.namespace == self.manifest.namespace && payload.schema_version > 0
                })
        };
        if self.nodes.iter().any(|node| !node_has_namespace(node))
            || self.edges.iter().any(|edge| !edge_has_namespace(edge))
        {
            return Err(ExtensionError::MissingNamespaceOwnership);
        }
        for observation in &self.observations {
            if observation.producer_namespace != self.manifest.namespace
                || observation.producer_version != self.manifest.producer_version
                || observation.validate().is_err()
            {
                return Err(ExtensionError::InvalidObservation);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionError {
    UnsupportedSchema,
    InvalidNamespace,
    MissingRequiredField,
    DuplicateCapability,
    UndeclaredCapability,
    NamespaceMismatch,
    MissingNamespaceOwnership,
    InvalidObservation,
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::UnsupportedSchema => "extension manifest uses an unsupported schema version",
            Self::InvalidNamespace => {
                "extension namespace must start with a lowercase letter and contain only lowercase ASCII letters, digits, '.', '-' or '_'"
            }
            Self::MissingRequiredField => {
                "extension manifest requires a producer version and at least one capability"
            }
            Self::DuplicateCapability => "extension manifest declares a capability more than once",
            Self::UndeclaredCapability => {
                "extension batch contains a capability not declared by its manifest"
            }
            Self::NamespaceMismatch => {
                "extension fact or provenance namespace does not match the manifest namespace"
            }
            Self::MissingNamespaceOwnership => {
                "extension facts must carry a namespaced external kind or versioned extension payload"
            }
            Self::InvalidObservation => {
                "runtime observation is invalid or does not match the extension producer"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for ExtensionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_namespace_collision_with_fact_producer() {
        let batch = FactBatch {
            manifest: ExtensionManifest {
                schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
                namespace: "example.lang".into(),
                producer_version: "1".into(),
                capabilities: vec![Capability::SourceFacts],
            },
            provenance: vec![Provenance {
                id: syntaxmesh_core::ProvenanceId::derive(&[b"p"]),
                producer_namespace: "someone.else".into(),
                producer_version: "1".into(),
                evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
                source: None,
            }],
            nodes: vec![],
            edges: vec![],
            observations: vec![],
        };
        assert_eq!(batch.validate(), Err(ExtensionError::NamespaceMismatch));
    }
}
