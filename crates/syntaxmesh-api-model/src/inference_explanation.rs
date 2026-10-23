//! Versioned, storage-independent proof evidence for one verified inference claim.

use crate::{InferenceClaim, InferenceSelection};
use serde::{Deserialize, Serialize};
use syntaxmesh_core::{Edge, FactVersionRef, Provenance};

pub const INFERENCE_EXPLANATION_SCHEMA_VERSION: u32 = 1;

/// One exact retained premise version with its producer and optional source span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferencePremiseEvidence {
    pub reference: FactVersionRef,
    pub edge: Edge,
    pub provenance: Provenance,
}

/// A deterministic-rule claim whose witness was verified against `selection`.
///
/// This is one direct witness, not a recursive derivation DAG or truth-maintenance
/// status. The premise order matches `claim.premises` exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceExplanation {
    pub schema_version: u32,
    pub selection: InferenceSelection,
    pub claim: InferenceClaim,
    pub premises: Vec<InferencePremiseEvidence>,
}

#[cfg(test)]
mod tests;
