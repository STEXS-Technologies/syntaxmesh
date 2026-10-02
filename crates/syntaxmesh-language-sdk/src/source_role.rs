//! Language-independent positive source-role metadata with legacy compatibility.

use syntaxmesh_core::ExtensionPayload;

pub const SOURCE_ROLE_NAMESPACE: &str = "syntaxmesh.source-role";

/// Positive syntactic intent, not evidence that a test executes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SourceRole {
    /// Missing or unrelated role evidence is not proof of production code.
    #[default]
    Unknown,
    TestIntent,
}

impl SourceRole {
    #[must_use]
    pub fn payload(self) -> Option<ExtensionPayload> {
        match self {
            Self::Unknown => None,
            Self::TestIntent => Some(ExtensionPayload {
                namespace: SOURCE_ROLE_NAMESPACE.to_owned(),
                schema_version: 1,
                bytes: vec![1],
            }),
        }
    }

    /// Decode positive source intent without assuming a producer language.
    ///
    /// # Errors
    /// Rejects malformed or unsupported payloads in recognized role namespaces.
    pub fn from_payload(payload: Option<&ExtensionPayload>) -> Result<Self, SourceRoleError> {
        let Some(payload) = payload else {
            return Ok(Self::Unknown);
        };
        if payload.namespace != SOURCE_ROLE_NAMESPACE
            && payload.namespace != "syntaxmesh.lang.rust.test-role"
        {
            return Ok(Self::Unknown);
        }
        if payload.schema_version == 1 && payload.bytes == [1] {
            Ok(Self::TestIntent)
        } else {
            Err(SourceRoleError)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRoleError;

impl std::fmt::Display for SourceRoleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid persisted source role metadata")
    }
}

impl std::error::Error for SourceRoleError {}

#[cfg(test)]
mod tests;
