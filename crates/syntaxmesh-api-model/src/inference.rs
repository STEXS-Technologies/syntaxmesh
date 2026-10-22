//! Storage-independent retained inference interchange. Hosts enforce admission.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use syntaxmesh_core::{
    FactVersionRef, GenerationId, NodeId, RelationKind, RepositoryId, StableId, WorktreeId,
};

pub const INFERENCE_READ_SCHEMA_VERSION: u32 = 1;

/// Operator binding selection, never a database path or authorization credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceCheckpointRequest {
    pub schema_version: u32,
    pub binding: String,
    pub generation: GenerationId,
}

/// Independent wire limits; hosts cap and convert these before storage access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferencePageLimits {
    pub claims: u64,
    pub payload_bytes: u64,
    pub premises: u64,
    pub tree_pages: u64,
}

/// Complete historical scope, independent of a claim's birth generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceSelection {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation: GenerationId,
}

/// Exclusive continuation, including the operator binding as well as tree scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferencePageCursor {
    pub binding: String,
    pub stream: StableId,
    pub selection: InferenceSelection,
    pub root: Option<StableId>,
    pub after: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferencePageRequest {
    pub schema_version: u32,
    pub binding: String,
    pub generation: GenerationId,
    pub limits: InferencePageLimits,
    pub cursor: Option<InferencePageCursor>,
}

/// Retained synchronization coordinates, not a proof of witness correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceCheckpoint {
    pub stream: StableId,
    pub selection: InferenceSelection,
    pub sequence: u64,
    pub previous: Option<GenerationId>,
    pub added_claims: u64,
    pub affected_claims: u64,
    pub closure_reasons: u64,
    pub active_claims: u64,
}

/// Explicit absence never means the latest checkpoint was substituted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceCheckpointResponse {
    pub schema_version: u32,
    pub binding: String,
    pub generation: GenerationId,
    pub checkpoint: Option<InferenceCheckpoint>,
}

/// Derivation method, not premise certainty, confidence or canonical evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceDerivationClass {
    DeterministicRule,
    HeuristicRule,
    ModelInference,
    UserDeclared,
    RuntimeCorrelation,
}

/// Exact retained witness. `generation` is its birth, not the page selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceClaim {
    pub id: StableId,
    pub derivation_class: InferenceDerivationClass,
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation: GenerationId,
    pub source: NodeId,
    pub target: NodeId,
    pub relation: RelationKind,
    pub rule_id: String,
    pub rule_version: String,
    pub ontology_versions: BTreeMap<String, String>,
    pub configuration_fingerprint: StableId,
    pub premises: Vec<FactVersionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferencePageResponse {
    pub schema_version: u32,
    pub binding: String,
    pub checkpoint: InferenceCheckpoint,
    pub claims: Vec<InferenceClaim>,
    pub next: Option<InferencePageCursor>,
}

#[cfg(test)]
mod tests;
