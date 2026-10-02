//! Pure, host-independent identities and graph facts.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A versioned, domain-separated 256-bit identity. The byte encoding is public;
/// the derivation version must change before its algorithm changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StableId(pub [u8; 32]);

impl StableId {
    /// Derive an ID without ambiguous component concatenation.
    #[must_use]
    pub fn derive(domain: &str, components: &[&[u8]]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"syntaxmesh-id-v1\0");
        update_component(&mut hasher, domain.as_bytes());
        for component in components {
            update_component(&mut hasher, component);
        }
        Self(*hasher.finalize().as_bytes())
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

fn update_component(hasher: &mut blake3::Hasher, component: &[u8]) {
    hasher.update(&(component.len() as u64).to_le_bytes());
    hasher.update(component);
}

macro_rules! typed_id {
    ($name:ident, $domain:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(pub StableId);

        impl $name {
            #[must_use]
            pub fn derive(components: &[&[u8]]) -> Self {
                Self(StableId::derive($domain, components))
            }
        }
    };
}

typed_id!(RepositoryId, "repository");
typed_id!(WorktreeId, "worktree");
typed_id!(FileId, "file");
typed_id!(NodeId, "node");
typed_id!(EdgeId, "edge");
typed_id!(ProvenanceId, "provenance");
typed_id!(GenerationId, "generation");

/// Direction used when paging historical edges adjacent to one node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeDirection {
    /// Edges whose source is the queried endpoint.
    Outgoing,
    /// Edges whose target is the queried endpoint.
    Incoming,
}
typed_id!(IndexRunId, "index-run");
typed_id!(ChangeEventId, "change-event");
typed_id!(ChangeSetId, "change-set");
typed_id!(ConsequenceEdgeId, "consequence-edge");
typed_id!(ImportRecordId, "source-import");
typed_id!(ExportRecordId, "source-export");

/// Producer-reported event time, distinct from engine acceptance time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObservationTime(pub u64);

impl ObservationTime {
    /// Return the producer-reported Unix timestamp in nanoseconds.
    #[must_use]
    pub const fn as_unix_nanos(self) -> u64 {
        self.0
    }
}

/// Continuation point for a producer-observation timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObservedFactCursor {
    pub observed_at: ObservationTime,
    pub fact: FactRef,
    pub valid_from: GenerationId,
}

/// Stable identity of one canonical fact family member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FactRef {
    File(FileId),
    Provenance(ProvenanceId),
    Node(NodeId),
    Edge(EdgeId),
}

/// Stable address of one retained version of a canonical fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FactVersionRef {
    pub fact: FactRef,
    pub valid_from: GenerationId,
}

/// How one canonical fact identity participated in an accepted transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactChangeKind {
    Added,
    Updated,
    Removed,
}

/// A canonical fact identity affected by one accepted change event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFact {
    pub fact: FactRef,
    pub kind: FactChangeKind,
}

/// Stable, generation-scoped record of one accepted graph transition.
///
/// The event links to fact identities, not to a causal claim between those
/// facts. Its `id` is derived from repository/worktree, transition endpoints,
/// and the canonical digest of the accepted delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub id: ChangeEventId,
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation_before: Option<GenerationId>,
    pub generation_after: GenerationId,
    pub changed_facts: Vec<ChangedFact>,
}

/// Cursor after the last event returned for a fact identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEventCursor {
    pub generation: GenerationId,
}

/// Stable continuation point for one source-event/fact correlation query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEventCorrelationCursor {
    /// Event that anchors the correlation query; a cursor cannot be reused for
    /// a different source event.
    pub source_event: ChangeEventId,
    /// Generation of the last returned target event.
    pub after_generation: GenerationId,
}

/// Classification of a relationship between accepted change events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeEventRelationKind {
    /// Both events directly changed the same fact identity. This is not a
    /// claim that either event caused the other.
    HistoricallyCorrelated,
}

/// One explicitly non-causal relation between transitions sharing a changed
/// canonical fact identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEventCorrelation {
    /// Explicit relation class; never interpreted as causation.
    pub kind: ChangeEventRelationKind,
    /// Event from which the query was anchored.
    pub source_event: ChangeEventId,
    /// Generation containing the source event.
    pub source_generation: GenerationId,
    /// Later event that also directly changed `shared_fact`.
    pub target_event: ChangeEventId,
    /// Generation containing the target event.
    pub target_generation: GenerationId,
    /// Exact canonical fact identity serving as the relation's evidence.
    pub shared_fact: FactRef,
}

/// Stable address of an explicit or derived cross-fact consequence relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LineageEndpoint {
    ChangeEvent(ChangeEventId),
    FactVersion(FactVersionRef),
}

/// Typed relationship represented by a consequence edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsequenceKind {
    DirectDependencyEffect,
    GeneratedArtifactEffect,
    DerivedSemanticEffect,
    ContractEffect,
    BuildEffect,
    TestEffect,
    DeploymentEffect,
    RuntimeObservedEffect,
    DeclaredMigrationEffect,
    ChangeDependency,
    PossibleDownstreamEffect,
}

