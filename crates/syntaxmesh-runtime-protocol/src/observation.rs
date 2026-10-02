//! Small observation envelope for game, SDK, and service probes.
//!
//! This crate deliberately carries no engine, workflow, database, or host code.

use serde::{Deserialize, Serialize};

pub const OBSERVATION_SCHEMA_VERSION: u32 = 1;

/// The receiver assigns final trust; a probe can only describe its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationOrigin {
    ClientProbe,
    ServerProbe,
    WorkflowAdapter,
    CommittedStateAdapter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub schema_version: u32,
    pub producer_namespace: String,
    pub producer_version: String,
    pub origin: ObservationOrigin,
    pub observed_at_unix_nanos: u64,
    /// Opaque stable identifiers supplied by the producer; the engine resolves
    /// them to canonical entities without assuming the producer is authoritative.
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub correlation_id: Option<String>,
}

impl Observation {
    /// Validate the wire schema and required envelope fields.
    ///
    /// # Errors
    /// Returns an error for an unsupported schema or missing required field.
    pub const fn validate(&self) -> Result<(), ObservationError> {
        if self.schema_version != OBSERVATION_SCHEMA_VERSION {
            return Err(ObservationError::UnsupportedSchema);
        }
        if self.producer_namespace.is_empty()
            || self.producer_version.is_empty()
            || self.subject.is_empty()
            || self.relation.is_empty()
            || self.object.is_empty()
        {
            return Err(ObservationError::MissingRequiredField);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationError {
    UnsupportedSchema,
    MissingRequiredField,
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ObservationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_wire_schema() {
        let observation = Observation {
            schema_version: 2,
            producer_namespace: "example.game".into(),
            producer_version: "1".into(),
            origin: ObservationOrigin::ClientProbe,
            observed_at_unix_nanos: 0,
            subject: "a".into(),
            relation: "called".into(),
            object: "b".into(),
            correlation_id: None,
        };
        assert_eq!(
            observation.validate(),
            Err(ObservationError::UnsupportedSchema)
        );
    }
}
