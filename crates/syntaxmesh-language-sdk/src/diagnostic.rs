//! Bounded source grammar diagnostics, independent of storage and hosts.

use syntaxmesh_core::ExtensionPayload;

pub const SYNTAX_DIAGNOSTIC_NAMESPACE: &str = "syntaxmesh.syntax-diagnostic";

/// Maximum retained UTF-8 bytes in one source grammar diagnostic.
pub const MAX_SYNTAX_DIAGNOSTIC_BYTES: usize = 4096;

/// A bounded prefix of a parser message with explicit loss information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    message: String,
    truncated: bool,
}

impl SyntaxDiagnostic {
    /// Retains at most the byte limit without splitting a UTF-8 character.
    /// This does not bound the parser's own allocation of the original message.
    #[must_use]
    pub fn new(message: &str) -> Self {
        let mut end = message.len().min(MAX_SYNTAX_DIAGNOSTIC_BYTES);
        while !message.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        Self {
            message: message[..end].to_owned(),
            truncated: end < message.len(),
        }
    }

    /// Retained parser message, which may be empty.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Whether bytes were omitted from the original parser message.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Versioned binary metadata following the shared fact payload contract.
    #[must_use]
    pub fn payload(&self) -> ExtensionPayload {
        let mut bytes = vec![u8::from(self.truncated)];
        bytes.extend_from_slice(self.message.as_bytes());
        ExtensionPayload {
            namespace: SYNTAX_DIAGNOSTIC_NAMESPACE.to_owned(),
            schema_version: 1,
            bytes,
        }
    }

    /// Restore a diagnostic without treating missing evidence as success.
    ///
    /// # Errors
    /// Rejects unsupported schemas and malformed recognized payloads.
    pub fn from_payload(
        payload: Option<&ExtensionPayload>,
    ) -> Result<Option<Self>, SyntaxDiagnosticError> {
        let Some(payload) = payload else {
            return Ok(None);
        };
        if payload.namespace != SYNTAX_DIAGNOSTIC_NAMESPACE {
            return Ok(None);
        }
        if payload.schema_version != 1 {
            return Err(SyntaxDiagnosticError);
        }
        let (flag, message) = payload.bytes.split_first().ok_or(SyntaxDiagnosticError)?;
        if *flag > 1 || message.len() > MAX_SYNTAX_DIAGNOSTIC_BYTES {
            return Err(SyntaxDiagnosticError);
        }
        let message =
            std::str::from_utf8(message).map_err(|_invalid_utf8| SyntaxDiagnosticError)?;
        Ok(Some(Self {
            message: message.to_owned(),
            truncated: *flag == 1,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxDiagnosticError;

impl std::fmt::Display for SyntaxDiagnosticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid persisted syntax diagnostic metadata")
    }
}

impl std::error::Error for SyntaxDiagnosticError {}

#[cfg(test)]
mod tests;