/// How SyntaxMesh obtained a consequence assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsequenceDerivation {
    /// Explicitly asserted by a producer; evidence and provenance are retained.
    Explicit,
    /// Derived from earlier, independently evidenced consequence edges.
    DerivedFrom(Vec<ConsequenceEdgeId>),
}

/// Evidence-backed temporal relation between a change event or fact versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceEdge {
    pub id: ConsequenceEdgeId,
    pub source: LineageEndpoint,
    pub target: LineageEndpoint,
    pub kind: ConsequenceKind,
    /// Exact canonical fact versions supporting this relation.
    pub evidence: Vec<FactVersionRef>,
    pub derivation: ConsequenceDerivation,
    /// Producer asserting the explicit edge or the derivation rule.
    pub provenance: ProvenanceId,
}

impl ConsequenceEdge {
    /// Validate locally representable evidence and derivation rules.
    ///
    /// # Errors
    /// Returns an error if evidence/derivation is empty, duplicated, or
    /// self-referential.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.source == self.target {
            return Err(ModelError::SelfReferentialConsequence);
        }
        if self.evidence.is_empty() {
            return Err(ModelError::EmptyConsequenceEvidence);
        }
        check_unique(self.evidence.iter().copied())?;
        if let ConsequenceDerivation::DerivedFrom(parents) = &self.derivation {
            if parents.is_empty() {
                return Err(ModelError::EmptyConsequenceDerivation);
            }
            check_unique(parents.iter().copied())?;
            if parents.contains(&self.id) {
                return Err(ModelError::SelfReferentialConsequence);
            }
        }
        Ok(())
    }
}

/// Lineage mutation envelope kept separate from `GraphDelta` serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceDelta {
    pub add: Vec<ConsequenceEdge>,
    /// Retracts an edge from this generation forward without rewriting history.
    pub retract: Vec<ConsequenceRetraction>,
}

/// Provenance-backed retraction of one active consequence edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceRetraction {
    pub edge: ConsequenceEdgeId,
    pub provenance: ProvenanceId,
}

impl ConsequenceDelta {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.add.is_empty() && self.retract.is_empty()
    }

    /// Validate duplicate and contradictory edge mutations.
    ///
    /// # Errors
    /// Returns an error for an invalid edge, duplicate ID, or same-generation
    /// add/retract conflict.
    pub fn validate(&self) -> Result<(), ModelError> {
        for edge in &self.add {
            edge.validate()?;
            if !matches!(edge.derivation, ConsequenceDerivation::Explicit) {
                return Err(ModelError::UnsupportedConsequenceDerivation);
            }
        }
        check_unique(self.add.iter().map(|edge| edge.id))?;
        check_unique(self.retract.iter().map(|retraction| retraction.edge))?;
        let added = self.add.iter().map(|edge| edge.id).collect::<BTreeSet<_>>();
        if self
            .retract
            .iter()
            .any(|retraction| added.contains(&retraction.edge))
        {
            return Err(ModelError::ContradictoryConsequenceMutation);
        }
        Ok(())
    }
}

/// New publication contract that adds consequence assertions without changing
/// the serialized legacy graph/ChangeSet publication envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphDeltaWithConsequences {
    pub publication: GraphDeltaWithLineage,
    pub consequences: ConsequenceDelta,
}

impl GraphDeltaWithConsequences {
    /// Validate every locally checkable part of a combined publication.
    ///
    /// # Errors
    /// Returns a model error if either publication envelope is invalid.
    pub fn validate(&self) -> Result<(), ModelError> {
        self.publication.validate()?;
        self.consequences.validate()
    }
}

/// Broad, caller-declared purpose of a logical group of accepted events.
/// These values are descriptive only; SyntaxMesh does not execute workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetKind {
    Feature,
    BugFix,
    Refactor,
    Migration,
    SecurityFix,
    ArchitectureChange,
    DocumentationChange,
    PolicyChange,
    SchemaChange,
    DependencyUpgrade,
    Rollback,
    GeneratedChange,
    AgentChange,
    ManualGroup,
    Unknown,
}

/// A caller-supplied reference outside the canonical graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExternalReference {
    pub system: String,
    pub reference: String,
}

/// Explicitly declared logical grouping of accepted change events.
///
/// Parent links and external references are descriptive relationships, not
/// causal claims. The `provenance` fact records who asserted this definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub id: ChangeSetId,
    pub kind: ChangeSetKind,
    pub title: Option<String>,
    pub originating_intent: Option<NodeId>,
    pub parent_changes: Vec<ChangeSetId>,
    pub git_commits: Vec<ExternalReference>,
    pub pull_requests: Vec<ExternalReference>,
    pub issues: Vec<ExternalReference>,
    pub adrs: Vec<NodeId>,
    pub repositories: Vec<RepositoryId>,
    pub first_generation: GenerationId,
    pub last_generation: Option<GenerationId>,
    pub provenance: ProvenanceId,
}

