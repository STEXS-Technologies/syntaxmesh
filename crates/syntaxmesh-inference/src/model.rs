use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{
    Edge, FactVersionRef, GenerationId, NodeId, RelationKind, RepositoryId, StableId, WorktreeId,
};

/// One externally verified retained fact and its selection label.
#[derive(Clone, Serialize)]
pub struct Premise {
    pub generation: GenerationId,
    pub valid_from: GenerationId,
    pub edge: Edge,
}

pub struct Selection<'facts> {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation: GenerationId,
    pub nodes: &'facts BTreeSet<NodeId>,
    pub premises: &'facts [Premise],
}

/// A constrained path, join and terminal concept-declaration rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub version: String,
    pub ontology_versions: BTreeMap<String, String>,
    pub configuration_fingerprint: StableId,
    pub path_relation: RelationKind,
    pub join_relation: RelationKind,
    pub declaration_relation: RelationKind,
    pub concept: NodeId,
    pub output_relation: RelationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    pub input_bytes: usize,
    pub facts: usize,
    pub edge_visits: usize,
    pub depth: usize,
    pub claims: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidationLimits {
    pub generations: usize,
    pub versions: usize,
    pub postings: usize,
    pub claims: usize,
}

/// Independent admission limits for schema-validated delta discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredDiscoveryBudgets {
    pub history_scan: usize,
    pub rules: usize,
    pub ontology_bindings: usize,
}

/// Accepted range `(from, selected generation]` for canonical opening discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryRange {
    pub from: GenerationId,
    pub generations: usize,
    pub versions: usize,
}

/// Accepted transition that closed one exact canonical premise version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PremiseClosure {
    pub generation: GenerationId,
    pub generation_sequence: u64,
}

/// The retained selection owning a complete inferred claim set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSelection {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation: GenerationId,
}

/// How a claim was derived, independent of premise certainty or confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DerivationClass {
    DeterministicRule,
    HeuristicRule,
    ModelInference,
    UserDeclared,
    RuntimeCorrelation,
}

/// A rule witness, distinct from an extracted canonical fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: StableId,
    pub derivation_class: DerivationClass,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceError {
    InvalidInput,
    BudgetExhausted,
    DepthExhausted,
    IdentityEncoding,
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "inference rejected: {self:?}")
    }
}

impl std::error::Error for InferenceError {}
