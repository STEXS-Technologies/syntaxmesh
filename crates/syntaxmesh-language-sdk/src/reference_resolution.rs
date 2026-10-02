use syntaxmesh_core::ExtensionPayload;

#[cfg(test)]
mod tests;

pub const REFERENCE_RESOLUTION_NAMESPACE: &str = "syntaxmesh.reference-resolution";

/// Source producer constraint on static target inference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReferenceResolution {
    /// Use the configured name resolver; missing/ambiguous names stay explicit.
    #[default]
    Name,
    /// The source occurrence is known, but its static callable target is not.
    Unresolved,
}

impl ReferenceResolution {
    /// Canonical non-default occurrence metadata; legacy Name adds no payload.
    #[must_use]
    pub fn payload(self) -> Option<ExtensionPayload> {
        match self {
            Self::Name => None,
            Self::Unresolved => Some(ExtensionPayload {
                namespace: REFERENCE_RESOLUTION_NAMESPACE.to_owned(),
                schema_version: 1,
                bytes: vec![1],
            }),
        }
    }

    /// Restore a persisted producer constraint.
    ///
    /// # Errors
    /// Rejects unsupported or malformed metadata in the reserved namespace.
    pub fn from_payload(
        payload: Option<&ExtensionPayload>,
    ) -> Result<Self, ReferenceResolutionError> {
        let Some(payload) = payload else {
            return Ok(Self::Name);
        };
        if payload.namespace != REFERENCE_RESOLUTION_NAMESPACE {
            return Ok(Self::Name);
        }
        if payload.schema_version == 1 && payload.bytes == [1] {
            Ok(Self::Unresolved)
        } else {
            Err(ReferenceResolutionError)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceResolutionError;

impl std::fmt::Display for ReferenceResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid persisted reference resolution constraint")
    }
}

impl std::error::Error for ReferenceResolutionError {}