/// Explicit, provenance-backed membership of one accepted event in one set.
///
/// Membership is independent of `ChangeEvent`: asserting it never changes the
/// event identity or payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSetMembership {
    pub change_set: ChangeSetId,
    pub event: ChangeEventId,
    pub provenance: ProvenanceId,
}

/// Cursor bound to the logical set being paged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsForChangeSetCursor {
    pub change_set: ChangeSetId,
    /// Fixes the snapshot across pages, so later membership changes cannot
    /// reorder or silently omit results midway through traversal.
    pub as_of_generation: GenerationId,
    pub after_event_generation: GenerationId,
}

/// Continuation point for a fixed-generation consequence endpoint read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceEdgeCursor {
    pub endpoint: LineageEndpoint,
    pub as_of_generation: GenerationId,
    pub after_edge: ConsequenceEdgeId,
}

/// Continuation point for an endpoint's consequence assertions overlapping a
/// bounded inclusive generation range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceRangeCursor {
    pub endpoint: LineageEndpoint,
    pub from_generation: GenerationId,
    pub until_generation: GenerationId,
    pub after_edge: ConsequenceEdgeId,
}

/// One consequence assertion together with its accepted-generation validity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceEdgeVersion {
    pub edge: ConsequenceEdge,
    pub valid_from: GenerationId,
    /// Exclusive generation where this assertion was retracted, if any.
    pub valid_until: Option<GenerationId>,
}

/// One accepted event together with the explicit membership evidence visible
/// at a selected generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSetEvent {
    pub event: ChangeEvent,
    pub membership: ChangeSetMembership,
    pub membership_valid_from: GenerationId,
}

/// Stable key for a membership relation; membership validity is tracked by
/// accepted generation in the store, not encoded in this identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChangeSetMembershipKey {
    pub change_set: ChangeSetId,
    pub event: ChangeEventId,
}

/// Provenance-bearing assertion that closes a membership interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSetMembershipRemoval {
    pub key: ChangeSetMembershipKey,
    pub provenance: ProvenanceId,
}

/// Explicit lineage assertions accepted atomically alongside one graph delta.
/// Keeping this envelope separate preserves the serialized shape and digest
/// of legacy `GraphDelta` values when no lineage assertions are supplied.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSetDelta {
    pub upsert_sets: Vec<ChangeSet>,
    pub assign_events: Vec<ChangeSetMembership>,
    pub unassign_events: Vec<ChangeSetMembershipRemoval>,
}

impl ChangeSetDelta {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.upsert_sets.is_empty()
            && self.assign_events.is_empty()
            && self.unassign_events.is_empty()
    }

    /// Validate local duplicate/conflict rules for explicit lineage evidence.
    /// Stores resolve referenced generations, events, and provenance against
    /// accepted state.
    ///
    /// # Errors
    /// Returns a model error for duplicate lineage identities or contradictory
    /// membership operations.
    pub fn validate(&self) -> Result<(), ModelError> {
        check_unique(self.upsert_sets.iter().map(|set| set.id))?;
        for set in &self.upsert_sets {
            check_unique(set.parent_changes.iter().copied())?;
            if set.parent_changes.contains(&set.id) {
                return Err(ModelError::ChangeSetCannotParentItself);
            }
            check_unique(set.git_commits.iter())?;
            check_unique(set.pull_requests.iter())?;
            check_unique(set.issues.iter())?;
            check_unique(set.adrs.iter().copied())?;
            check_unique(set.repositories.iter().copied())?;
            if set
                .git_commits
                .iter()
                .chain(&set.pull_requests)
                .chain(&set.issues)
                .any(|reference| {
                    reference.system.trim().is_empty() || reference.reference.trim().is_empty()
                })
            {
                return Err(ModelError::InvalidExternalReference);
            }
        }
        let assigned = self
            .assign_events
            .iter()
            .map(|membership| ChangeSetMembershipKey {
                change_set: membership.change_set,
                event: membership.event,
            })
            .collect::<Vec<_>>();
        check_unique(assigned.iter().copied())?;
        check_unique(self.unassign_events.iter().map(|removal| removal.key))?;
        let unassigned = self
            .unassign_events
            .iter()
            .map(|removal| removal.key)
            .collect::<BTreeSet<_>>();
        if assigned.iter().any(|key| unassigned.contains(key)) {
            return Err(ModelError::ConflictingMembershipMutation);
        }
        Ok(())
    }
}

/// Append-only typed lineage assertion accepted with a generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationLineageEntry {
    pub generation: GenerationId,
    pub delta: ChangeSetDelta,
}

/// Consequence mutations accepted atomically with one graph generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationConsequenceEntry {
    pub generation: GenerationId,
    pub delta: ConsequenceDelta,
}

/// Full publication request with optional explicit change-set evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphDeltaWithLineage {
    pub graph: GraphDelta,
    pub lineage: ChangeSetDelta,
}

impl GraphDeltaWithLineage {
    /// Validate local duplicate/conflict rules; stores resolve references and
    /// active-version conflicts against the accepted state.
    ///
    /// # Errors
    /// Returns a model error for an invalid graph delta, duplicate lineage
    /// identities, or contradictory membership operations.
    pub fn validate(&self) -> Result<(), ModelError> {
        self.graph.validate()?;
        self.lineage.validate()
    }
}

/// Typed canonical payload returned for one retained fact version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactPayload {
    File(FileVersion),
    Provenance(Provenance),
    Node(Node),
    Edge(Edge),
}

/// Wall-clock time at which SyntaxMesh accepted a generation. This is
/// knowledge-time metadata, not modeled validity or producer observation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AcceptanceTime(pub u64);

impl AcceptanceTime {
    /// Return the Unix timestamp in nanoseconds.
    #[must_use]
    pub const fn as_unix_nanos(self) -> u64 {
        self.0
    }
}

/// Stable continuation point for a generation-acceptance timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedGenerationCursor {
    /// Timestamp of the last returned generation.
    pub accepted_at: AcceptanceTime,
    /// Generation ID tie-breaker when multiple generations share one timestamp.
    pub generation: GenerationId,
}

/// A file's exact bytes and location are separate from its stable identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileVersion {
    pub file_id: FileId,
    pub normalized_path: String,
    pub content_hash: [u8; 32],
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start_byte: u64,
    pub end_byte: u64,
}

impl SourceSpan {
    /// End is exclusive; an empty span is valid.
    ///
    /// # Errors
    /// Returns [`ModelError::InvalidSpan`] when the end precedes the start.
    pub const fn new(start_byte: u64, end_byte: u64) -> Result<Self, ModelError> {
        if start_byte > end_byte {
            return Err(ModelError::InvalidSpan);
        }
        Ok(Self {
            start_byte,
            end_byte,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file_id: FileId,
    pub content_hash: [u8; 32],
    pub span: SourceSpan,
}

/// Syntax form of one source-backed module import occurrence.
///
/// This describes syntax only; it does not imply that the module specifier has
/// been resolved to a file or package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    SideEffect,
    Default,
    Named,
    Namespace,
    Dynamic,
    CommonJs,
    /// Python `import package.module [as name]` source syntax.
    /// Appended to preserve existing bincode variant ordinals.
    PythonModule,
    /// Python `from module import name [as alias]` source syntax.
    /// Appended to preserve existing bincode variant ordinals.
    PythonFrom,
    /// Python `from module import *` source syntax.
    /// Appended to preserve existing bincode variant ordinals.
    PythonStar,
    /// Rust named `use`-tree binding. Its specifier is an import path, not
    /// necessarily a module specifier.
    /// Appended to preserve existing bincode variant ordinals.
    RustUse,
    /// Rust `use path::*` source syntax.
    /// Appended to preserve existing bincode variant ordinals.
    RustGlob,
    /// Static `namespace.member` access sourced from an ECMAScript namespace
    /// import. Appended to preserve existing bincode variant ordinals.
    NamespaceMember,
}

/// Source-backed module import, preserved independently from resolver output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRecord {
    pub id: ImportRecordId,
    /// Per-file module node containing this occurrence.
    pub module: NodeId,
    /// Exact module specifier or language-specific import target path.
    pub specifier: String,
    pub kind: ImportKind,
    /// Name selected from the imported module, when the syntax names one.
    pub imported_name: Option<String>,
    /// Local binding introduced by the import, when there is one.
    pub local_name: Option<String>,
    /// Whether the source marks this import as type-only.
    pub type_only: bool,
    pub source: SourceLocation,
    pub provenance: ProvenanceId,
}

/// Syntax form of one source-backed export occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Local,
    Default,
    NamedReExport,
    NamespaceReExport,
    StarReExport,
}

/// Source-backed module export, preserved independently from graph edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRecord {
    pub id: ExportRecordId,
    /// Per-file module node containing this occurrence.
    pub module: NodeId,
    /// Present only for a re-export from another module.
    pub source_specifier: Option<String>,
    pub kind: ExportKind,
    pub exported_name: Option<String>,
    pub local_name: Option<String>,
    /// Whether the source marks this export as type-only.
    pub type_only: bool,
    pub source: SourceLocation,
    pub provenance: ProvenanceId,
}

/// Certainty states what kind of support a fact has; it is not a score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceClass {
    SourceFact,
    StaticallyResolved,
    Heuristic,
    SemanticInference,
    UserAsserted,
    RuntimeObserved,
    CommittedTransition,
    /// A typed non-success result from a configured module resolver.
    /// Appended to preserve existing bincode variant ordinals.
    ResolutionDiagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub id: ProvenanceId,
    pub producer_namespace: String,
    pub producer_version: String,
    pub evidence_class: EvidenceClass,
    pub source: Option<SourceLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Repository,
    File,
    Module,
    Function,
    Struct,
    Enum,
    Trait,
    Test,
    External {
        namespace: String,
        kind: String,
    },
    Reference {
        relation: RelationKind,
    },
    UnresolvedReference {
        relation: RelationKind,
    },
    AmbiguousReference {
        relation: RelationKind,
    },
    RuntimeObservation,
    /// A source-language class declaration, distinct from a Rust-style struct.
    /// Appended to preserve existing bincode variant ordinals.
    Class,
    /// One unresolved, source-backed module import occurrence.
    /// Appended to preserve existing bincode variant ordinals.
    Import {
        specifier: String,
        kind: ImportKind,
        imported_name: Option<String>,
        local_name: Option<String>,
        type_only: bool,
    },
    /// One source-backed module export or re-export occurrence.
    /// Appended to preserve existing bincode variant ordinals.
    Export {
        source_specifier: Option<String>,
        kind: ExportKind,
        exported_name: Option<String>,
        local_name: Option<String>,
        type_only: bool,
    },
    /// A generation-scoped, provenance-backed non-success resolver outcome.
    /// Appended to preserve existing bincode variant ordinals.
    ModuleResolutionDiagnostic {
        occurrence: NodeId,
        status: ModuleResolutionDiagnosticStatus,
        candidate_paths: Vec<String>,
    },
    /// One Bash source file, appended to preserve existing bincode ordinals.
    Script,
    /// One documentation source file, appended to preserve existing ordinals.
    Document,
    /// One source heading section within a documentation file.
    Section,
    /// One source-grounded prose block within a documentation file.
    /// Appended to preserve existing bincode variant ordinals.
    DocumentChunk,
}

/// Portable classification of a module-resolution result that did not produce
/// a link to an indexed module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModuleResolutionDiagnosticStatus {
    Unresolved,
    Ambiguous,
    Invalid,
    TargetNotIndexed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationKind {
    Defines,
    Imports,
    References,
    Calls,
    Implements,
    External {
        namespace: String,
        relation: String,
    },
    /// A static resolution edge. Import occurrences may point to both their
    /// resolved Module and, when uniquely identified, an Export occurrence;
    /// inspect the target node kind to distinguish module from binding.
    ResolvesTo,
    /// A module contains one source-backed export occurrence.
    /// Appended to preserve existing bincode variant ordinals.
    Exports,
    /// A source import/export occurrence has a non-success resolution result.
    /// Appended to preserve existing bincode variant ordinals.
    HasResolutionDiagnostic,
    /// A script/document structurally contains declarations or sections.
    Contains,
}

/// Extension-owned bytes retain their schema and namespace without teaching
/// the language-independent core a private product's data model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionPayload {
    pub namespace: String,
    pub schema_version: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub owner_file: Option<FileId>,
    pub source: Option<SourceLocation>,
    pub provenance: ProvenanceId,
    pub extension_payload: Option<ExtensionPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub relation: RelationKind,
    pub provenance: ProvenanceId,
    pub extension_payload: Option<ExtensionPayload>,
}

/// A requested atomic transition; store implementations reject stale bases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphDelta {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub run_id: IndexRunId,
    pub expected_base: Option<GenerationId>,
    pub next_generation: GenerationId,
    pub changed_files: Vec<FileVersion>,
    pub removed_files: Vec<FileId>,
    pub upsert_provenance: Vec<Provenance>,
    pub upsert_nodes: Vec<Node>,
    pub upsert_edges: Vec<Edge>,
    pub remove_nodes: Vec<NodeId>,
    pub remove_edges: Vec<EdgeId>,
}

impl GraphDelta {
    /// Check local consistency before a store performs referential checks.
    ///
    /// # Errors
    /// Returns an error for duplicate IDs or a generation equal to its base.
    pub fn validate(&self) -> Result<(), ModelError> {
        let has_mutations = !self.changed_files.is_empty()
            || !self.removed_files.is_empty()
            || !self.upsert_provenance.is_empty()
            || !self.upsert_nodes.is_empty()
            || !self.upsert_edges.is_empty()
            || !self.remove_nodes.is_empty()
            || !self.remove_edges.is_empty();
        if has_mutations && self.expected_base == Some(self.next_generation) {
            return Err(ModelError::SameGeneration);
        }
        check_unique(self.changed_files.iter().map(|file| file.file_id))?;
        check_unique(self.removed_files.iter().copied())?;
        check_unique(self.upsert_provenance.iter().map(|item| item.id))?;
        check_unique(self.upsert_nodes.iter().map(|item| item.id))?;
        check_unique(self.upsert_edges.iter().map(|item| item.id))?;
        check_unique(self.remove_nodes.iter().copied())?;
        check_unique(self.remove_edges.iter().copied())?;
        Ok(())
    }
}

fn check_unique<T: Ord>(values: impl IntoIterator<Item = T>) -> Result<(), ModelError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ModelError::DuplicateIdentity);
        }
    }
    Ok(())
}

/// StateChronicle verifies a published generation; canonical facts are already
/// durable before verification begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenerationStatus {
    Durable,
    VerificationPending,
    Verified,
    VerificationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationManifest {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation: GenerationId,
    pub parent: Option<GenerationId>,
    pub graph_root: [u8; 32],
    pub configuration_hash: [u8; 32],
    pub extractor_set_hash: [u8; 32],
    pub schema_version: u32,
    pub status: GenerationStatus,
}

/// Immutable accepted transition retained for historical graph reconstruction.
/// A legacy store with no old deltas begins at a full-state anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationHistoryEntry {
    pub manifest: GenerationManifest,
    pub delta: Option<GraphDelta>,
    pub anchor: Option<GraphSnapshot>,
}

/// Complete logical graph state at one accepted generation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub files: Vec<FileVersion>,
    pub provenance: Vec<Provenance>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelError {
    InvalidSpan,
    DuplicateIdentity,
    SameGeneration,
    ConflictingMembershipMutation,
    ChangeSetCannotParentItself,
    InvalidExternalReference,
    EmptyConsequenceEvidence,
    EmptyConsequenceDerivation,
    UnsupportedConsequenceDerivation,
    SelfReferentialConsequence,
    ContradictoryConsequenceMutation,
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ModelError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documentation_and_script_facts_round_trip_with_append_only_kinds() {
        let kinds = [
            NodeKind::Script,
            NodeKind::Document,
            NodeKind::Section,
            NodeKind::DocumentChunk,
        ];
        for kind in kinds {
            let encoded = bincode::serialize(&kind);
            assert!(matches!(
                encoded.and_then(|bytes| bincode::deserialize::<NodeKind>(&bytes)),
                Ok(decoded) if decoded == kind
            ));
        }
        let relation = RelationKind::Contains;
        let encoded = bincode::serialize(&relation);
        assert!(matches!(
            encoded.and_then(|bytes| bincode::deserialize::<RelationKind>(&bytes)),
            Ok(decoded) if decoded == relation
        ));
        let legacy_class = bincode::serialize(&NodeKind::Class);
        assert!(matches!(legacy_class, Ok(bytes) if bytes == [13, 0, 0, 0]));
        let appended_section = bincode::serialize(&NodeKind::Section);
        assert!(matches!(appended_section, Ok(bytes) if bytes == [19, 0, 0, 0]));
        let appended_chunk = bincode::serialize(&NodeKind::DocumentChunk);
        assert!(matches!(appended_chunk, Ok(bytes) if bytes == [20, 0, 0, 0]));
        let appended_relation = bincode::serialize(&RelationKind::Contains);
        assert!(matches!(appended_relation, Ok(bytes) if bytes == [9, 0, 0, 0]));
        let appended_rust_use = bincode::serialize(&ImportKind::RustUse);
        assert!(matches!(appended_rust_use, Ok(bytes) if bytes == [9, 0, 0, 0]));
        let appended_rust_glob = bincode::serialize(&ImportKind::RustGlob);
        assert!(matches!(appended_rust_glob, Ok(bytes) if bytes == [10, 0, 0, 0]));
        let appended_namespace_member = bincode::serialize(&ImportKind::NamespaceMember);
        assert!(matches!(appended_namespace_member, Ok(bytes) if bytes == [11, 0, 0, 0]));
    }

    #[test]
    fn import_export_source_facts_round_trip_as_typed_records() {
        let source = SourceLocation {
            file_id: FileId::derive(&[b"src/main.ts"]),
            content_hash: [7; 32],
            span: SourceSpan {
                start_byte: 11,
                end_byte: 24,
            },
        };
        let provenance = ProvenanceId::derive(&[b"oxc"]);
        let module = NodeId::derive(&[b"src/main.ts"]);
        let import = ImportRecord {
            id: ImportRecordId::derive(&[b"src/main.ts", b"named", b"./utils", b"0"]),
            module,
            specifier: "./utils".to_owned(),
            kind: ImportKind::Named,
            imported_name: Some("format".to_owned()),
            local_name: Some("prettyFormat".to_owned()),
            type_only: true,
            source: source.clone(),
            provenance,
        };
        let export = ExportRecord {
            id: ExportRecordId::derive(&[b"src/main.ts", b"named-re-export", b"./utils", b"0"]),
            module,
            source_specifier: Some("./utils".to_owned()),
            kind: ExportKind::NamedReExport,
            exported_name: Some("format".to_owned()),
            local_name: Some("prettyFormat".to_owned()),
            type_only: true,
            source,
            provenance,
        };

        let import_round_trip = serde_json::to_vec(&import)
            .and_then(|encoded| serde_json::from_slice::<ImportRecord>(&encoded));
        let export_round_trip = serde_json::to_vec(&export)
            .and_then(|encoded| serde_json::from_slice::<ExportRecord>(&encoded));
        assert!(matches!(import_round_trip, Ok(record) if record == import));
        assert!(matches!(export_round_trip, Ok(record) if record == export));
    }

    #[test]
    fn identity_is_domain_and_boundary_separated() {
        assert_ne!(
            NodeId::derive(&[b"ab", b"c"]),
            NodeId::derive(&[b"a", b"bc"])
        );
        assert_ne!(NodeId::derive(&[b"x"]).0, EdgeId::derive(&[b"x"]).0);
        assert_ne!(
            ChangeSetId::derive(&[b"x"]).0,
            ChangeEventId::derive(&[b"x"]).0
        );
    }

    #[test]
    fn explicit_membership_is_a_separate_provenance_backed_fact() {
        let membership = ChangeSetMembership {
            change_set: ChangeSetId::derive(&[b"release-1"]),
            event: ChangeEventId::derive(&[b"generation-1"]),
            provenance: ProvenanceId::derive(&[b"caller-assertion"]),
        };
        assert_eq!(membership.change_set, ChangeSetId::derive(&[b"release-1"]));
        assert_eq!(membership.event, ChangeEventId::derive(&[b"generation-1"]));
        assert_eq!(
            membership.provenance,
            ProvenanceId::derive(&[b"caller-assertion"])
        );
    }

    fn sample_consequence(id: ConsequenceEdgeId) -> ConsequenceEdge {
        ConsequenceEdge {
            id,
            source: LineageEndpoint::ChangeEvent(ChangeEventId::derive(&[b"source-event"])),
            target: LineageEndpoint::FactVersion(FactVersionRef {
                fact: FactRef::Node(NodeId::derive(&[b"target-node"])),
                valid_from: GenerationId::derive(&[b"target-generation"]),
            }),
            kind: ConsequenceKind::DirectDependencyEffect,
            evidence: vec![FactVersionRef {
                fact: FactRef::Edge(EdgeId::derive(&[b"evidence-edge"])),
                valid_from: GenerationId::derive(&[b"evidence-generation"]),
            }],
            derivation: ConsequenceDerivation::Explicit,
            provenance: ProvenanceId::derive(&[b"consequence-producer"]),
        }
    }

    #[test]
    fn explicit_consequence_requires_distinct_exact_evidence() {
        let mut empty_edge = sample_consequence(ConsequenceEdgeId::derive(&[b"consequence"]));
        assert_eq!(empty_edge.validate(), Ok(()));

        empty_edge.evidence.clear();
        assert_eq!(
            empty_edge.validate(),
            Err(ModelError::EmptyConsequenceEvidence)
        );

        let mut duplicate_edge = sample_consequence(ConsequenceEdgeId::derive(&[b"consequence"]));
        let existing_evidence = duplicate_edge.evidence.clone();
        duplicate_edge.evidence.extend(existing_evidence);
        assert_eq!(
            duplicate_edge.validate(),
            Err(ModelError::DuplicateIdentity)
        );
    }

    #[test]
    fn consequence_delta_accepts_explicit_edges_only_and_rejects_self_edges() {
        let mut self_edge = sample_consequence(ConsequenceEdgeId::derive(&[b"consequence"]));
        self_edge.target = self_edge.source;
        assert_eq!(
            self_edge.validate(),
            Err(ModelError::SelfReferentialConsequence)
        );

        let mut derived_edge = sample_consequence(ConsequenceEdgeId::derive(&[b"consequence"]));
        derived_edge.derivation =
            ConsequenceDerivation::DerivedFrom(vec![ConsequenceEdgeId::derive(&[b"parent"])]);
        assert_eq!(
            ConsequenceDelta {
                add: vec![derived_edge],
                retract: Vec::new(),
            }
            .validate(),
            Err(ModelError::UnsupportedConsequenceDerivation)
        );
    }

    #[test]
    fn derived_consequence_requires_a_non_recursive_parent_chain() {
        let id = ConsequenceEdgeId::derive(&[b"consequence"]);
        let mut edge = sample_consequence(id);
        edge.derivation = ConsequenceDerivation::DerivedFrom(Vec::new());
        assert_eq!(edge.validate(), Err(ModelError::EmptyConsequenceDerivation));

        edge.derivation = ConsequenceDerivation::DerivedFrom(vec![id]);
        assert_eq!(edge.validate(), Err(ModelError::SelfReferentialConsequence));

        edge.derivation = ConsequenceDerivation::DerivedFrom(vec![
            ConsequenceEdgeId::derive(&[b"parent"]),
            ConsequenceEdgeId::derive(&[b"parent"]),
        ]);
        assert_eq!(edge.validate(), Err(ModelError::DuplicateIdentity));
    }

    #[test]
    fn consequence_delta_rejects_duplicate_and_contradictory_mutations() {
        let edge = sample_consequence(ConsequenceEdgeId::derive(&[b"consequence"]));
        assert_eq!(
            ConsequenceDelta {
                add: vec![edge.clone()],
                retract: vec![ConsequenceRetraction {
                    edge: edge.id,
                    provenance: ProvenanceId::derive(&[b"retractor"]),
                }],
            }
            .validate(),
            Err(ModelError::ContradictoryConsequenceMutation)
        );
        assert_eq!(
            ConsequenceDelta {
                add: vec![edge.clone(), edge],
                retract: vec![],
            }
            .validate(),
            Err(ModelError::DuplicateIdentity)
        );
    }

    #[test]
    fn lineage_delta_rejects_duplicate_or_conflicting_membership_mutations() {
        let repository = RepositoryId::derive(&[b"repo"]);
        let worktree = WorktreeId::derive(&[b"worktree"]);
        let event = ChangeEventId::derive(&[b"event"]);
        let change_set = ChangeSetId::derive(&[b"set"]);
        let key = ChangeSetMembershipKey { change_set, event };
        let request = GraphDeltaWithLineage {
            graph: GraphDelta {
                repository,
                worktree,
                run_id: IndexRunId::derive(&[b"run"]),
                expected_base: None,
                next_generation: GenerationId::derive(&[b"next"]),
                changed_files: vec![],
                removed_files: vec![],
                upsert_provenance: vec![],
                upsert_nodes: vec![],
                upsert_edges: vec![],
                remove_nodes: vec![],
                remove_edges: vec![],
            },
            lineage: ChangeSetDelta {
                assign_events: vec![ChangeSetMembership {
                    change_set,
                    event,
                    provenance: ProvenanceId::derive(&[b"assertion"]),
                }],
                unassign_events: vec![ChangeSetMembershipRemoval {
                    key,
                    provenance: ProvenanceId::derive(&[b"removal"]),
                }],
                ..ChangeSetDelta::default()
            },
        };
        assert_eq!(
            request.validate(),
            Err(ModelError::ConflictingMembershipMutation)
        );
    }

    #[test]
    fn lineage_delta_rejects_self_parent_and_empty_external_references() {
        let change_set = ChangeSetId::derive(&[b"set"]);
        let base = ChangeSet {
            id: change_set,
            kind: ChangeSetKind::ManualGroup,
            title: None,
            originating_intent: None,
            parent_changes: vec![change_set],
            git_commits: vec![],
            pull_requests: vec![],
            issues: vec![],
            adrs: vec![],
            repositories: vec![],
            first_generation: GenerationId::derive(&[b"first"]),
            last_generation: None,
            provenance: ProvenanceId::derive(&[b"p"]),
        };
        let graph = GraphDelta {
            repository: RepositoryId::derive(&[b"repo"]),
            worktree: WorktreeId::derive(&[b"worktree"]),
            run_id: IndexRunId::derive(&[b"run"]),
            expected_base: None,
            next_generation: GenerationId::derive(&[b"next"]),
            changed_files: vec![],
            removed_files: vec![],
            upsert_provenance: vec![],
            upsert_nodes: vec![],
            upsert_edges: vec![],
            remove_nodes: vec![],
            remove_edges: vec![],
        };
        let mut request = GraphDeltaWithLineage {
            graph,
            lineage: ChangeSetDelta {
                upsert_sets: vec![base.clone()],
                ..ChangeSetDelta::default()
            },
        };
        assert_eq!(
            request.validate(),
            Err(ModelError::ChangeSetCannotParentItself)
        );

        let mut non_self_parent = base;
        non_self_parent.parent_changes.clear();
        non_self_parent.git_commits.push(ExternalReference {
            system: " ".into(),
            reference: "abc123".into(),
        });
        request.lineage.upsert_sets = vec![non_self_parent];
        assert_eq!(
            request.validate(),
            Err(ModelError::InvalidExternalReference)
        );
    }

    #[test]
    fn delta_rejects_duplicate_node_ids() {
        let node = Node {
            id: NodeId::derive(&[b"same"]),
            kind: NodeKind::Function,
            name: "same".into(),
            owner_file: None,
            source: None,
            provenance: ProvenanceId::derive(&[b"p"]),
            extension_payload: None,
        };
        let delta = GraphDelta {
            repository: RepositoryId::derive(&[b"repo"]),
            worktree: WorktreeId::derive(&[b"worktree"]),
            run_id: IndexRunId::derive(&[b"run"]),
            expected_base: None,
            next_generation: GenerationId::derive(&[b"one"]),
            changed_files: vec![],
            removed_files: vec![],
            upsert_provenance: vec![],
            upsert_nodes: vec![node.clone(), node],
            upsert_edges: vec![],
            remove_nodes: vec![],
            remove_edges: vec![],
        };
        assert_eq!(delta.validate(), Err(ModelError::DuplicateIdentity));
    }

    #[test]
    fn empty_same_generation_delta_is_an_idempotent_noop() {
        let generation = GenerationId::derive(&[b"same"]);
        let delta = GraphDelta {
            repository: RepositoryId::derive(&[b"repo"]),
            worktree: WorktreeId::derive(&[b"worktree"]),
            run_id: IndexRunId::derive(&[b"run"]),
            expected_base: Some(generation),
            next_generation: generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        };
        assert_eq!(delta.validate(), Ok(()));
    }
}
