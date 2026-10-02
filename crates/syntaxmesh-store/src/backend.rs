//! Canonical graph storage port. A backend must publish one whole generation
//! atomically and reject a delta whose expected base is stale.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "benchmark-instrumentation")]
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    PersistentFactKey, PersistentFactMutation, PersistentFactTree, PersistentFactTreeCache,
    generation_root_v2,
};
use imbl::{OrdMap, Vector};
use serde::{Deserialize, Serialize};
use syntaxmesh_core::{
    AcceptanceTime, AcceptedGenerationCursor, ChangeEvent, ChangeEventCursor, ChangeSetDelta,
    ChangeSetEvent, ChangeSetId, ChangeSetMembership, ChangeSetMembershipKey, ConsequenceDelta,
    ConsequenceEdge, ConsequenceEdgeCursor, ConsequenceEdgeId, ConsequenceEdgeVersion,
    ConsequenceRangeCursor, Edge, EdgeDirection, EdgeId, EventsForChangeSetCursor, FactPayload,
    FactRef, FactVersionRef, FileId, FileVersion, GenerationConsequenceEntry,
    GenerationHistoryEntry, GenerationId, GenerationLineageEntry, GenerationManifest,
    GenerationStatus, GraphDelta, GraphDeltaWithConsequences, GraphDeltaWithLineage, GraphSnapshot,
    LineageEndpoint, Node, NodeId, ObservationTime, ObservedFactCursor, Provenance, ProvenanceId,
    RepositoryId, WorktreeId,
};

#[cfg(feature = "benchmark-instrumentation")]
struct DeltaApplicationProfile {
    enabled: bool,
    stage_started: Instant,
}

#[cfg(feature = "benchmark-instrumentation")]
impl DeltaApplicationProfile {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("SYNTAXMESH_STORE_PROFILE").is_some(),
            stage_started: Instant::now(),
        }
    }

    fn mark(&mut self, stage: &str) {
        if self.enabled {
            eprintln!(
                "store_delta_stage stage={stage} elapsed_us={}",
                self.stage_started.elapsed().as_micros()
            );
            self.stage_started = Instant::now();
        }
    }
}

#[cfg(feature = "benchmark-instrumentation")]
struct StoreCloneProfile {
    enabled: bool,
    stage_started: Instant,
}

#[cfg(feature = "benchmark-instrumentation")]
impl StoreCloneProfile {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("SYNTAXMESH_STORE_PROFILE").is_some(),
            stage_started: Instant::now(),
        }
    }

    fn mark(&mut self, stage: &str) {
        if self.enabled {
            eprintln!(
                "store_clone_stage stage={stage} elapsed_us={}",
                self.stage_started.elapsed().as_micros()
            );
            self.stage_started = Instant::now();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    StaleBase {
        expected: Option<GenerationId>,
        actual: Option<GenerationId>,
    },
    RecordConflict(String),
    InvalidDelta(String),
    InvalidTemporalRange,
    InvalidPageLimit,
    Unsupported(String),
    Integrity(String),
    Backend(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for StoreError {}

/// One retained version of a graph node, with a half-open generation interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHistoryVersion {
    /// First generation in which this payload is active.
    pub valid_from: GenerationId,
    /// First generation in which this payload is no longer active.
    pub valid_until: Option<GenerationId>,
    /// Exact node payload for this version.
    pub node: Node,
}

/// One typed canonical fact payload and its retained generation-validity interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactHistoryVersion {
    pub valid_from: GenerationId,
    pub valid_until: Option<GenerationId>,
    /// Producer occurrence time when this fact carries it.
    pub observed_at: Option<ObservationTime>,
    /// Engine acceptance time for the generation that introduced this version.
    pub accepted_at: Option<AcceptanceTime>,
    pub payload: FactPayload,
}

/// One canonical fact version selected by its producer-reported event time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedFactVersion {
    pub fact: FactRef,
    pub version: FactHistoryVersion,
}

/// Bounded page of observation-time fact versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedFactPage {
    pub items: Vec<ObservedFactVersion>,
    pub next_cursor: Option<ObservedFactCursor>,
}

/// One globally ordered retained fact version for a pinned historical export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactHistoryEntry {
    pub fact: FactRef,
    /// One-based canonical sequence at which this version became valid.
    pub valid_from_sequence: u64,
    /// One-based canonical sequence at which this version stopped being valid
    /// by the page's pinned generation, if already known at that snapshot.
    pub valid_until_sequence: Option<u64>,
    pub version: FactHistoryVersion,
}

/// Cursor for a globally ordered, snapshot-pinned fact-history scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactHistoryCursor {
    pub as_of_generation: GenerationId,
    pub after_fact: FactRef,
    pub after_valid_from: GenerationId,
}

/// Bounded page of canonical fact versions, ordered by fact identity then
/// generation-history sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactHistoryPage {
    pub items: Vec<FactHistoryEntry>,
    pub next_cursor: Option<FactHistoryCursor>,
}

/// Cursor for one generation-pinned page of fact versions opened or closed
/// during an accepted transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactVersionChangeCursor {
    pub generation: GenerationId,
    pub after_fact: FactRef,
    pub after_valid_from_sequence: u64,
}

/// Bounded page of fact versions opened or closed at one accepted generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactVersionChangePage {
    pub items: Vec<FactHistoryEntry>,
    pub next_cursor: Option<FactVersionChangeCursor>,
}

/// Maximum fact-version changes returned in one generation-scoped page.
pub const MAX_FACT_VERSION_CHANGE_PAGE_SIZE: usize = 1_024;

/// One accepted transition in a requested generation range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationChange {
    /// One-based canonical position of this accepted transition.
    pub sequence: u64,
    /// Manifest produced by this transition.
    pub manifest: GenerationManifest,
    /// Deterministic graph facts added, changed, or removed in the transition.
    pub delta: GraphDelta,
}

/// Cursor for one pinned, bounded generation-change range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationChangeCursor {
    pub from_generation: GenerationId,
    pub to_generation: GenerationId,
    pub after_sequence: u64,
    pub after_generation: GenerationId,
}

/// Bounded page of immutable accepted generation changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationChangePage {
    pub items: Vec<GenerationChange>,
    pub next_cursor: Option<GenerationChangeCursor>,
}

/// Maximum accepted transitions in a generation-change page.
pub const MAX_GENERATION_CHANGE_PAGE_SIZE: usize = 256;

/// One accepted generation in an acceptance-time range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedGeneration {
    /// Engine-assigned knowledge time for the generation.
    pub accepted_at: AcceptanceTime,
    /// Generation manifest associated with that acceptance.
    pub manifest: GenerationManifest,
}

/// Bounded page of known acceptances and an optional continuation cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedGenerationPage {
    /// Accepted generations in timestamp/generation order.
    pub items: Vec<AcceptedGeneration>,
    /// Cursor to pass to the next query when more range results exist.
    pub next_cursor: Option<AcceptedGenerationCursor>,
}

/// Bounded page of accepted change events linked to one canonical fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEventPage {
    pub items: Vec<ChangeEvent>,
    pub next_cursor: Option<ChangeEventCursor>,
}

/// Bounded page of explicitly grouped accepted events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventsForChangeSetPage {
    pub items: Vec<ChangeSetEvent>,
    pub next_cursor: Option<EventsForChangeSetCursor>,
}

/// Bounded page of active consequence edges incident to one exact endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsequenceEdgePage {
    pub items: Vec<ConsequenceEdge>,
    pub next_cursor: Option<ConsequenceEdgeCursor>,
}

/// Bounded endpoint page of consequence assertions overlapping a generation range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsequenceRangePage {
    pub items: Vec<ConsequenceEdgeVersion>,
    pub next_cursor: Option<ConsequenceRangeCursor>,
    /// Storage work counts emitted only in benchmark-instrumented builds.
    #[cfg(feature = "benchmark-instrumentation")]
    pub read_metrics: ConsequenceRangeReadMetrics,
}

impl ConsequenceRangePage {
    /// Construct a page independently of feature unification in its consumers.
    /// Optional counters default to unmeasured zero values.
    #[must_use]
    pub const fn new(
        items: Vec<ConsequenceEdgeVersion>,
        next_cursor: Option<ConsequenceRangeCursor>,
    ) -> Self {
        Self {
            items,
            next_cursor,
            #[cfg(feature = "benchmark-instrumentation")]
            read_metrics: ConsequenceRangeReadMetrics {
                sql_read_statements: 0,
                reference_generation_entries_materialized: 0,
                reference_consequence_entries_materialized: 0,
                reference_consequence_entries_scanned: 0,
                reference_consequence_mutations_scanned: 0,
                consequence_rows_returned: 0,
            },
        }
    }

    /// Attach counters measured by an instrumented adapter.
    #[cfg(feature = "benchmark-instrumentation")]
    #[must_use]
    pub const fn with_read_metrics(mut self, metrics: ConsequenceRangeReadMetrics) -> Self {
        self.read_metrics = metrics;
        self
    }
}

/// Low-level operation counts for temporal consequence-range benchmarks.
#[cfg(feature = "benchmark-instrumentation")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsequenceRangeReadMetrics {
    /// SQL read statements executed by a durable endpoint-range page.
    pub sql_read_statements: usize,
    /// Complete retained generation-history entries materialized by the reference path.
    pub reference_generation_entries_materialized: usize,
    /// Complete retained consequence-history entries materialized by the reference path.
    pub reference_consequence_entries_materialized: usize,
    /// Consequence-history entries traversed through the requested upper bound.
    pub reference_consequence_entries_scanned: usize,
    /// Add/retract mutation records examined in traversed consequence entries.
    pub reference_consequence_mutations_scanned: usize,
    /// Matching consequence versions returned after endpoint/range filtering.
    pub consequence_rows_returned: usize,
}

/// Bounded historical incident-edge page. `has_more` is true only when the
/// backend found an edge after the returned items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalEdgePage {
    pub items: Vec<Edge>,
    pub has_more: bool,
    /// Storage work counts emitted only in benchmark-instrumented builds.
    #[cfg(feature = "benchmark-instrumentation")]
    pub read_metrics: HistoricalEdgeReadMetrics,
}

impl HistoricalEdgePage {
    /// Construct a page independently of feature unification in its consumers.
    /// Optional counters default to unmeasured zero values.
    #[must_use]
    pub const fn new(items: Vec<Edge>, has_more: bool) -> Self {
        Self {
            items,
            has_more,
            #[cfg(feature = "benchmark-instrumentation")]
            read_metrics: HistoricalEdgeReadMetrics {
                sql_read_statements: 0,
                incidence_index_page_lookups: 0,
                incidence_index_pages_loaded: 0,
                edge_payload_rows_fetched: 0,
                reference_snapshots_materialized: 0,
            },
        }
    }

    /// Attach counters measured by an instrumented adapter.
    #[cfg(feature = "benchmark-instrumentation")]
    #[must_use]
    pub const fn with_read_metrics(mut self, metrics: HistoricalEdgeReadMetrics) -> Self {
        self.read_metrics = metrics;
        self
    }
}

/// Low-level read counts for temporal adjacency benchmarks; not a latency SLA.
#[cfg(feature = "benchmark-instrumentation")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoricalEdgeReadMetrics {
    /// SQL read statements executed by the durable page adapter.
    pub sql_read_statements: usize,
    /// Persistent incidence-tree cache lookups, including misses and retries.
    pub incidence_index_page_lookups: usize,
    /// Persistent incidence-tree pages fetched from durable storage.
    pub incidence_index_pages_loaded: usize,
    /// Historical edge payload rows fetched to hydrate the page.
    pub edge_payload_rows_fetched: usize,
    /// Complete reference snapshots materialized to answer the page.
    pub reference_snapshots_materialized: usize,
}

/// Maximum edge facts returned by a historical incidence-page request.
pub const MAX_HISTORICAL_EDGE_PAGE_SIZE: usize = 1_000;

/// Stable-ID-ordered node versions from one exact retained generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalNodePage {
    pub items: Vec<Node>,
    /// True only when another node exists after the returned items.
    pub has_more: bool,
}

/// Maximum node versions returned by one historical page request.
pub const MAX_HISTORICAL_NODE_PAGE_SIZE: usize = 1_000;

/// Stable-ID-ordered file versions from one exact retained generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalFilePage {
    pub items: Vec<FileVersion>,
    pub has_more: bool,
}

/// Maximum file versions returned by one historical page request.
pub const MAX_HISTORICAL_FILE_PAGE_SIZE: usize = 1_000;

/// Version of a ChangeSet declaration selected at one retained generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSetVersion {
    /// Canonical versioned declaration.
    pub change_set: syntaxmesh_core::ChangeSet,
    /// First accepted generation where this payload became valid.
    pub valid_from: GenerationId,
    /// Exclusive generation where a later declaration superseded it.
    pub valid_until: Option<GenerationId>,
}

type ChangeSetLineageState = (
    BTreeMap<ChangeSetId, syntaxmesh_core::ChangeSet>,
    BTreeMap<ChangeSetMembershipKey, (ChangeSetMembership, GenerationId)>,
);

const fn close_node_version(versions: &mut [NodeHistoryVersion], until: GenerationId) {
    if let Some(version) = versions.last_mut()
        && version.valid_until.is_none()
    {
        version.valid_until = Some(until);
    }
}

const fn close_fact_version(versions: &mut [FactHistoryVersion], until: GenerationId) {
    if let Some(version) = versions.last_mut()
        && version.valid_until.is_none()
    {
        version.valid_until = Some(until);
    }
}

fn fact_in_snapshot(snapshot: &GraphSnapshot, fact: FactRef) -> Option<FactPayload> {
    match fact {
        FactRef::File(id) => snapshot
            .files
            .iter()
            .find(|item| item.file_id == id)
            .cloned()
            .map(FactPayload::File),
        FactRef::Provenance(id) => snapshot
            .provenance
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .map(FactPayload::Provenance),
        FactRef::Node(id) => snapshot
            .nodes
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .map(FactPayload::Node),
        FactRef::Edge(id) => snapshot
            .edges
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .map(FactPayload::Edge),
    }
}

fn fact_in_delta(
    delta: &GraphDelta,
    fact: FactRef,
    active_version: Option<&FactPayload>,
) -> Option<Option<FactPayload>> {
    match fact {
        FactRef::File(id) => {
            if delta.removed_files.contains(&id) {
                Some(None)
            } else {
                delta
                    .changed_files
                    .iter()
                    .find(|item| item.file_id == id)
                    .cloned()
                    .map(FactPayload::File)
                    .map(Some)
            }
        }
        FactRef::Provenance(id) => delta
            .upsert_provenance
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .map(FactPayload::Provenance)
            .map(Some),
        FactRef::Node(id) => delta
            .upsert_nodes
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .map(FactPayload::Node)
            .map(Some)
            .or_else(|| delta.remove_nodes.contains(&id).then_some(None)),
        FactRef::Edge(id) => delta
            .upsert_edges
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .map(FactPayload::Edge)
            .map(Some)
            .or_else(|| {
                let incident_to_removed_node = active_version.is_some_and(|payload| {
                    let FactPayload::Edge(edge) = payload else {
                        return false;
                    };
                    delta
                        .remove_nodes
                        .iter()
                        .any(|node| edge.source == *node || edge.target == *node)
                });
                (delta.remove_edges.contains(&id) || incident_to_removed_node).then_some(None)
            }),
    }
}

fn fact_observation_time(payload: &FactPayload) -> Option<ObservationTime> {
    let FactPayload::Node(node) = payload else {
        return None;
    };
    if !matches!(node.kind, syntaxmesh_core::NodeKind::RuntimeObservation) {
        return None;
    }
    let extension_payload = node.extension_payload.as_ref()?;
    let value: serde_json::Value = serde_json::from_slice(&extension_payload.bytes).ok()?;
    value
        .get("observed_at_unix_nanos")?
        .as_u64()
        .map(ObservationTime)
}

fn acceptance_record_key(generation: GenerationId) -> String {
    format!("syntaxmesh.temporal.accepted-at/{}", generation.0.to_hex())
}

/// Logical behavior shared by Turso and the SQLite reference backend.
/// Implementations decide how to provide a consistent read snapshot.
pub trait GraphStore {
    /// Read the last published manifest for one repository worktree.
    ///
    /// # Errors
    /// Returns a store error if the snapshot cannot be read or validated.
    fn current_generation(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationManifest>, StoreError>;

    /// Read the manifest for one exact generation.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    fn manifest(&self, generation: GenerationId) -> Result<GenerationManifest, StoreError>;

    /// Read immutable accepted generation transitions in parent-first order.
    /// This is graph history, not a StateChronicle verification log.
    ///
    /// # Errors
    /// Returns a store error if history cannot be read or validated.
    fn generation_history(&self) -> Result<Vec<GenerationHistoryEntry>, StoreError>;

    /// Read the accepted history entry for the current generation in one scope.
    /// Durable adapters should resolve the current generation ID directly
    /// rather than loading the complete retained history.
    ///
    /// # Errors
    /// Returns a store error if the current manifest or its history entry
    /// cannot be read consistently.
    fn current_generation_entry(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationHistoryEntry>, StoreError> {
        let Some(current) = self.current_generation(repository, worktree)? else {
            return Ok(None);
        };
        Ok(self
            .generation_history()?
            .into_iter()
            .find(|entry| entry.manifest.generation == current.generation))
    }

    /// Read a bounded page of all canonical fact versions through one pinned
    /// generation. Durable adapters should use the composite fact-history
    /// identity/sequence index; the default reference implementation scans
    /// retained transitions.
    ///
    /// # Errors
    /// Returns an error if the snapshot is not retained, the cursor is bound
    /// to another snapshot, or stored fact history is malformed.
    fn fact_history_page(
        &self,
        as_of_generation: GenerationId,
        after: Option<FactHistoryCursor>,
        limit: usize,
    ) -> Result<FactHistoryPage, StoreError> {
        if limit == 0 {
            return Ok(FactHistoryPage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        if after.is_some_and(|cursor| cursor.as_of_generation != as_of_generation) {
            return Err(StoreError::InvalidDelta(
                "fact-history cursor is bound to a different generation".to_owned(),
            ));
        }
        let history = self.generation_history()?;
        let positions = history
            .iter()
            .enumerate()
            .map(|(position, entry)| (entry.manifest.generation, position))
            .collect::<BTreeMap<_, _>>();
        let Some(as_of_position) = positions.get(&as_of_generation).copied() else {
            return Err(StoreError::Integrity(
                "fact-history snapshot is not retained".to_owned(),
            ));
        };
        let sequence_at = |position: usize| {
            position
                .checked_add(1)
                .and_then(|sequence| u64::try_from(sequence).ok())
                .ok_or_else(|| StoreError::Integrity("generation sequence overflow".to_owned()))
        };
        if after.is_some_and(|cursor| {
            positions
                .get(&cursor.after_valid_from)
                .is_none_or(|position| *position > as_of_position)
        }) {
            return Err(StoreError::InvalidDelta(
                "fact-history cursor is outside its retained snapshot".to_owned(),
            ));
        }
        let mut facts = BTreeSet::new();
        let snapshot = self.historical_snapshot(as_of_generation)?;
        facts.extend(
            snapshot
                .files
                .into_iter()
                .map(|item| FactRef::File(item.file_id)),
        );
        facts.extend(
            snapshot
                .provenance
                .into_iter()
                .map(|item| FactRef::Provenance(item.id)),
        );
        facts.extend(
            snapshot
                .nodes
                .into_iter()
                .map(|item| FactRef::Node(item.id)),
        );
        facts.extend(
            snapshot
                .edges
                .into_iter()
                .map(|item| FactRef::Edge(item.id)),
        );
        for entry in history.iter().take(as_of_position.saturating_add(1)) {
            if let Some(delta) = &entry.delta {
                facts.extend(crate::delta_fact_candidates(delta));
            }
        }
        let mut entries = Vec::new();
        for fact in facts {
            for mut version in self.fact_history(fact)? {
                let Some(valid_from_position) = positions.get(&version.valid_from).copied() else {
                    return Err(StoreError::Integrity(
                        "fact version starts outside retained generation history".to_owned(),
                    ));
                };
                if valid_from_position > as_of_position {
                    continue;
                }
                let valid_from_sequence = sequence_at(valid_from_position)?;
                let valid_until_sequence = version
                    .valid_until
                    .map(|generation| {
                        positions.get(&generation).copied().ok_or_else(|| {
                            StoreError::Integrity(
                                "fact version ends outside retained generation history".to_owned(),
                            )
                        })
                    })
                    .transpose()?
                    .filter(|position| *position <= as_of_position)
                    .map(sequence_at)
                    .transpose()?;
                if version.valid_until.is_some() && valid_until_sequence.is_none() {
                    version.valid_until = None;
                }
                let after_position =
                    after.and_then(|cursor| positions.get(&cursor.after_valid_from).copied());
                if after.is_some_and(|cursor| {
                    cursor.after_fact > fact
                        || (cursor.after_fact == fact
                            && after_position
                                .is_some_and(|position| valid_from_position <= position))
                }) {
                    continue;
                }
                entries.push(FactHistoryEntry {
                    fact,
                    valid_from_sequence,
                    valid_until_sequence,
                    version,
                });
            }
        }
        entries.sort_by_key(|entry| {
            (
                entry.fact,
                positions
                    .get(&entry.version.valid_from)
                    .copied()
                    .unwrap_or(usize::MAX),
            )
        });
        let mut items = entries
            .into_iter()
            .take(limit.saturating_add(1))
            .collect::<Vec<_>>();
        let next_cursor = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|entry| FactHistoryCursor {
                as_of_generation,
                after_fact: entry.fact,
                after_valid_from: entry.version.valid_from,
            })
        } else {
            None
        };
        Ok(FactHistoryPage { items, next_cursor })
    }

    /// Read immutable typed ChangeSet declarations accepted with generations.
    ///
    /// # Errors
    /// Returns an error when the backend cannot read or validate its lineage journal.
    fn generation_lineage_history(&self) -> Result<Vec<GenerationLineageEntry>, StoreError> {
        Err(StoreError::Unsupported(
            "backend does not persist explicit ChangeSet lineage".to_owned(),
        ))
    }

    /// Read immutable consequence assertions by accepted generation.
    ///
    /// # Errors
    /// Returns an error when this backend does not retain consequence history.
    fn generation_consequence_history(
        &self,
    ) -> Result<Vec<GenerationConsequenceEntry>, StoreError> {
        Err(StoreError::Unsupported(
            "backend does not persist consequence history".to_owned(),
        ))
    }

    /// Atomically publish graph, ChangeSet lineage, and explicit consequence
    /// assertions. The default preserves compatibility for empty assertions.
    ///
    /// # Errors
    /// Returns a store error if validation/publication fails or consequences
    /// are unsupported by this backend.
    fn apply_delta_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        request
            .validate()
            .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
        if !request.consequences.is_empty() {
            return Err(StoreError::Unsupported(
                "backend does not atomically publish consequences".to_owned(),
            ));
        }
        self.apply_delta_with_lineage(request.publication, accepted_at)
    }

    /// Atomically publish one graph transition and its explicit ChangeSet
    /// declarations. The event identity remains derived from `graph` alone.
    ///
    /// # Errors
    /// Returns an error if validation fails, publication is stale, or this backend
    /// does not support atomic lineage publication.
    fn apply_delta_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        if !request.lineage.is_empty() {
            return Err(StoreError::Unsupported(
                "backend does not atomically publish explicit ChangeSet lineage".to_owned(),
            ));
        }
        self.apply_delta_with_acceptance_time(request.graph, accepted_at)
    }

    /// Read explicitly grouped events at one fixed generation snapshot.
    ///
    /// # Errors
    /// Returns an error for stale snapshots, invalid cursors, or unsupported backends.
    fn events_for_change_set(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
        after: Option<EventsForChangeSetCursor>,
        limit: usize,
    ) -> Result<EventsForChangeSetPage, StoreError> {
        let _ = (change_set, as_of, after, limit);
        Err(StoreError::Unsupported(
            "backend does not index explicit ChangeSet membership".to_owned(),
        ))
    }

    /// Read active consequence edges incident to one exact endpoint at a pinned generation.
    ///
    /// Durable adapters should use endpoint interval indexes; reference adapters may scan
    /// retained consequence deltas. Results are ordered by edge identity.
    ///
    /// # Errors
    /// Returns an error for a stale snapshot, a cursor bound to another query, or an
    /// unsupported backend.
    fn consequence_edges_for_endpoint(
        &self,
        endpoint: LineageEndpoint,
        as_of: GenerationId,
        after: Option<ConsequenceEdgeCursor>,
        limit: usize,
    ) -> Result<ConsequenceEdgePage, StoreError> {
        if limit == 0 {
            return Ok(ConsequenceEdgePage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        if let Some(cursor) = after
            && (cursor.endpoint != endpoint || cursor.as_of_generation != as_of)
        {
            return Err(StoreError::InvalidDelta(
                "consequence cursor is bound to a different endpoint or snapshot".to_owned(),
            ));
        }
        let history = self.generation_history()?;
        let as_of_position = history
            .iter()
            .position(|entry| entry.manifest.generation == as_of)
            .ok_or_else(|| {
                StoreError::Integrity("consequence snapshot is not retained".to_owned())
            })?;
        let retained = history
            .iter()
            .take(as_of_position.saturating_add(1))
            .map(|entry| entry.manifest.generation)
            .collect::<BTreeSet<_>>();
        let mut active = BTreeMap::new();
        for entry in self.generation_consequence_history()? {
            if retained.contains(&entry.generation) {
                for retraction in entry.delta.retract {
                    active.remove(&retraction.edge);
                }
                for edge in entry.delta.add {
                    active.insert(edge.id, edge);
                }
            }
        }
        let limit_plus_one = limit.saturating_add(1);
        let mut items = active
            .into_iter()
            .filter(|(id, edge)| {
                after.is_none_or(|cursor| *id > cursor.after_edge)
                    && (edge.source == endpoint || edge.target == endpoint)
            })
            .map(|(_, edge)| edge)
            .take(limit_plus_one)
            .collect::<Vec<_>>();
        let next_cursor = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|edge| ConsequenceEdgeCursor {
                endpoint,
                as_of_generation: as_of,
                after_edge: edge.id,
            })
        } else {
            None
        };
        Ok(ConsequenceEdgePage { items, next_cursor })
    }

    /// Read consequence assertions incident to an exact endpoint whose
    /// validity overlaps the inclusive retained-generation range. Durable
    /// stores should use endpoint/interval indexes; this default is a reference
    /// implementation over retained consequence deltas.
    ///
    /// # Errors
    /// Returns an error if either generation is not retained, the range is
    /// reversed, the cursor is bound to another request, or history is corrupt.
    fn consequence_edges_for_endpoint_range(
        &self,
        endpoint: LineageEndpoint,
        from_generation: GenerationId,
        until_generation: GenerationId,
        after: Option<ConsequenceRangeCursor>,
        limit: usize,
    ) -> Result<ConsequenceRangePage, StoreError> {
        if limit == 0 {
            return Ok(ConsequenceRangePage {
                items: Vec::new(),
                next_cursor: None,
                #[cfg(feature = "benchmark-instrumentation")]
                read_metrics: ConsequenceRangeReadMetrics::default(),
            });
        }
        if after.is_some_and(|cursor| {
            cursor.endpoint != endpoint
                || cursor.from_generation != from_generation
                || cursor.until_generation != until_generation
        }) {
            return Err(StoreError::InvalidDelta(
                "consequence range cursor is bound to a different query".to_owned(),
            ));
        }
        let history = self.generation_history()?;
        let Some(from) = history
            .iter()
            .position(|item| item.manifest.generation == from_generation)
        else {
            return Err(StoreError::StaleBase {
                expected: Some(from_generation),
                actual: history.last().map(|item| item.manifest.generation),
            });
        };
        let Some(until) = history
            .iter()
            .position(|item| item.manifest.generation == until_generation)
        else {
            return Err(StoreError::StaleBase {
                expected: Some(until_generation),
                actual: history.last().map(|item| item.manifest.generation),
            });
        };
        if from > until {
            return Err(StoreError::InvalidDelta(
                "consequence range starts after it ends".to_owned(),
            ));
        }
        let history_positions = history
            .iter()
            .enumerate()
            .map(|(index, item)| (item.manifest.generation, index))
            .collect::<BTreeMap<_, _>>();
        let mut active = BTreeMap::<ConsequenceEdgeId, (ConsequenceEdge, GenerationId)>::new();
        let mut versions = BTreeMap::<ConsequenceEdgeId, ConsequenceEdgeVersion>::new();
        let consequence_history = self.generation_consequence_history()?;
        #[cfg(feature = "benchmark-instrumentation")]
        let mut read_metrics = ConsequenceRangeReadMetrics {
            reference_generation_entries_materialized: history.len(),
            reference_consequence_entries_materialized: consequence_history.len(),
            ..ConsequenceRangeReadMetrics::default()
        };
        for entry in consequence_history {
            let index = history_positions
                .get(&entry.generation)
                .copied()
                .ok_or_else(|| {
                    StoreError::Integrity(
                        "consequence history references an unretained generation".to_owned(),
                    )
                })?;
            if index > until {
                break;
            }
            #[cfg(feature = "benchmark-instrumentation")]
            {
                read_metrics.reference_consequence_entries_scanned = read_metrics
                    .reference_consequence_entries_scanned
                    .saturating_add(1);
                read_metrics.reference_consequence_mutations_scanned = read_metrics
                    .reference_consequence_mutations_scanned
                    .saturating_add(entry.delta.add.len())
                    .saturating_add(entry.delta.retract.len());
            }
            for retraction in entry.delta.retract {
                if let Some((edge, valid_from)) = active.remove(&retraction.edge) {
                    versions.insert(
                        edge.id,
                        ConsequenceEdgeVersion {
                            edge,
                            valid_from,
                            valid_until: Some(entry.generation),
                        },
                    );
                }
            }
            for edge in entry.delta.add {
                active.insert(edge.id, (edge, entry.generation));
            }
        }
        for (edge, valid_from) in active.into_values() {
            versions.insert(
                edge.id,
                ConsequenceEdgeVersion {
                    edge,
                    valid_from,
                    valid_until: None,
                },
            );
        }
        let limit_plus_one = limit.saturating_add(1);
        let mut items = versions
            .into_values()
            .filter(|version| {
                let starts_by_until = history_positions
                    .get(&version.valid_from)
                    .copied()
                    .is_some_and(|position| position <= until);
                let ends_after_from = version
                    .valid_until
                    .and_then(|generation| history_positions.get(&generation).copied())
                    .is_none_or(|position| position > from);
                let incident = version.edge.source == endpoint || version.edge.target == endpoint;
                let after_id = after.is_none_or(|cursor| version.edge.id > cursor.after_edge);
                starts_by_until && ends_after_from && incident && after_id
            })
            .take(limit_plus_one)
            .collect::<Vec<_>>();
        let next_cursor = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|item| ConsequenceRangeCursor {
                endpoint,
                from_generation,
                until_generation,
                after_edge: item.edge.id,
            })
        } else {
            None
        };
        #[cfg(feature = "benchmark-instrumentation")]
        {
            read_metrics.consequence_rows_returned = items.len();
        }
        Ok(ConsequenceRangePage {
            items,
            next_cursor,
            #[cfg(feature = "benchmark-instrumentation")]
            read_metrics,
        })
    }

    /// Read the ChangeSet declaration valid at an exact retained generation.
    ///
    /// # Errors
    /// Returns an error for a stale snapshot or when this backend does not
    /// index explicit ChangeSet declarations.
    fn change_set_at(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
    ) -> Result<Option<ChangeSetVersion>, StoreError> {
        let _ = (change_set, as_of);
        Err(StoreError::Unsupported(
            "backend does not index explicit ChangeSet declarations".to_owned(),
        ))
    }

    /// Return the one-based durable position of a retained generation.
    ///
    /// Durable adapters should resolve this from the generation index;
    /// reference stores may find it in retained history.
    ///
    /// # Errors
    /// Returns a stale-generation error or an integrity error when the
    /// sequence cannot be represented.
    fn generation_sequence(&self, generation: GenerationId) -> Result<u64, StoreError> {
        let history = self.generation_history()?;
        let Some(index) = history
            .iter()
            .position(|entry| entry.manifest.generation == generation)
        else {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: history.last().map(|entry| entry.manifest.generation),
            });
        };
        let sequence = index
            .checked_add(1)
            .ok_or_else(|| StoreError::Integrity("generation sequence overflow".to_owned()))?;
        u64::try_from(sequence)
            .map_err(|error| StoreError::Integrity(format!("invalid generation sequence: {error}")))
    }

    /// Return the atomic change event for one accepted transition, including
    /// transitions that did not alter a canonical fact.
    ///
    /// Durable backends should read by the event's generation key; reference
    /// backends may derive the event from retained history.
    ///
    /// # Errors
    /// Returns a store error if the generation is unknown or its event payload
    /// is malformed.
    fn change_event(&self, generation: GenerationId) -> Result<Option<ChangeEvent>, StoreError> {
        let history = self.generation_history()?;
        if !history
            .iter()
            .any(|entry| entry.manifest.generation == generation)
        {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: history.last().map(|entry| entry.manifest.generation),
            });
        }
        Ok(crate::lineage::change_events_from_history(&history)?
            .into_iter()
            .find(|event| event.generation_after == generation))
    }

    /// Return accepted events that directly changed `fact`, in generation
    /// order. This reference default scans retained transitions; durable
    /// backends should use the `(fact kind, identity, generation)` index.
    ///
    /// # Errors
    /// Returns a store error when history cannot be read or a retained
    /// transition is inconsistent with its parent graph.
    fn change_events_for_fact(
        &self,
        fact: FactRef,
        after: Option<ChangeEventCursor>,
        limit: usize,
    ) -> Result<ChangeEventPage, StoreError> {
        if limit == 0 {
            return Ok(ChangeEventPage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        let history = self.generation_history()?;
        let mut events = crate::lineage::change_events_from_history(&history)?
            .into_iter()
            .filter(|event| {
                event
                    .changed_facts
                    .iter()
                    .any(|changed| changed.fact == fact)
            })
            .collect::<Vec<_>>();
        if let Some(cursor) = after {
            let start = events
                .iter()
                .position(|event| event.generation_after == cursor.generation)
                .map(|index| index.saturating_add(1))
                .ok_or_else(|| StoreError::StaleBase {
                    expected: Some(cursor.generation),
                    actual: history.last().map(|entry| entry.manifest.generation),
                })?;
            events = events.into_iter().skip(start).collect();
        }
        let next_cursor = (events.len() > limit)
            .then(|| events.get(limit.saturating_sub(1)))
            .flatten()
            .map(|event| ChangeEventCursor {
                generation: event.generation_after,
            });
        events.truncate(limit);
        Ok(ChangeEventPage {
            items: events,
            next_cursor,
        })
    }

    /// Return the durable acceptance time for one generation, or `None` when
    /// the generation predates acceptance-time recording.
    ///
    /// # Errors
    /// Returns a stale-generation error or an integrity error for malformed metadata.
    fn acceptance_time(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError>;

    /// Return the maximum known acceptance time across the retained ancestry
    /// ending at `generation`. `None` means at least one transition has
    /// unknown acceptance time; the generation itself must still be retained.
    /// Durable stores should override this with a per-generation projection.
    ///
    /// # Errors
    /// Returns a stale-generation error or an integrity error when acceptance
    /// metadata is malformed.
    fn accepted_through(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError> {
        let history = self.generation_history()?;
        let mut maximum = 0_u64;
        let mut known = true;
        for entry in history {
            match self.acceptance_time(entry.manifest.generation)? {
                Some(time) if known => maximum = maximum.max(time.0),
                Some(_) => {}
                None => known = false,
            }
            if entry.manifest.generation == generation {
                return Ok(known.then_some(AcceptanceTime(maximum)));
            }
        }
        Err(StoreError::StaleBase {
            expected: Some(generation),
            actual: self
                .generation_history()?
                .last()
                .map(|entry| entry.manifest.generation),
        })
    }

    /// Return known generation acceptances in `[from_inclusive, until_exclusive)`.
    /// Results are ordered by acceptance time, then generation ID, and capped
    /// at `limit`. Legacy generations with unknown time are omitted.
    ///
    /// # Errors
    /// Returns [`StoreError::InvalidTemporalRange`] when the range is empty or reversed.
    fn accepted_generations_between(
        &self,
        from_inclusive: AcceptanceTime,
        until_exclusive: AcceptanceTime,
        after: Option<AcceptedGenerationCursor>,
        limit: usize,
    ) -> Result<AcceptedGenerationPage, StoreError> {
        if from_inclusive >= until_exclusive {
            return Err(StoreError::InvalidTemporalRange);
        }
        if limit == 0 {
            return Ok(AcceptedGenerationPage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        let mut accepted = Vec::new();
        for entry in self.generation_history()? {
            let Some(accepted_at) = self.acceptance_time(entry.manifest.generation)? else {
                continue;
            };
            let cursor_key = (accepted_at, entry.manifest.generation);
            let after_cursor =
                after.is_none_or(|cursor| cursor_key > (cursor.accepted_at, cursor.generation));
            if accepted_at >= from_inclusive && accepted_at < until_exclusive && after_cursor {
                accepted.push(AcceptedGeneration {
                    accepted_at,
                    manifest: entry.manifest,
                });
            }
        }
        accepted.sort_by_key(|item| (item.accepted_at, item.manifest.generation));
        let next_cursor = if accepted.len() > limit {
            limit
                .checked_sub(1)
                .and_then(|last_index| accepted.get(last_index))
                .map(|item| AcceptedGenerationCursor {
                    accepted_at: item.accepted_at,
                    generation: item.manifest.generation,
                })
        } else {
            None
        };
        accepted.truncate(limit);
        Ok(AcceptedGenerationPage {
            items: accepted,
            next_cursor,
        })
    }

    /// Reconstruct the logical graph at one accepted generation by replaying
    /// its retained delta chain from genesis or a migration anchor. This
    /// default reference path is O(delta history traversed + output size);
    /// production backends should override it with temporal-version indexes.
    ///
    /// # Errors
    /// Returns a store error if the generation is not retained or history
    /// cannot be replayed consistently.
    fn historical_snapshot(&self, generation: GenerationId) -> Result<GraphSnapshot, StoreError> {
        let history = self.generation_history()?;
        let target = history
            .iter()
            .position(|entry| entry.manifest.generation == generation)
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(generation),
                actual: history.last().map(|entry| entry.manifest.generation),
            })?;
        let history_until_target = history
            .get(..=target)
            .ok_or_else(|| StoreError::Integrity("history target range is invalid".to_owned()))?;
        let anchor_index = history_until_target
            .iter()
            .rposition(|entry| entry.anchor.is_some());
        let mut replay = InMemoryGraphStore::new();
        let start = if let Some(index) = anchor_index {
            let anchor = history.get(index).ok_or_else(|| {
                StoreError::Integrity("history anchor index is invalid".to_owned())
            })?;
            replay.restore_history_anchor(anchor)?;
            index.saturating_add(1)
        } else {
            0
        };
        if start <= target {
            for entry in history.iter().take(target.saturating_add(1)).skip(start) {
                let delta = entry.delta.as_ref().ok_or_else(|| {
                    StoreError::Integrity(
                        "history entry has neither a replay delta nor an anchor".to_owned(),
                    )
                })?;
                let replayed =
                    replay.apply_delta_for_schema(delta, entry.manifest.schema_version)?;
                if replayed.repository != entry.manifest.repository
                    || replayed.worktree != entry.manifest.worktree
                    || replayed.generation != entry.manifest.generation
                    || replayed.parent != entry.manifest.parent
                    || replayed.graph_root != entry.manifest.graph_root
                {
                    return Err(StoreError::Integrity(
                        "historical delta replay does not match its manifest".to_owned(),
                    ));
                }
            }
        }
        replay.snapshot_for_generation(generation)
    }

    /// Read one node version at a retained generation. The default reference
    /// implementation reconstructs a snapshot; durable stores should use the
    /// `(kind, identity, generation interval)` index directly.
    ///
    /// # Errors
    /// Returns a store error if the generation is unavailable or invalid.
    fn historical_node(
        &self,
        generation: GenerationId,
        id: NodeId,
    ) -> Result<Option<Node>, StoreError> {
        Ok(self
            .historical_snapshot(generation)?
            .nodes
            .into_iter()
            .find(|node| node.id == id))
    }

    /// Read at most 256 requested IDs from one retained generation.
    /// Duplicates are collapsed; missing nodes are omitted; output is ID ordered.
    ///
    /// # Errors
    /// Rejects oversized requests and unavailable or corrupt generations.
    fn historical_nodes_by_ids(
        &self,
        generation: GenerationId,
        ids: &[NodeId],
    ) -> Result<Vec<Node>, StoreError> {
        if ids.len() > 256 {
            return Err(StoreError::InvalidPageLimit);
        }
        self.manifest(generation)?;
        let mut nodes = Vec::new();
        let unique_ids = ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        for id in &unique_ids {
            if let Some(node) = self.historical_node(generation, *id)? {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    /// Seek node versions in stable ID order within an exact retained generation.
    /// `after` is exclusive. Durable backends override this reference snapshot
    /// implementation with a bounded persistent-tree walk and one lookahead.
    ///
    /// # Errors
    /// Rejects zero/oversized limits and unavailable or corrupt generations.
    fn historical_nodes_page(
        &self,
        generation: GenerationId,
        after: Option<NodeId>,
        limit: usize,
    ) -> Result<HistoricalNodePage, StoreError> {
        if limit == 0 || limit > MAX_HISTORICAL_NODE_PAGE_SIZE {
            return Err(StoreError::InvalidPageLimit);
        }
        let mut nodes = self.historical_snapshot(generation)?.nodes;
        nodes.sort_by_key(|node| node.id);
        let mut items = nodes
            .into_iter()
            .filter(|node| after.is_none_or(|id| node.id > id))
            .take(limit.saturating_add(1))
            .collect::<Vec<_>>();
        let has_more = items.len() > limit;
        items.truncate(limit);
        Ok(HistoricalNodePage { items, has_more })
    }

    /// Visit all nodes in strictly increasing stable-ID order for one generation.
    /// The visitor is synchronous and must not mutate or reenter this store.
    /// Partial callbacks may precede an error; publish results only on success.
    /// The reference implementation uses existing bounded pages. Adapters may
    /// override with one validated, generation-pinned read operation.
    ///
    /// # Errors
    /// Rejects zero/exhausted total budgets, invalid pages, unavailable/corrupt
    /// generations, and propagates the first visitor error without continuing.
    fn visit_historical_nodes(
        &self,
        generation: GenerationId,
        max_nodes: usize,
        visitor: &mut dyn FnMut(Node) -> Result<(), StoreError>,
    ) -> Result<usize, StoreError> {
        crate::historical_node_scan::visit(self, generation, max_nodes, visitor)
    }

    /// Seek file versions in stable ID order in an exact retained generation.
    /// `after` is exclusive; durable adapters should override snapshot fallback.
    ///
    /// # Errors
    /// Rejects zero/oversized limits and unavailable or corrupt generations.
    fn historical_files_page(
        &self,
        generation: GenerationId,
        after: Option<FileId>,
        limit: usize,
    ) -> Result<HistoricalFilePage, StoreError> {
        if limit == 0 || limit > MAX_HISTORICAL_FILE_PAGE_SIZE {
            return Err(StoreError::InvalidPageLimit);
        }
        let mut files = self.historical_snapshot(generation)?.files;
        files.sort_by_key(|file| file.file_id);
        let mut items = files
            .into_iter()
            .filter(|file| after.is_none_or(|id| file.file_id > id))
            .take(limit.saturating_add(1))
            .collect::<Vec<_>>();
        let has_more = items.len() > limit;
        items.truncate(limit);
        Ok(HistoricalFilePage { items, has_more })
    }

    /// Read one file version at a retained generation. Durable stores should
    /// use the identity/validity index; this default is for reference backends.
    ///
    /// # Errors
    /// Returns a store error if the generation is unavailable or invalid.
    fn historical_file(
        &self,
        generation: GenerationId,
        id: FileId,
    ) -> Result<Option<FileVersion>, StoreError> {
        Ok(self
            .historical_snapshot(generation)?
            .files
            .into_iter()
            .find(|file| file.file_id == id))
    }

    /// Read one provenance record at a retained generation. Durable stores
    /// should use the identity/validity index rather than reconstructing history.
    ///
    /// # Errors
    /// Returns a store error if the generation is unavailable or invalid.
    fn historical_provenance(
        &self,
        generation: GenerationId,
        id: ProvenanceId,
    ) -> Result<Option<Provenance>, StoreError> {
        Ok(self
            .historical_snapshot(generation)?
            .provenance
            .into_iter()
            .find(|item| item.id == id))
    }

    /// Find stable-ID-ordered node versions by case-insensitive substring at a
    /// retained generation. Durable stores should scan its node-family root
    /// range rather than reconstructing a snapshot or scanning all versions.
    ///
    /// # Errors
    /// Returns a store error if the generation is unavailable or invalid.
    fn historical_search_nodes(
        &self,
        generation: GenerationId,
        text: &str,
        limit: usize,
    ) -> Result<Vec<Node>, StoreError> {
        let text = text.to_lowercase();
        let mut nodes = self.historical_snapshot(generation)?.nodes;
        nodes.sort_by_key(|node| node.id);
        Ok(nodes
            .into_iter()
            .filter(|node| node.name.to_lowercase().contains(&text))
            .take(limit)
            .collect())
    }

    /// Read an ordered page of historical edges in one explicit direction.
    /// Durable stores should seek their generation-scoped incidence index;
    /// reference stores may scan the reconstructed generation snapshot.
    ///
    /// # Errors
    /// Returns a store error if the generation is unavailable, the page limit
    /// overflows, or retained graph history is malformed.
    fn historical_incident_edges(
        &self,
        generation: GenerationId,
        endpoint: NodeId,
        direction: EdgeDirection,
        after: Option<EdgeId>,
        limit: usize,
    ) -> Result<HistoricalEdgePage, StoreError> {
        if limit == 0 || limit > MAX_HISTORICAL_EDGE_PAGE_SIZE {
            return Err(StoreError::InvalidPageLimit);
        }
        let snapshot = self.historical_snapshot(generation)?;
        let mut edges = snapshot
            .edges
            .into_iter()
            .filter(|edge| match direction {
                EdgeDirection::Outgoing => edge.source == endpoint,
                EdgeDirection::Incoming => edge.target == endpoint,
            })
            .filter(|edge| after.is_none_or(|cursor| edge.id > cursor))
            .collect::<Vec<_>>();
        edges.sort_by_key(|edge| edge.id);
        let has_more = edges.len() > limit;
        edges.truncate(limit);
        Ok(HistoricalEdgePage {
            items: edges,
            has_more,
            #[cfg(feature = "benchmark-instrumentation")]
            read_metrics: HistoricalEdgeReadMetrics {
                reference_snapshots_materialized: 1,
                ..HistoricalEdgeReadMetrics::default()
            },
        })
    }

    /// Return every retained payload version of one node.
    ///
    /// Intervals are half-open: a version is active at `valid_from` and
    /// inactive at `valid_until` when present.
    ///
    /// # Errors
    /// Returns a store error if retained history is unavailable or malformed.
    fn node_history(&self, id: NodeId) -> Result<Vec<NodeHistoryVersion>, StoreError> {
        let mut versions = Vec::new();
        for entry in self.generation_history()? {
            if let Some(anchor) = &entry.anchor
                && let Some(node) = anchor.nodes.iter().find(|node| node.id == id)
            {
                versions.push(NodeHistoryVersion {
                    valid_from: entry.manifest.generation,
                    valid_until: None,
                    node: node.clone(),
                });
            }
            let Some(delta) = &entry.delta else {
                continue;
            };
            if delta.remove_nodes.contains(&id) {
                close_node_version(&mut versions, entry.manifest.generation);
            }
            if let Some(node) = delta.upsert_nodes.iter().find(|node| node.id == id) {
                close_node_version(&mut versions, entry.manifest.generation);
                versions.push(NodeHistoryVersion {
                    valid_from: entry.manifest.generation,
                    valid_until: None,
                    node: node.clone(),
                });
            }
        }
        Ok(versions)
    }

    /// Return every retained version of a canonical file, provenance, node, or edge fact.
    ///
    /// Durable stores should answer from their fact-identity interval index;
    /// this default reference implementation scans accepted deltas.
    ///
    /// # Errors
    /// Returns a store error if retained history is unavailable or malformed.
    fn fact_history(&self, fact: FactRef) -> Result<Vec<FactHistoryVersion>, StoreError> {
        let mut versions = Vec::new();
        for entry in self.generation_history()? {
            if let Some(anchor) = &entry.anchor
                && let Some(payload) = fact_in_snapshot(anchor, fact)
            {
                close_fact_version(&mut versions, entry.manifest.generation);
                let observed_at = fact_observation_time(&payload);
                let accepted_at = self.acceptance_time(entry.manifest.generation)?;
                versions.push(FactHistoryVersion {
                    valid_from: entry.manifest.generation,
                    valid_until: None,
                    observed_at,
                    accepted_at,
                    payload,
                });
            }
            let active_payload = versions
                .last()
                .filter(|version| version.valid_until.is_none())
                .map(|version| &version.payload);
            if let Some(delta) = &entry.delta
                && let Some(change) = fact_in_delta(delta, fact, active_payload)
            {
                close_fact_version(&mut versions, entry.manifest.generation);
                if let Some(payload) = change {
                    let observed_at = fact_observation_time(&payload);
                    let accepted_at = self.acceptance_time(entry.manifest.generation)?;
                    versions.push(FactHistoryVersion {
                        valid_from: entry.manifest.generation,
                        valid_until: None,
                        observed_at,
                        accepted_at,
                        payload,
                    });
                }
            }
        }
        Ok(versions)
    }

    /// Return only the exact versions of `fact` opened or closed at one
    /// accepted generation. An update normally returns the old and new
    /// versions; a deletion returns the closed version; an unchanged fact
    /// returns no entries.
    ///
    /// Durable adapters should seek the fact identity/start and
    /// identity/end indexes. The default reference implementation scans the
    /// retained fact history for this one identity.
    ///
    /// # Errors
    /// Returns an error if the generation or fact history is unavailable or
    /// malformed.
    fn fact_versions_changed_at(
        &self,
        fact: FactRef,
        generation: GenerationId,
    ) -> Result<Vec<FactHistoryEntry>, StoreError> {
        let history = self.generation_history()?;
        let sequences = history
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let sequence = index
                    .checked_add(1)
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or_else(|| {
                        StoreError::Integrity("generation sequence overflow".to_owned())
                    })?;
                Ok((entry.manifest.generation, sequence))
            })
            .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
        let changed_sequence =
            sequences
                .get(&generation)
                .copied()
                .ok_or_else(|| StoreError::StaleBase {
                    expected: Some(generation),
                    actual: history.last().map(|entry| entry.manifest.generation),
                })?;
        let mut entries = Vec::new();
        for version in self.fact_history(fact)? {
            let valid_from_sequence =
                sequences.get(&version.valid_from).copied().ok_or_else(|| {
                    StoreError::Integrity("fact version starts at a missing generation".to_owned())
                })?;
            let valid_until_sequence = version
                .valid_until
                .map(|until| {
                    sequences.get(&until).copied().ok_or_else(|| {
                        StoreError::Integrity(
                            "fact version ends at a missing generation".to_owned(),
                        )
                    })
                })
                .transpose()?;
            if valid_from_sequence == changed_sequence
                || valid_until_sequence == Some(changed_sequence)
            {
                entries.push(FactHistoryEntry {
                    fact,
                    valid_from_sequence,
                    valid_until_sequence,
                    version,
                });
            }
        }
        Ok(entries)
    }

    /// Return a bounded page of every fact version opened or closed at one
    /// accepted generation. Durable adapters should seek generation-leading
    /// temporal indexes; the reference implementation filters its typed
    /// historical fact pages.
    ///
    /// # Errors
    /// Returns an error when the generation/cursor is not retained or bound to
    /// this request, stored history is malformed, or `limit` exceeds
    /// [`MAX_FACT_VERSION_CHANGE_PAGE_SIZE`].
    fn fact_version_changes_at_page(
        &self,
        generation: GenerationId,
        after: Option<FactVersionChangeCursor>,
        limit: usize,
    ) -> Result<FactVersionChangePage, StoreError> {
        if limit > MAX_FACT_VERSION_CHANGE_PAGE_SIZE {
            return Err(StoreError::InvalidPageLimit);
        }
        if limit == 0 {
            return Ok(FactVersionChangePage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        if after.is_some_and(|cursor| cursor.generation != generation) {
            return Err(StoreError::InvalidDelta(
                "fact-version-change cursor is bound to a different generation".to_owned(),
            ));
        }

        let history = self.generation_history()?;
        let sequence_by_generation = history
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let sequence = index
                    .checked_add(1)
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or_else(|| {
                        StoreError::Integrity("generation sequence overflow".to_owned())
                    })?;
                Ok((entry.manifest.generation, sequence))
            })
            .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
        let generation_sequence = sequence_by_generation
            .get(&generation)
            .copied()
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(generation),
                actual: history.last().map(|entry| entry.manifest.generation),
            })?;
        let after_history_cursor = after
            .map(|cursor| {
                if cursor.after_valid_from_sequence == 0 {
                    return Err(StoreError::InvalidDelta(
                        "fact-version-change cursor has a zero start sequence".to_owned(),
                    ));
                }
                let valid_from = sequence_by_generation
                    .iter()
                    .find_map(|(candidate, sequence)| {
                        (*sequence == cursor.after_valid_from_sequence).then_some(*candidate)
                    })
                    .ok_or_else(|| {
                        StoreError::InvalidDelta(
                            "fact-version-change cursor has an unknown start sequence".to_owned(),
                        )
                    })?;
                if cursor.after_valid_from_sequence > generation_sequence {
                    return Err(StoreError::InvalidDelta(
                        "fact-version-change cursor is after its generation".to_owned(),
                    ));
                }
                Ok(FactHistoryCursor {
                    as_of_generation: generation,
                    after_fact: cursor.after_fact,
                    after_valid_from: valid_from,
                })
            })
            .transpose()?;

        let mut history_cursor = after_history_cursor;
        let mut items = Vec::with_capacity(limit.saturating_add(1));
        loop {
            let page = self.fact_history_page(
                generation,
                history_cursor,
                MAX_FACT_VERSION_CHANGE_PAGE_SIZE,
            )?;
            for entry in page.items {
                if entry.valid_from_sequence == generation_sequence
                    || entry.valid_until_sequence == Some(generation_sequence)
                {
                    items.push(entry);
                    if items.len() > limit {
                        break;
                    }
                }
            }
            if items.len() > limit || page.next_cursor.is_none() {
                break;
            }
            history_cursor = page.next_cursor;
        }

        let next_cursor = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|entry| FactVersionChangeCursor {
                generation,
                after_fact: entry.fact,
                after_valid_from_sequence: entry.valid_from_sequence,
            })
        } else {
            None
        };
        Ok(FactVersionChangePage { items, next_cursor })
    }

    /// Return known producer-observation versions in `[from, until)` order.
    ///
    /// This default reference implementation enumerates retained fact identities
    /// and histories; durable stores should override it with the observation-time
    /// index. The cursor order is `(observed_at, fact reference, valid_from)`.
    ///
    /// # Errors
    /// Returns [`StoreError::InvalidTemporalRange`] for an empty/reversed range.
    fn observed_facts_between(
        &self,
        from_inclusive: ObservationTime,
        until_exclusive: ObservationTime,
        after: Option<ObservedFactCursor>,
        limit: usize,
    ) -> Result<ObservedFactPage, StoreError> {
        if from_inclusive >= until_exclusive {
            return Err(StoreError::InvalidTemporalRange);
        }
        if limit == 0 {
            return Ok(ObservedFactPage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        let history = self.generation_history()?;
        let generation_order = history
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.manifest.generation, index))
            .collect::<BTreeMap<_, _>>();
        let after_order = after
            .map(|cursor| {
                generation_order
                    .get(&cursor.valid_from)
                    .copied()
                    .map(|index| (cursor.observed_at, cursor.fact, index))
                    .ok_or_else(|| StoreError::StaleBase {
                        expected: Some(cursor.valid_from),
                        actual: history.last().map(|entry| entry.manifest.generation),
                    })
            })
            .transpose()?;
        let mut facts = BTreeSet::new();
        for entry in &history {
            if let Some(snapshot) = &entry.anchor {
                facts.extend(
                    snapshot
                        .files
                        .iter()
                        .map(|item| FactRef::File(item.file_id)),
                );
                facts.extend(
                    snapshot
                        .provenance
                        .iter()
                        .map(|item| FactRef::Provenance(item.id)),
                );
                facts.extend(snapshot.nodes.iter().map(|item| FactRef::Node(item.id)));
                facts.extend(snapshot.edges.iter().map(|item| FactRef::Edge(item.id)));
            }
            if let Some(delta) = &entry.delta {
                facts.extend(
                    delta
                        .changed_files
                        .iter()
                        .map(|item| FactRef::File(item.file_id)),
                );
                facts.extend(
                    delta
                        .upsert_provenance
                        .iter()
                        .map(|item| FactRef::Provenance(item.id)),
                );
                facts.extend(delta.upsert_nodes.iter().map(|item| FactRef::Node(item.id)));
                facts.extend(delta.upsert_edges.iter().map(|item| FactRef::Edge(item.id)));
            }
        }
        let mut ordered_items = Vec::new();
        for fact in facts {
            for version in self.fact_history(fact)? {
                let Some(observed_at) = version.observed_at else {
                    continue;
                };
                let Some(&sequence) = generation_order.get(&version.valid_from) else {
                    return Err(StoreError::Integrity(
                        "fact version starts at an unretained generation".to_owned(),
                    ));
                };
                let after_cursor =
                    after_order.is_none_or(|cursor| (observed_at, fact, sequence) > cursor);
                if observed_at >= from_inclusive && observed_at < until_exclusive && after_cursor {
                    ordered_items.push((sequence, ObservedFactVersion { fact, version }));
                }
            }
        }
        ordered_items
            .sort_by_key(|(sequence, item)| (item.version.observed_at, item.fact, *sequence));
        let next_cursor = if ordered_items.len() > limit {
            limit
                .checked_sub(1)
                .and_then(|index| ordered_items.get(index))
                .and_then(|(_, item)| {
                    item.version
                        .observed_at
                        .map(|observed_at| ObservedFactCursor {
                            observed_at,
                            fact: item.fact,
                            valid_from: item.version.valid_from,
                        })
                })
        } else {
            None
        };
        let mut items = ordered_items
            .into_iter()
            .map(|(_, item)| item)
            .collect::<Vec<_>>();
        items.truncate(limit);
        Ok(ObservedFactPage { items, next_cursor })
    }

    /// Return accepted graph deltas from `from` (exclusive) through `to`
    /// (inclusive), requiring both generations to be retained on one chain.
    ///
    /// # Errors
    /// Returns a stale-base error for missing generations and an integrity
    /// error when the requested interval crosses a broken chain or anchor.
    fn changes_between(
        &self,
        from: GenerationId,
        to: GenerationId,
    ) -> Result<Vec<GenerationChange>, StoreError> {
        let history = self.generation_history()?;
        let from_index = history
            .iter()
            .position(|entry| entry.manifest.generation == from)
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(from),
                actual: history.last().map(|entry| entry.manifest.generation),
            })?;
        let to_index = history
            .iter()
            .position(|entry| entry.manifest.generation == to)
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(to),
                actual: history.last().map(|entry| entry.manifest.generation),
            })?;
        if from_index >= to_index {
            return Err(StoreError::InvalidDelta(
                "change range must move forward between retained generations".to_owned(),
            ));
        }
        let mut parent = from;
        let mut changes = Vec::new();
        let transitions = history
            .get(from_index.saturating_add(1)..to_index.saturating_add(1))
            .ok_or_else(|| StoreError::Integrity("change range indices are invalid".to_owned()))?;
        for (transition_offset, entry) in transitions.iter().enumerate() {
            if entry.manifest.parent != Some(parent) {
                return Err(StoreError::Integrity(
                    "change range is not a contiguous generation chain".to_owned(),
                ));
            }
            let delta = entry.delta.clone().ok_or_else(|| {
                StoreError::Integrity("change range crosses a non-transition anchor".to_owned())
            })?;
            let sequence = from_index
                .checked_add(transition_offset)
                .and_then(|index| index.checked_add(2))
                .ok_or_else(|| StoreError::Integrity("generation sequence overflow".to_owned()))?;
            changes.push(GenerationChange {
                sequence: u64::try_from(sequence).map_err(|error| {
                    StoreError::Integrity(format!("invalid generation sequence: {error}"))
                })?,
                manifest: entry.manifest.clone(),
                delta,
            });
            parent = entry.manifest.generation;
        }
        Ok(changes)
    }

    /// Return a bounded page of accepted changes in one pinned generation
    /// range. Durable adapters should seek the sequence primary key directly;
    /// the reference implementation filters `changes_between`.
    ///
    /// # Errors
    /// Returns a stale-base or integrity error for unavailable/broken history,
    /// an invalid-delta error for a mismatched cursor, or an invalid-page-limit
    /// error when `limit` exceeds [`MAX_GENERATION_CHANGE_PAGE_SIZE`].
    fn changes_between_page(
        &self,
        from: GenerationId,
        to: GenerationId,
        after: Option<GenerationChangeCursor>,
        limit: usize,
    ) -> Result<GenerationChangePage, StoreError> {
        if limit == 0 {
            return Ok(GenerationChangePage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        if limit > MAX_GENERATION_CHANGE_PAGE_SIZE {
            return Err(StoreError::InvalidPageLimit);
        }
        let from_sequence = self.generation_sequence(from)?;
        let to_sequence = self.generation_sequence(to)?;
        if from_sequence >= to_sequence {
            return Err(StoreError::InvalidDelta(
                "change range must move forward between retained generations".to_owned(),
            ));
        }
        let after_sequence = match after {
            Some(cursor) if cursor.from_generation == from && cursor.to_generation == to => {
                if cursor.after_sequence < from_sequence || cursor.after_sequence > to_sequence {
                    return Err(StoreError::InvalidDelta(
                        "generation-change cursor is outside its pinned range".to_owned(),
                    ));
                }
                if self.generation_sequence(cursor.after_generation)? != cursor.after_sequence {
                    return Err(StoreError::InvalidDelta(
                        "generation-change cursor sequence does not match its generation"
                            .to_owned(),
                    ));
                }
                cursor.after_sequence
            }
            Some(_) => {
                return Err(StoreError::InvalidDelta(
                    "generation-change cursor is bound to a different range".to_owned(),
                ));
            }
            None => from_sequence,
        };
        let mut items = self
            .changes_between(from, to)?
            .into_iter()
            .filter(|change| change.sequence > after_sequence)
            .take(limit.saturating_add(1))
            .collect::<Vec<_>>();
        let has_more = items.len() > limit;
        if has_more {
            items.truncate(limit);
        }
        let next_cursor = if has_more {
            items.last().map(|change| GenerationChangeCursor {
                from_generation: from,
                to_generation: to,
                after_sequence: change.sequence,
                after_generation: change.manifest.generation,
            })
        } else {
            None
        };
        Ok(GenerationChangePage { items, next_cursor })
    }

    /// Commit all delta effects and the new manifest together, or none.
    ///
    /// # Errors
    /// Returns a stale-base, invalid-delta, integrity, or backend error.
    fn apply_delta(&mut self, delta: GraphDelta) -> Result<GenerationManifest, StoreError>;

    /// Atomically commit a graph delta and its engine-assigned acceptance time.
    ///
    /// # Errors
    /// Returns a stale-base, invalid-delta, integrity, or backend error.
    fn apply_delta_at(
        &mut self,
        delta: GraphDelta,
        accepted_at: AcceptanceTime,
    ) -> Result<GenerationManifest, StoreError> {
        self.apply_delta_with_acceptance_time(delta, Some(accepted_at))
    }

    /// Commit a generation while preserving unknown acceptance time for legacy
    /// workflow recovery. New full-engine publications always provide a value.
    ///
    /// # Errors
    /// Returns a stale-base, invalid-delta, integrity, or backend error.
    fn apply_delta_with_acceptance_time(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError>;

    /// Update verification status without changing canonical graph facts.
    ///
    /// # Errors
    /// Returns a stale-base or backend error when the generation is unavailable.
    fn set_generation_status(
        &mut self,
        generation: GenerationId,
        status: GenerationStatus,
    ) -> Result<GenerationManifest, StoreError>;

    /// Read a node from one consistent generation.
    ///
    /// # Errors
    /// Returns a store error if the generation or node cannot be read reliably.
    fn node(&self, generation: GenerationId, id: NodeId) -> Result<Option<Node>, StoreError>;

    /// Read all nodes from one consistent generation in stable ID order.
    ///
    /// # Errors
    /// Returns a store error if the generation is not current.
    fn nodes(&self, generation: GenerationId) -> Result<Vec<Node>, StoreError>;

    /// Read module declarations, source import/export occurrences, and
    /// resolution diagnostics. Durable backends should avoid materializing
    /// unrelated graph nodes.
    ///
    /// # Errors
    /// Returns a store error if the generation or persisted node payloads are invalid.
    fn module_resolution_nodes(&self, generation: GenerationId) -> Result<Vec<Node>, StoreError> {
        Ok(self
            .nodes(generation)?
            .into_iter()
            .filter(|node| {
                matches!(
                    &node.kind,
                    syntaxmesh_core::NodeKind::Module
                        | syntaxmesh_core::NodeKind::Import { .. }
                        | syntaxmesh_core::NodeKind::Export { .. }
                        | syntaxmesh_core::NodeKind::ModuleResolutionDiagnostic { .. }
                )
            })
            .collect())
    }

    /// Search canonical node names case-insensitively, in stable ID order.
    /// Backends with a suitable index should override this default scan.
    ///
    /// # Errors
    /// Returns a store error if the generation is not current.
    fn search_nodes(
        &self,
        generation: GenerationId,
        text: &str,
        limit: usize,
    ) -> Result<Vec<Node>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let needle = text.to_ascii_lowercase();
        Ok(self
            .nodes(generation)?
            .into_iter()
            .filter(|node| node.name.to_ascii_lowercase().contains(&needle))
            .take(limit)
            .collect())
    }

    /// Return exact-name and terminal-name candidates for resolver lookups.
    /// This may return definitions and reference occurrences; callers must
    /// check node kinds and normalized symbol equality themselves. The default
    /// compatibility implementation scans all nodes; scalable backends should
    /// override it with indexed exact and terminal-name lookups.
    ///
    /// # Errors
    /// Returns a store error if the generation is not current.
    fn symbol_candidates(
        &self,
        generation: GenerationId,
        exact_names: &[String],
        terminal_names: &[String],
    ) -> Result<Vec<Node>, StoreError> {
        let exact = exact_names
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let terminals = terminal_names
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        Ok(self
            .nodes(generation)?
            .into_iter()
            .filter(|node| {
                exact.contains(node.name.as_str())
                    || node
                        .name
                        .rsplit("::")
                        .next()
                        .is_some_and(|terminal| terminals.contains(terminal))
            })
            .collect())
    }

    /// Read all known file versions from one consistent generation.
    ///
    /// # Errors
    /// Returns a store error if the generation is not current.
    fn files(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::FileVersion>, StoreError>;

    /// Read all edges from one consistent generation in stable ID order.
    ///
    /// # Errors
    /// Returns a store error if the generation is not current.
    fn edges(&self, generation: GenerationId) -> Result<Vec<Edge>, StoreError>;

    /// Read all provenance records from one consistent generation in stable ID order.
    ///
    /// # Errors
    /// Returns a store error if the generation is not current.
    fn provenance(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::Provenance>, StoreError>;

    /// Read only the requested producer records from one accepted generation.
    /// The compatibility implementation filters the reference provenance
    /// result; indexed durable adapters should override it by stable ID.
    ///
    /// # Errors
    /// Returns a store error if the generation or provenance payloads cannot
    /// be read reliably.
    fn provenance_for_ids(
        &self,
        generation: GenerationId,
        ids: &[ProvenanceId],
    ) -> Result<Vec<Provenance>, StoreError> {
        self.manifest(generation)?;
        let requested = ids.iter().copied().collect::<BTreeSet<_>>();
        Ok(self
            .provenance(generation)?
            .into_iter()
            .filter(|provenance| requested.contains(&provenance.id))
            .collect())
    }

    /// Read outgoing edges from one consistent generation.
    ///
    /// # Errors
    /// Returns a store error if the generation or edges cannot be read reliably.
    fn outgoing(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError>;

    /// Read incoming edges from one consistent generation.
    ///
    /// The default preserves compatibility for backends that only implement
    /// [`Self::edges`]. Backends with adjacency indexes should override it.
    ///
    /// # Errors
    /// Returns a store error if the generation or edges cannot be read reliably.
    fn incoming(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        Ok(self
            .edges(generation)?
            .into_iter()
            .filter(|edge| edge.target == id)
            .collect())
    }

    /// List nodes owned by one file in a consistent generation.
    ///
    /// # Errors
    /// Returns a store error if the requested generation is not current.
    fn nodes_for_file(
        &self,
        generation: GenerationId,
        file: syntaxmesh_core::FileId,
    ) -> Result<Vec<NodeId>, StoreError>;
}

/// Opaque durable records used by higher-level operations without coupling a
/// storage backend to a workflow implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableRecordPage {
    /// Ordered records in this page.
    pub records: Vec<(String, Vec<u8>)>,
    /// Exclusive key cursor for the next page.
    pub next_cursor: Option<String>,
}

pub trait DurableRecordStore {
    /// Reads the exact record identified by a namespaced key.
    ///
    /// # Errors
    /// Returns a store error if the record cannot be read reliably.
    fn read_record(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError>;

    /// Lists records with a namespaced key prefix in deterministic order.
    ///
    /// # Errors
    /// Returns a store error if records cannot be read reliably.
    fn records_with_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError>;

    /// Reads an ordered, bounded page under `prefix`, strictly after `after_key`.
    ///
    /// Ordered durable stores should implement this as a key-range query so
    /// callers can inspect large journals without materializing them. The
    /// fallback preserves compatibility for small/reference stores.
    /// A zero limit returns an empty page with no cursor.
    ///
    /// # Errors
    /// Returns a store error if records cannot be read reliably.
    fn records_with_prefix_page(
        &self,
        prefix: &str,
        after_key: Option<&str>,
        limit: usize,
    ) -> Result<DurableRecordPage, StoreError> {
        if limit == 0 {
            return Ok(DurableRecordPage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let records = self
            .records_with_prefix(prefix)?
            .into_iter()
            .filter(|(key, _)| after_key.is_none_or(|after| key.as_str() > after))
            .collect::<Vec<_>>();
        let has_more = records.len() > limit;
        let records = records.into_iter().take(limit).collect::<Vec<_>>();
        let next_cursor = has_more
            .then(|| records.last().map(|(key, _)| key.clone()))
            .flatten();
        Ok(DurableRecordPage {
            records,
            next_cursor,
        })
    }

    /// Reads the lexicographically latest record whose key has this prefix.
    ///
    /// The default preserves compatibility for stores that only implement
    /// [`Self::records_with_prefix`]. Ordered stores should override it with an
    /// indexed reverse range so callers can advance hash-linked journals
    /// without loading their full history.
    ///
    /// # Errors
    /// Returns a store error if the record cannot be read reliably.
    fn last_record_with_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        Ok(self.records_with_prefix(prefix)?.pop())
    }

    /// Replaces a record only if its current bytes equal `expected`.
    ///
    /// Pass `None` to create a record that does not yet exist. Passing the
    /// current payload makes updates compare-and-swap safe across store handles.
    ///
    /// # Errors
    /// Returns [`StoreError::RecordConflict`] when the current payload differs
    /// from `expected`, or another store error when persistence fails.
    fn compare_exchange_record(
        &mut self,
        key: &str,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), StoreError>;
}

fn prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut chars = prefix.chars().collect::<Vec<_>>();
    while let Some(last) = chars.pop() {
        let next = u32::from(last)
            .checked_add(1)
            .map(|codepoint| {
                if codepoint == 0xD800 {
                    0xE000
                } else {
                    codepoint
                }
            })
            .and_then(char::from_u32);
        if let Some(next) = next {
            chars.push(next);
            return Some(chars.into_iter().collect());
        }
    }
    None
}

/// Backend whose durable representation can run an explicit integrity check.
pub trait BackendIntegrityCheck {
    /// Check the backend's durable representation without changing it.
    ///
    /// # Errors
    /// Returns a store error if the integrity report itself cannot be produced.
    fn backend_integrity_check(&self) -> Result<BackendIntegrityReport, StoreError>;
}

/// Representation checked by a backend integrity report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendIntegrityKind {
    /// Restartable serialized snapshot used by the reference backend.
    FileSnapshot,
    /// Physical database checked through SQLite's integrity pragma.
    SqliteDatabase,
    /// Physical database checked through Turso's integrity pragma.
    TursoDatabase,
}

impl BackendIntegrityKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileSnapshot => "file_snapshot",
            Self::SqliteDatabase => "sqlite_database",
            Self::TursoDatabase => "turso_database",
        }
    }
}

/// Outcome of an explicit backend-specific integrity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendIntegrityReport {
    /// Durable representation checked by the adapter.
    pub kind: BackendIntegrityKind,
    /// Whether the backend-specific check passed.
    pub passed: bool,
    /// Successful `ok` marker or detailed errors/check failures.
    pub findings: Vec<String>,
}

impl BackendIntegrityReport {
    /// Build a report from a database `integrity_check` result set.
    #[must_use]
    pub fn from_database_findings(kind: BackendIntegrityKind, findings: Vec<String>) -> Self {
        let passed = findings.len() == 1 && findings.first().is_some_and(|item| item == "ok");
        Self {
            kind,
            passed,
            findings,
        }
    }

    /// Build a report from a serialized-snapshot check.
    #[must_use]
    pub const fn snapshot(passed: bool, findings: Vec<String>) -> Self {
        Self {
            kind: BackendIntegrityKind::FileSnapshot,
            passed,
            findings,
        }
    }
}

/// A deterministic reference backend for tests, embedded hosts, and backend
/// conformance. It keeps one complete published snapshot in memory and applies
/// each delta as a single state transition.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct InMemoryGraphStore {
    repository: Option<RepositoryId>,
    worktree: Option<WorktreeId>,
    manifest: Option<GenerationManifest>,
    history: Vector<GenerationHistoryEntry>,
    files: OrdMap<syntaxmesh_core::FileId, syntaxmesh_core::FileVersion>,
    provenance: OrdMap<ProvenanceId, Provenance>,
    nodes: OrdMap<NodeId, Node>,
    edges: OrdMap<EdgeId, Edge>,
    records: OrdMap<String, Vec<u8>>,
    #[serde(default)]
    lineage_history: Vec<GenerationLineageEntry>,
    #[serde(default)]
    consequence_history: Vec<GenerationConsequenceEntry>,
    #[serde(skip)]
    outgoing_index: OrdMap<NodeId, BTreeSet<EdgeId>>,
    #[serde(skip)]
    incoming_index: OrdMap<NodeId, BTreeSet<EdgeId>>,
    #[serde(skip)]
    nodes_by_file_index: OrdMap<syntaxmesh_core::FileId, BTreeSet<NodeId>>,
    #[serde(skip)]
    nodes_by_name_index: OrdMap<String, BTreeSet<NodeId>>,
    #[serde(skip)]
    nodes_by_terminal_index: OrdMap<String, BTreeSet<NodeId>>,
    #[serde(skip)]
    persistent_fact_root: Option<syntaxmesh_core::StableId>,
    #[serde(skip)]
    persistent_fact_root_ready: bool,
    #[serde(skip)]
    persistent_fact_cache: PersistentFactTreeCache,
}

impl Clone for InMemoryGraphStore {
    fn clone(&self) -> Self {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut profile = StoreCloneProfile::new();

        let repository = self.repository;
        let worktree = self.worktree;
        let manifest = self.manifest.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("identity_and_manifest");

        let history = self.history.clone();
        let lineage_history = self.lineage_history.clone();
        let consequence_history = self.consequence_history.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("generation_history");

        let files = self.files.clone();
        let provenance = self.provenance.clone();
        let nodes = self.nodes.clone();
        let edges = self.edges.clone();
        let records = self.records.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("canonical_facts");

        let outgoing_index = self.outgoing_index.clone();
        let incoming_index = self.incoming_index.clone();
        let nodes_by_file_index = self.nodes_by_file_index.clone();
        let nodes_by_name_index = self.nodes_by_name_index.clone();
        let nodes_by_terminal_index = self.nodes_by_terminal_index.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("query_indexes");

        let persistent_fact_root = self.persistent_fact_root;
        let persistent_fact_root_ready = self.persistent_fact_root_ready;
        let persistent_fact_cache = self.persistent_fact_cache.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("persistent_fact_cache");

        Self {
            repository,
            worktree,
            manifest,
            history,
            files,
            provenance,
            nodes,
            edges,
            records,
            lineage_history,
            consequence_history,
            outgoing_index,
            incoming_index,
            nodes_by_file_index,
            nodes_by_name_index,
            nodes_by_terminal_index,
            persistent_fact_root,
            persistent_fact_root_ready,
            persistent_fact_cache,
        }
    }
}

const FILE_SNAPSHOT_MAGIC: &[u8] = b"SYNTMESH-SNAPSHOT\0";
const FILE_SNAPSHOT_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct FileSnapshotV1 {
    version: u32,
    memory: InMemoryGraphStoreV1,
}

#[derive(Serialize, Deserialize)]
struct InMemoryGraphStoreV1 {
    repository: Option<RepositoryId>,
    worktree: Option<WorktreeId>,
    manifest: Option<GenerationManifest>,
    history: Vec<GenerationHistoryEntry>,
    files: BTreeMap<syntaxmesh_core::FileId, syntaxmesh_core::FileVersion>,
    provenance: BTreeMap<ProvenanceId, Provenance>,
    nodes: BTreeMap<NodeId, Node>,
    edges: BTreeMap<EdgeId, Edge>,
    records: BTreeMap<String, Vec<u8>>,
    lineage_history: Vec<GenerationLineageEntry>,
}

impl InMemoryGraphStoreV1 {
    fn into_current(self) -> InMemoryGraphStore {
        InMemoryGraphStore {
            repository: self.repository,
            worktree: self.worktree,
            manifest: self.manifest,
            history: self.history.into_iter().collect(),
            files: self.files.into_iter().collect(),
            provenance: self.provenance.into_iter().collect(),
            nodes: self.nodes.into_iter().collect(),
            edges: self.edges.into_iter().collect(),
            records: self.records.into_iter().collect(),
            lineage_history: self.lineage_history,
            consequence_history: Vec::new(),
            outgoing_index: OrdMap::new(),
            incoming_index: OrdMap::new(),
            nodes_by_file_index: OrdMap::new(),
            nodes_by_name_index: OrdMap::new(),
            nodes_by_terminal_index: OrdMap::new(),
            persistent_fact_root: None,
            persistent_fact_root_ready: false,
            persistent_fact_cache: PersistentFactTreeCache::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FileSnapshotV2 {
    version: u32,
    memory: InMemoryGraphStore,
}

/// Pre-envelope bincode layout used by existing FileGraphStore snapshots.
#[derive(Serialize, Deserialize)]
struct LegacyFileSnapshot {
    repository: Option<RepositoryId>,
    worktree: Option<WorktreeId>,
    manifest: Option<GenerationManifest>,
    history: Vec<GenerationHistoryEntry>,
    files: BTreeMap<syntaxmesh_core::FileId, syntaxmesh_core::FileVersion>,
    provenance: BTreeMap<ProvenanceId, Provenance>,
    nodes: BTreeMap<NodeId, Node>,
    edges: BTreeMap<EdgeId, Edge>,
    records: BTreeMap<String, Vec<u8>>,
}

impl LegacyFileSnapshot {
    fn into_current(self) -> InMemoryGraphStore {
        InMemoryGraphStore {
            repository: self.repository,
            worktree: self.worktree,
            manifest: self.manifest,
            history: self.history.into_iter().collect(),
            files: self.files.into_iter().collect(),
            provenance: self.provenance.into_iter().collect(),
            nodes: self.nodes.into_iter().collect(),
            edges: self.edges.into_iter().collect(),
            records: self.records.into_iter().collect(),
            lineage_history: Vec::new(),
            consequence_history: Vec::new(),
            outgoing_index: OrdMap::new(),
            incoming_index: OrdMap::new(),
            nodes_by_file_index: OrdMap::new(),
            nodes_by_name_index: OrdMap::new(),
            nodes_by_terminal_index: OrdMap::new(),
            persistent_fact_root: None,
            persistent_fact_root_ready: false,
            persistent_fact_cache: PersistentFactTreeCache::default(),
        }
    }
}

/// A restartable reference backend using atomic versioned-bincode replacement.
/// This is intentionally a conformance backend, not the production Turso
/// schema; both implement the same `GraphStore` port.
#[derive(Debug, Clone)]
pub struct FileGraphStore {
    path: PathBuf,
    memory: InMemoryGraphStore,
}

impl FileGraphStore {
    /// Open an existing snapshot or create an empty store at `path`.
    ///
    /// # Errors
    /// Returns a backend error when the snapshot cannot be read or decoded.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        let mut memory =
            if path.exists() && path.metadata().map_or(true, |metadata| metadata.len() > 0) {
                let bytes = std::fs::read(&path)
                    .map_err(|error| StoreError::Backend(format!("read snapshot: {error}")))?;
                decode_file_snapshot(&bytes)?
            } else {
                InMemoryGraphStore::new()
            };
        memory.rebuild_indexes();
        let lineage_history = memory.lineage_history.clone();
        memory.restore_generation_lineage_history(lineage_history)?;
        let consequence_history = memory.consequence_history.clone();
        memory.restore_generation_consequence_history(consequence_history)?;
        Ok(Self { path, memory })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn latest_generation(&self) -> Option<GenerationManifest> {
        self.memory.manifest.clone()
    }

    fn persist(&self, memory: &InMemoryGraphStore) -> Result<(), StoreError> {
        let snapshot = FileSnapshotV2 {
            version: FILE_SNAPSHOT_VERSION,
            memory: memory.clone(),
        };
        let mut bytes = FILE_SNAPSHOT_MAGIC.to_vec();
        bytes.extend(
            bincode::serialize(&snapshot)
                .map_err(|error| StoreError::Backend(format!("encode snapshot: {error}")))?,
        );
        let (temporary, mut file) = create_snapshot_temporary_file(&self.path)?;
        let write_result = file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| StoreError::Backend(format!("write snapshot: {error}")));
        drop(file);
        if let Err(error) = write_result {
            let cleanup = std::fs::remove_file(&temporary);
            return Err(StoreError::Backend(match cleanup {
                Ok(()) => error.to_string(),
                Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                    error.to_string()
                }
                Err(cleanup_error) => {
                    format!("{error}; temporary cleanup failed: {cleanup_error}")
                }
            }));
        }

        if let Err(error) = std::fs::rename(&temporary, &self.path) {
            let cleanup = std::fs::remove_file(&temporary);
            return Err(StoreError::Backend(match cleanup {
                Ok(()) => format!("replace snapshot: {error}"),
                Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                    format!("replace snapshot: {error}")
                }
                Err(cleanup_error) => {
                    format!("replace snapshot: {error}; temporary cleanup failed: {cleanup_error}")
                }
            }));
        }
        Ok(())
    }
}

static SNAPSHOT_TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

fn decode_file_snapshot(bytes: &[u8]) -> Result<InMemoryGraphStore, StoreError> {
    if let Some(payload) = bytes.strip_prefix(FILE_SNAPSHOT_MAGIC) {
        let version = bincode::deserialize::<u32>(payload)
            .map_err(|error| StoreError::Integrity(format!("decode snapshot version: {error}")))?;
        match version {
            1 => {
                let snapshot: FileSnapshotV1 = bincode::deserialize(payload).map_err(|error| {
                    StoreError::Integrity(format!("decode v1 snapshot: {error}"))
                })?;
                Ok(snapshot.memory.into_current())
            }
            FILE_SNAPSHOT_VERSION => {
                let snapshot: FileSnapshotV2 = bincode::deserialize(payload).map_err(|error| {
                    StoreError::Integrity(format!("decode v2 snapshot: {error}"))
                })?;
                if snapshot.version == FILE_SNAPSHOT_VERSION {
                    Ok(snapshot.memory)
                } else {
                    Err(StoreError::Integrity(format!(
                        "snapshot envelope version {} disagrees with payload {}",
                        version, snapshot.version
                    )))
                }
            }
            _ => Err(StoreError::Integrity(format!(
                "unsupported FileGraphStore snapshot version {version}; expected 1 or {}",
                FILE_SNAPSHOT_VERSION
            ))),
        }
    } else {
        let legacy: LegacyFileSnapshot = bincode::deserialize(bytes)
            .map_err(|error| StoreError::Integrity(format!("decode legacy snapshot: {error}")))?;
        Ok(legacy.into_current())
    }
}

fn create_snapshot_temporary_file(path: &Path) -> Result<(PathBuf, std::fs::File), StoreError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| StoreError::Backend("snapshot path must include a file name".to_owned()))?;
    loop {
        let counter = SNAPSHOT_TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0_u128, |duration| duration.as_nanos());
        let mut temporary_name = file_name.to_os_string();
        temporary_name.push(format!(".tmp-{timestamp}-{}-{counter}", std::process::id()));
        let temporary = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(StoreError::Backend(format!(
                    "create snapshot temporary file: {error}"
                )));
            }
        }
    }
}

impl GraphStore for FileGraphStore {
    fn current_generation(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationManifest>, StoreError> {
        self.memory.current_generation(repository, worktree)
    }

    fn manifest(&self, generation: GenerationId) -> Result<GenerationManifest, StoreError> {
        self.memory.manifest(generation)
    }

    fn generation_history(&self) -> Result<Vec<GenerationHistoryEntry>, StoreError> {
        self.memory.generation_history()
    }

    fn current_generation_entry(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationHistoryEntry>, StoreError> {
        self.memory.current_generation_entry(repository, worktree)
    }

    fn generation_lineage_history(&self) -> Result<Vec<GenerationLineageEntry>, StoreError> {
        self.memory.generation_lineage_history()
    }

    fn generation_consequence_history(
        &self,
    ) -> Result<Vec<GenerationConsequenceEntry>, StoreError> {
        self.memory.generation_consequence_history()
    }

    fn apply_delta_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        let candidate = self.memory.clone();
        let (candidate, manifest) = candidate.stage_delta_with_lineage(request, accepted_at)?;
        self.persist(&candidate)?;
        self.memory = candidate;
        Ok(manifest)
    }

    fn apply_delta_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        let candidate = self.memory.clone();
        let (candidate, manifest) =
            candidate.stage_delta_with_consequences(request, accepted_at)?;
        self.persist(&candidate)?;
        self.memory = candidate;
        Ok(manifest)
    }

    fn events_for_change_set(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
        after: Option<EventsForChangeSetCursor>,
        limit: usize,
    ) -> Result<EventsForChangeSetPage, StoreError> {
        self.memory
            .events_for_change_set(change_set, as_of, after, limit)
    }

    fn change_set_at(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
    ) -> Result<Option<ChangeSetVersion>, StoreError> {
        self.memory.change_set_at(change_set, as_of)
    }

    fn acceptance_time(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError> {
        self.memory.acceptance_time(generation)
    }

    fn apply_delta(&mut self, delta: GraphDelta) -> Result<GenerationManifest, StoreError> {
        let mut candidate = self.memory.clone();
        let manifest = candidate.apply_delta(delta)?;
        self.persist(&candidate)?;
        self.memory = candidate;
        Ok(manifest)
    }

    fn apply_delta_at(
        &mut self,
        delta: GraphDelta,
        accepted_at: AcceptanceTime,
    ) -> Result<GenerationManifest, StoreError> {
        self.apply_delta_with_acceptance_time(delta, Some(accepted_at))
    }

    fn apply_delta_with_acceptance_time(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        let mut candidate = self.memory.clone();
        let manifest = candidate.apply_delta(delta)?;
        if let Some(accepted_at) = accepted_at {
            candidate.records.insert(
                acceptance_record_key(manifest.generation),
                accepted_at.0.to_be_bytes().to_vec(),
            );
        }
        self.persist(&candidate)?;
        self.memory = candidate;
        Ok(manifest)
    }

    fn set_generation_status(
        &mut self,
        generation: GenerationId,
        status: GenerationStatus,
    ) -> Result<GenerationManifest, StoreError> {
        let mut candidate = self.memory.clone();
        let manifest = candidate.set_generation_status(generation, status)?;
        self.persist(&candidate)?;
        self.memory = candidate;
        Ok(manifest)
    }

    fn node(&self, generation: GenerationId, id: NodeId) -> Result<Option<Node>, StoreError> {
        self.memory.node(generation, id)
    }

    fn nodes(&self, generation: GenerationId) -> Result<Vec<Node>, StoreError> {
        self.memory.nodes(generation)
    }

    fn files(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::FileVersion>, StoreError> {
        self.memory.files(generation)
    }

    fn edges(&self, generation: GenerationId) -> Result<Vec<Edge>, StoreError> {
        self.memory.edges(generation)
    }

    fn provenance(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::Provenance>, StoreError> {
        self.memory.provenance(generation)
    }

    fn outgoing(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        self.memory.outgoing(generation, id)
    }

    fn incoming(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        self.memory.incoming(generation, id)
    }

    fn nodes_for_file(
        &self,
        generation: GenerationId,
        file: syntaxmesh_core::FileId,
    ) -> Result<Vec<NodeId>, StoreError> {
        self.memory.nodes_for_file(generation, file)
    }
}

impl DurableRecordStore for InMemoryGraphStore {
    fn read_record(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.records.get(key).cloned())
    }

    fn records_with_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        Ok(self
            .records
            .range(prefix.to_owned()..)
            .take_while(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect())
    }

    fn records_with_prefix_page(
        &self,
        prefix: &str,
        after_key: Option<&str>,
        limit: usize,
    ) -> Result<DurableRecordPage, StoreError> {
        if limit == 0 {
            return Ok(DurableRecordPage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let lower = after_key.map_or_else(
            || std::ops::Bound::Included(prefix.to_owned()),
            |after| {
                if after >= prefix {
                    std::ops::Bound::Excluded(after.to_owned())
                } else {
                    std::ops::Bound::Included(prefix.to_owned())
                }
            },
        );
        let upper = prefix_upper_bound(prefix)
            .map(std::ops::Bound::Excluded)
            .unwrap_or(std::ops::Bound::Unbounded);
        let mut records = self
            .records
            .range((lower, upper))
            .take_while(|(key, _)| key.starts_with(prefix))
            .take(limit.saturating_add(1))
            .map(|(key, payload)| (key.clone(), payload.clone()))
            .collect::<Vec<_>>();
        let has_more = records.len() > limit;
        if has_more {
            records.pop();
        }
        let next_cursor = if has_more {
            records.last().map(|(key, _)| key.clone())
        } else {
            None
        };
        Ok(DurableRecordPage {
            records,
            next_cursor,
        })
    }

    fn last_record_with_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        let lower = std::ops::Bound::Included(prefix.to_owned());
        let upper = prefix_upper_bound(prefix)
            .map(std::ops::Bound::Excluded)
            .unwrap_or(std::ops::Bound::Unbounded);
        Ok(self
            .records
            .range((lower, upper))
            .next_back()
            .map(|(key, value)| (key.clone(), value.clone())))
    }

    fn compare_exchange_record(
        &mut self,
        key: &str,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), StoreError> {
        if self.records.get(key).map(Vec::as_slice) != expected {
            return Err(StoreError::RecordConflict(key.to_owned()));
        }
        self.records.insert(key.to_owned(), replacement.to_vec());
        Ok(())
    }
}

impl DurableRecordStore for FileGraphStore {
    fn read_record(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.memory.read_record(key)
    }

    fn records_with_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        self.memory.records_with_prefix(prefix)
    }

    fn records_with_prefix_page(
        &self,
        prefix: &str,
        after_key: Option<&str>,
        limit: usize,
    ) -> Result<DurableRecordPage, StoreError> {
        self.memory
            .records_with_prefix_page(prefix, after_key, limit)
    }

    fn last_record_with_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        self.memory.last_record_with_prefix(prefix)
    }

    fn compare_exchange_record(
        &mut self,
        key: &str,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), StoreError> {
        let mut candidate = self.memory.clone();
        candidate.compare_exchange_record(key, expected, replacement)?;
        self.persist(&candidate)?;
        self.memory = candidate;
        Ok(())
    }
}

impl BackendIntegrityCheck for FileGraphStore {
    fn backend_integrity_check(&self) -> Result<BackendIntegrityReport, StoreError> {
        let result = (|| {
            let bytes = std::fs::read(&self.path)
                .map_err(|error| StoreError::Backend(format!("read snapshot: {error}")))?;
            let mut restored = decode_file_snapshot(&bytes)?;
            restored.rebuild_indexes();
            let lineage_history = restored.lineage_history.clone();
            restored.restore_generation_lineage_history(lineage_history)?;
            let consequence_history = restored.consequence_history.clone();
            restored.restore_generation_consequence_history(consequence_history)?;
            let restored_bytes = bincode::serialize(&restored).map_err(|error| {
                StoreError::Backend(format!("re-encode snapshot for integrity check: {error}"))
            })?;
            let open_bytes = bincode::serialize(&self.memory).map_err(|error| {
                StoreError::Backend(format!("encode open snapshot for integrity check: {error}"))
            })?;
            Ok::<_, StoreError>(restored_bytes == open_bytes)
        })();
        match result {
            Ok(true) => Ok(BackendIntegrityReport::snapshot(
                true,
                vec!["snapshot decodes and matches open state".to_owned()],
            )),
            Ok(false) => Ok(BackendIntegrityReport::snapshot(
                false,
                vec!["snapshot differs from open state".to_owned()],
            )),
            Err(error) => Ok(BackendIntegrityReport::snapshot(
                false,
                vec![error.to_string()],
            )),
        }
    }
}

impl InMemoryGraphStore {
    /// Derive the accepted transition event using identity and adjacency
    /// indexes, without copying the complete prior graph.
    ///
    /// # Errors
    /// Returns an integrity or serialization error for an invalid event
    /// projection.
    pub fn change_event_for_delta(&self, delta: &GraphDelta) -> Result<ChangeEvent, StoreError> {
        let candidates = crate::lineage::delta_fact_candidates(delta);
        let mut prior_facts = BTreeSet::new();
        for fact in candidates {
            let exists = match fact {
                FactRef::File(id) => self.files.contains_key(&id),
                FactRef::Provenance(id) => self.provenance.contains_key(&id),
                FactRef::Node(id) => self.nodes.contains_key(&id),
                FactRef::Edge(id) => self.edges.contains_key(&id),
            };
            if exists {
                prior_facts.insert(fact);
            }
        }
        let mut incident_edges = BTreeMap::new();
        for node in &delta.remove_nodes {
            if let Some(edges) = self.outgoing_index.get(node) {
                incident_edges.extend(
                    edges
                        .iter()
                        .filter_map(|id| self.edges.get(id).map(|edge| (*id, edge.clone()))),
                );
            }
            if let Some(edges) = self.incoming_index.get(node) {
                incident_edges.extend(
                    edges
                        .iter()
                        .filter_map(|id| self.edges.get(id).map(|edge| (*id, edge.clone()))),
                );
            }
        }
        for edge in incident_edges.values() {
            prior_facts.insert(FactRef::Edge(edge.id));
        }
        crate::lineage::change_event_from_prior(
            delta,
            &prior_facts,
            &incident_edges.into_values().collect::<Vec<_>>(),
        )
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the current manifest for backend persistence adapters.
    #[must_use]
    pub fn current_manifest(&self) -> Option<GenerationManifest> {
        self.manifest.clone()
    }

    fn lineage_state_at(
        &self,
        generation: GenerationId,
    ) -> Result<ChangeSetLineageState, StoreError> {
        let target = self
            .history
            .iter()
            .position(|entry| entry.manifest.generation == generation)
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(generation),
                actual: self.history.last().map(|entry| entry.manifest.generation),
            })?;
        let lineage_by_generation = self
            .lineage_history
            .iter()
            .map(|entry| (entry.generation, &entry.delta))
            .collect::<BTreeMap<_, _>>();
        let mut sets = BTreeMap::new();
        let mut memberships = BTreeMap::new();
        for history_entry in self.history.iter().take(target.saturating_add(1)) {
            let Some(delta) = lineage_by_generation.get(&history_entry.manifest.generation) else {
                continue;
            };
            for set in &delta.upsert_sets {
                sets.insert(set.id, set.clone());
            }
            for membership in &delta.assign_events {
                let key = ChangeSetMembershipKey {
                    change_set: membership.change_set,
                    event: membership.event,
                };
                memberships.insert(key, (*membership, history_entry.manifest.generation));
            }
            for removal in &delta.unassign_events {
                memberships.remove(&removal.key);
            }
        }
        Ok((sets, memberships))
    }

    fn consequence_state_at(
        &self,
        generation: GenerationId,
    ) -> Result<BTreeMap<ConsequenceEdgeId, ConsequenceEdge>, StoreError> {
        let target = self
            .history
            .iter()
            .position(|entry| entry.manifest.generation == generation)
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(generation),
                actual: self.history.last().map(|entry| entry.manifest.generation),
            })?;
        let by_generation = self
            .consequence_history
            .iter()
            .map(|entry| (entry.generation, &entry.delta))
            .collect::<BTreeMap<_, _>>();
        let mut edges = BTreeMap::new();
        for history_entry in self.history.iter().take(target.saturating_add(1)) {
            let Some(delta) = by_generation.get(&history_entry.manifest.generation) else {
                continue;
            };
            for edge in &delta.add {
                edges.insert(edge.id, edge.clone());
            }
            for retraction in &delta.retract {
                edges.remove(&retraction.edge);
            }
        }
        Ok(edges)
    }

    fn validate_consequence_delta(
        &self,
        generation: GenerationId,
        delta: &ConsequenceDelta,
    ) -> Result<(), StoreError> {
        delta
            .validate()
            .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
        let generation_position = self
            .history
            .iter()
            .position(|entry| entry.manifest.generation == generation)
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(generation),
                actual: self.history.last().map(|entry| entry.manifest.generation),
            })?;
        if delta.is_empty() {
            return Ok(());
        }
        let mut active = if generation_position > 0 {
            let parent_index = generation_position.checked_sub(1).ok_or_else(|| {
                StoreError::Integrity("consequence parent index underflow".to_owned())
            })?;
            let parent = self.history.get(parent_index).ok_or_else(|| {
                StoreError::Integrity("consequence generation parent is missing".to_owned())
            })?;
            self.consequence_state_at(parent.manifest.generation)?
        } else {
            BTreeMap::new()
        };
        let mut all_prior_ids = self
            .consequence_history
            .iter()
            .take_while(|entry| entry.generation != generation)
            .flat_map(|entry| entry.delta.add.iter().map(|edge| edge.id))
            .collect::<BTreeSet<_>>();
        for retraction in &delta.retract {
            if active.remove(&retraction.edge).is_none() {
                return Err(StoreError::InvalidDelta(format!(
                    "cannot retract inactive consequence edge {:?}",
                    retraction.edge
                )));
            }
            if !self.provenance.contains_key(&retraction.provenance) {
                return Err(StoreError::InvalidDelta(format!(
                    "consequence retraction references unknown provenance {:?}",
                    retraction.provenance
                )));
            }
        }
        let history = self.history.iter().cloned().collect::<Vec<_>>();
        let events = crate::lineage::change_events_from_history(&history)?;
        let event_ids = events.iter().map(|event| event.id).collect::<BTreeSet<_>>();
        for edge in &delta.add {
            if !all_prior_ids.insert(edge.id) {
                return Err(StoreError::InvalidDelta(format!(
                    "consequence edge ID {:?} was already used",
                    edge.id
                )));
            }
            if !self.provenance.contains_key(&edge.provenance) {
                return Err(StoreError::InvalidDelta(format!(
                    "consequence edge references unknown provenance {:?}",
                    edge.provenance
                )));
            }
            for endpoint in [edge.source, edge.target] {
                match endpoint {
                    LineageEndpoint::ChangeEvent(id) if !event_ids.contains(&id) => {
                        return Err(StoreError::InvalidDelta(format!(
                            "consequence endpoint references unknown event {id:?}"
                        )));
                    }
                    LineageEndpoint::FactVersion(reference)
                        if !self.has_fact_version(reference)? =>
                    {
                        return Err(StoreError::InvalidDelta(format!(
                            "consequence endpoint references unknown fact version {reference:?}"
                        )));
                    }
                    LineageEndpoint::ChangeEvent(_) | LineageEndpoint::FactVersion(_) => {}
                }
            }
            for reference in &edge.evidence {
                if !self.has_fact_version(*reference)? {
                    return Err(StoreError::InvalidDelta(format!(
                        "consequence evidence references unknown fact version {reference:?}"
                    )));
                }
            }
            if let syntaxmesh_core::ConsequenceDerivation::DerivedFrom(parents) = &edge.derivation
                && parents.iter().any(|parent| !active.contains_key(parent))
            {
                return Err(StoreError::InvalidDelta(
                    "derived consequence references an inactive parent edge".to_owned(),
                ));
            }
            active.insert(edge.id, edge.clone());
        }
        Ok(())
    }

    fn has_fact_version(&self, reference: FactVersionRef) -> Result<bool, StoreError> {
        Ok(GraphStore::fact_history(self, reference.fact)?
            .iter()
            .any(|version| version.valid_from == reference.valid_from))
    }

    fn validate_lineage_delta(
        &self,
        generation: GenerationId,
        delta: &ChangeSetDelta,
        prior_sets: &BTreeMap<ChangeSetId, syntaxmesh_core::ChangeSet>,
        prior_memberships: &BTreeMap<ChangeSetMembershipKey, (ChangeSetMembership, GenerationId)>,
    ) -> Result<(), StoreError> {
        delta
            .validate()
            .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
        let provenance_exists = |id: ProvenanceId| self.provenance.contains_key(&id);
        let mut next_sets = prior_sets.clone();
        for set in &delta.upsert_sets {
            if !provenance_exists(set.provenance) {
                return Err(StoreError::InvalidDelta(format!(
                    "change set {:?} references unknown provenance {:?}",
                    set.id, set.provenance
                )));
            }
            if set
                .originating_intent
                .is_some_and(|id| !self.nodes.contains_key(&id))
            {
                return Err(StoreError::InvalidDelta(format!(
                    "change set {:?} references an unknown originating intent",
                    set.id
                )));
            }
            if set.adrs.iter().any(|id| !self.nodes.contains_key(id)) {
                return Err(StoreError::InvalidDelta(format!(
                    "change set {:?} references an unknown ADR node",
                    set.id
                )));
            }
            let first = self.generation_sequence(set.first_generation)?;
            let last = set
                .last_generation
                .map(|last| self.generation_sequence(last))
                .transpose()?
                .unwrap_or(self.generation_sequence(generation)?);
            if first > last {
                return Err(StoreError::InvalidDelta(format!(
                    "change set {:?} has inverted generation bounds",
                    set.id
                )));
            }
            next_sets.insert(set.id, set.clone());
        }
        for set in &delta.upsert_sets {
            if set
                .parent_changes
                .iter()
                .any(|parent| !next_sets.contains_key(parent))
            {
                return Err(StoreError::InvalidDelta(format!(
                    "change set {:?} references an unknown parent change set",
                    set.id
                )));
            }
        }
        let event_ids = if delta.assign_events.is_empty() {
            None
        } else {
            Some(
                self.history
                    .iter()
                    .filter_map(|entry| entry.delta.as_ref())
                    .map(crate::lineage::change_event_id)
                    .collect::<Result<BTreeSet<_>, _>>()?,
            )
        };
        for membership in &delta.assign_events {
            if !next_sets.contains_key(&membership.change_set) {
                return Err(StoreError::InvalidDelta(format!(
                    "membership references unknown change set {:?}",
                    membership.change_set
                )));
            }
            if !event_ids
                .as_ref()
                .is_some_and(|event_ids| event_ids.contains(&membership.event))
            {
                return Err(StoreError::InvalidDelta(format!(
                    "membership references unknown change event {:?}",
                    membership.event
                )));
            }
            if !provenance_exists(membership.provenance) {
                return Err(StoreError::InvalidDelta(format!(
                    "membership references unknown provenance {:?}",
                    membership.provenance
                )));
            }
        }
        for removal in &delta.unassign_events {
            if !prior_memberships.contains_key(&removal.key) {
                return Err(StoreError::InvalidDelta(format!(
                    "cannot remove inactive change-set membership {:?}",
                    removal.key
                )));
            }
            if !provenance_exists(removal.provenance) {
                return Err(StoreError::InvalidDelta(format!(
                    "membership removal references unknown provenance {:?}",
                    removal.provenance
                )));
            }
        }
        Ok(())
    }

    fn snapshot_for_generation(
        &self,
        generation: GenerationId,
    ) -> Result<GraphSnapshot, StoreError> {
        self.manifest(generation)?;
        Ok(GraphSnapshot {
            files: self.files.values().cloned().collect(),
            provenance: self.provenance.values().cloned().collect(),
            nodes: self.nodes.values().cloned().collect(),
            edges: self.edges.values().cloned().collect(),
        })
    }

    /// Restore one validated snapshot anchor into a replay store.
    ///
    /// # Errors
    /// Returns an integrity error if references or the graph root are invalid.
    pub fn restore_history_anchor(
        &mut self,
        entry: &GenerationHistoryEntry,
    ) -> Result<(), StoreError> {
        let snapshot = entry.anchor.as_ref().ok_or_else(|| {
            StoreError::Integrity("generation history entry is not an anchor".to_owned())
        })?;
        let files = snapshot
            .files
            .iter()
            .cloned()
            .map(|file| (file.file_id, file))
            .collect();
        let provenance = snapshot
            .provenance
            .iter()
            .cloned()
            .map(|item| (item.id, item))
            .collect();
        let nodes = snapshot
            .nodes
            .iter()
            .cloned()
            .map(|node| (node.id, node))
            .collect();
        let edges = snapshot
            .edges
            .iter()
            .cloned()
            .map(|edge| (edge.id, edge))
            .collect();
        Self::validate_references(&provenance, &nodes, &edges)?;
        let (root, persistent_state) = match entry.manifest.schema_version {
            1 => (Self::root(entry.manifest.generation, &nodes, &edges)?, None),
            2 => {
                let (root, fact_root, cache) = Self::root_v2_with_cache(
                    entry.manifest.generation,
                    &files,
                    &provenance,
                    &nodes,
                    &edges,
                )?;
                (root, Some((fact_root, cache)))
            }
            version => {
                return Err(StoreError::Unsupported(format!(
                    "unsupported generation root schema version {version}"
                )));
            }
        };
        if root != entry.manifest.graph_root {
            return Err(StoreError::Integrity(
                "history anchor does not match its manifest root".to_owned(),
            ));
        }
        self.repository = Some(entry.manifest.repository);
        self.worktree = Some(entry.manifest.worktree);
        self.manifest = Some(entry.manifest.clone());
        self.files = files;
        self.provenance = provenance;
        self.nodes = nodes;
        self.edges = edges;
        self.rebuild_indexes();
        if let Some((fact_root, cache)) = persistent_state {
            self.persistent_fact_root = fact_root;
            self.persistent_fact_root_ready = true;
            self.persistent_fact_cache = cache;
        } else {
            self.persistent_fact_root = None;
            self.persistent_fact_root_ready = false;
            self.persistent_fact_cache = PersistentFactTreeCache::default();
        }
        Ok(())
    }

    /// Replace reconstructed history during durable-backend restore.
    ///
    /// # Errors
    /// Rejects entries whose parent chain is not contiguous or whose final
    /// generation differs from the restored current manifest.
    pub fn restore_generation_history(
        &mut self,
        history: Vec<GenerationHistoryEntry>,
    ) -> Result<(), StoreError> {
        for pair in history.windows(2) {
            let (Some(parent), Some(child)) = (pair.first(), pair.get(1)) else {
                return Err(StoreError::Integrity(
                    "generation history contains an invalid pair".to_owned(),
                ));
            };
            if child.manifest.parent != Some(parent.manifest.generation) {
                return Err(StoreError::Integrity(
                    "generation history parent chain is not contiguous".to_owned(),
                ));
            }
        }
        if history.last().map(|entry| entry.manifest.generation)
            != self.manifest.as_ref().map(|manifest| manifest.generation)
        {
            return Err(StoreError::Integrity(
                "generation history does not end at the current manifest".to_owned(),
            ));
        }
        self.history = history.into_iter().collect();
        Ok(())
    }

    /// Restore accepted typed lineage deltas after validating that each entry
    /// belongs to a retained generation. Missing entries become empty deltas
    /// for history predating explicit ChangeSet support.
    ///
    /// # Errors
    /// Returns an error when entries are duplicated or reference unretained generations.
    pub fn restore_generation_lineage_history(
        &mut self,
        lineage_history: Vec<GenerationLineageEntry>,
    ) -> Result<(), StoreError> {
        let mut by_generation = BTreeMap::new();
        for entry in lineage_history {
            if by_generation
                .insert(entry.generation, entry.delta)
                .is_some()
            {
                return Err(StoreError::Integrity(
                    "generation lineage contains duplicate generation entries".to_owned(),
                ));
            }
        }
        let retained = self
            .history
            .iter()
            .map(|entry| entry.manifest.generation)
            .collect::<BTreeSet<_>>();
        if by_generation
            .keys()
            .any(|generation| !retained.contains(generation))
        {
            return Err(StoreError::Integrity(
                "generation lineage references an unretained generation".to_owned(),
            ));
        }
        self.lineage_history = self
            .history
            .iter()
            .map(|entry| GenerationLineageEntry {
                generation: entry.manifest.generation,
                delta: by_generation
                    .remove(&entry.manifest.generation)
                    .unwrap_or_default(),
            })
            .collect();
        Ok(())
    }

    /// Restore consequence assertions for retained generations, filling
    /// generations predating this feature with empty deltas.
    ///
    /// # Errors
    /// Rejects duplicate or unretained generations and locally invalid deltas.
    pub fn restore_generation_consequence_history(
        &mut self,
        consequence_history: Vec<GenerationConsequenceEntry>,
    ) -> Result<(), StoreError> {
        let mut by_generation = BTreeMap::new();
        for entry in consequence_history {
            entry
                .delta
                .validate()
                .map_err(|error| StoreError::Integrity(error.to_string()))?;
            if by_generation
                .insert(entry.generation, entry.delta)
                .is_some()
            {
                return Err(StoreError::Integrity(
                    "generation consequence journal contains duplicate generations".to_owned(),
                ));
            }
        }
        let retained = self
            .history
            .iter()
            .map(|entry| entry.manifest.generation)
            .collect::<BTreeSet<_>>();
        if by_generation
            .keys()
            .any(|generation| !retained.contains(generation))
        {
            return Err(StoreError::Integrity(
                "consequence journal references an unretained generation".to_owned(),
            ));
        }
        self.consequence_history = self
            .history
            .iter()
            .map(|entry| GenerationConsequenceEntry {
                generation: entry.manifest.generation,
                delta: by_generation
                    .remove(&entry.manifest.generation)
                    .unwrap_or_default(),
            })
            .collect();
        Ok(())
    }

    fn rebuild_indexes(&mut self) {
        self.outgoing_index.clear();
        self.incoming_index.clear();
        self.nodes_by_file_index.clear();
        self.nodes_by_name_index.clear();
        self.nodes_by_terminal_index.clear();
        for node in self.nodes.values() {
            self.nodes_by_name_index
                .entry(node.name.clone())
                .or_default()
                .insert(node.id);
            if let Some(terminal) = node.name.rsplit("::").next() {
                self.nodes_by_terminal_index
                    .entry(terminal.to_owned())
                    .or_default()
                    .insert(node.id);
            }
            if let Some(file) = node.owner_file {
                self.nodes_by_file_index
                    .entry(file)
                    .or_default()
                    .insert(node.id);
            }
        }
        for edge in self.edges.values() {
            self.outgoing_index
                .entry(edge.source)
                .or_default()
                .insert(edge.id);
            self.incoming_index
                .entry(edge.target)
                .or_default()
                .insert(edge.id);
        }
    }

    fn unindex_node(&mut self, node: &Node) {
        remove_index_value(&mut self.nodes_by_name_index, &node.name, &node.id);
        if let Some(terminal) = node.name.rsplit("::").next() {
            remove_index_value(
                &mut self.nodes_by_terminal_index,
                &terminal.to_owned(),
                &node.id,
            );
        }
        if let Some(file) = node.owner_file {
            remove_index_value(&mut self.nodes_by_file_index, &file, &node.id);
        }
    }

    fn index_node(&mut self, node: &Node) {
        self.nodes_by_name_index
            .entry(node.name.clone())
            .or_default()
            .insert(node.id);
        if let Some(terminal) = node.name.rsplit("::").next() {
            self.nodes_by_terminal_index
                .entry(terminal.to_owned())
                .or_default()
                .insert(node.id);
        }
        if let Some(file) = node.owner_file {
            self.nodes_by_file_index
                .entry(file)
                .or_default()
                .insert(node.id);
        }
    }

    fn unindex_edge(&mut self, edge: &Edge) {
        remove_index_value(&mut self.outgoing_index, &edge.source, &edge.id);
        remove_index_value(&mut self.incoming_index, &edge.target, &edge.id);
    }

    fn index_edge(&mut self, edge: &Edge) {
        self.outgoing_index
            .entry(edge.source)
            .or_default()
            .insert(edge.id);
        self.incoming_index
            .entry(edge.target)
            .or_default()
            .insert(edge.id);
    }

    fn apply_delta_indexes(&mut self, delta: &GraphDelta) {
        for node_id in &delta.remove_nodes {
            if let Some(old) = self.nodes.get(node_id).cloned() {
                self.unindex_node(&old);
            }
        }
        for node in &delta.upsert_nodes {
            if let Some(old) = self.nodes.get(&node.id).cloned() {
                self.unindex_node(&old);
            }
            self.index_node(node);
        }

        for edge_id in cascaded_edge_ids(self, delta) {
            if let Some(old) = self.edges.get(&edge_id).cloned() {
                self.unindex_edge(&old);
            }
        }
        for edge in &delta.upsert_edges {
            if let Some(old) = self.edges.get(&edge.id).cloned() {
                self.unindex_edge(&old);
            }
            self.index_edge(edge);
        }
    }

    fn validate_references(
        provenance: &OrdMap<ProvenanceId, Provenance>,
        nodes: &OrdMap<NodeId, Node>,
        edges: &OrdMap<EdgeId, Edge>,
    ) -> Result<(), StoreError> {
        for node in nodes.values() {
            if !provenance.contains_key(&node.provenance) {
                return Err(StoreError::Integrity(format!(
                    "node {:?} references missing provenance {:?}",
                    node.id, node.provenance
                )));
            }
        }
        for edge in edges.values() {
            if !provenance.contains_key(&edge.provenance) {
                return Err(StoreError::Integrity(format!(
                    "edge {:?} references missing provenance {:?}",
                    edge.id, edge.provenance
                )));
            }
            if !nodes.contains_key(&edge.source) || !nodes.contains_key(&edge.target) {
                return Err(StoreError::Integrity(format!(
                    "edge {:?} references a missing endpoint",
                    edge.id
                )));
            }
        }
        Ok(())
    }

    /// Validate only references introduced or affected by a delta.
    ///
    /// The previously accepted state has already passed a full validation.
    /// Deltas cannot remove provenance, and removing a node also removes every
    /// incident edge, so unchanged facts retain valid references by induction.
    fn validate_delta_references(
        provenance: &OrdMap<ProvenanceId, Provenance>,
        nodes: &OrdMap<NodeId, Node>,
        edges: &OrdMap<EdgeId, Edge>,
        delta: &GraphDelta,
    ) -> Result<(), StoreError> {
        for node in &delta.upsert_nodes {
            if !provenance.contains_key(&node.provenance) {
                return Err(StoreError::Integrity(format!(
                    "node {:?} references missing provenance {:?}",
                    node.id, node.provenance
                )));
            }
            if !nodes.contains_key(&node.id) {
                return Err(StoreError::Integrity(format!(
                    "upserted node {:?} is missing from the candidate state",
                    node.id
                )));
            }
        }
        for edge in &delta.upsert_edges {
            if !provenance.contains_key(&edge.provenance) {
                return Err(StoreError::Integrity(format!(
                    "edge {:?} references missing provenance {:?}",
                    edge.id, edge.provenance
                )));
            }
            if !nodes.contains_key(&edge.source) || !nodes.contains_key(&edge.target) {
                return Err(StoreError::Integrity(format!(
                    "edge {:?} references a missing endpoint",
                    edge.id
                )));
            }
            if !edges.contains_key(&edge.id) {
                return Err(StoreError::Integrity(format!(
                    "upserted edge {:?} is missing from the candidate state",
                    edge.id
                )));
            }
        }
        Ok(())
    }

    fn root(
        generation: GenerationId,
        nodes: &OrdMap<NodeId, Node>,
        edges: &OrdMap<EdgeId, Edge>,
    ) -> Result<[u8; 32], StoreError> {
        let mut bytes = Vec::new();
        for node in nodes.values() {
            bytes.extend_from_slice(&serde_json::to_vec(node).map_err(|error| {
                StoreError::Backend(format!("serialize node for root: {error}"))
            })?);
        }
        for edge in edges.values() {
            bytes.extend_from_slice(&serde_json::to_vec(edge).map_err(|error| {
                StoreError::Backend(format!("serialize edge for root: {error}"))
            })?);
        }
        Ok(*blake3::hash(&[generation.0.0.as_slice(), bytes.as_slice()].concat()).as_bytes())
    }

    fn root_v2_with_cache(
        generation: GenerationId,
        files: &OrdMap<syntaxmesh_core::FileId, syntaxmesh_core::FileVersion>,
        provenance: &OrdMap<ProvenanceId, Provenance>,
        nodes: &OrdMap<NodeId, Node>,
        edges: &OrdMap<EdgeId, Edge>,
    ) -> Result<
        (
            [u8; 32],
            Option<syntaxmesh_core::StableId>,
            PersistentFactTreeCache,
        ),
        StoreError,
    > {
        let mut mutations = Vec::new();
        for file in files.values() {
            push_canonical_fact(&mut mutations, 0, file.file_id.0, file)?;
        }
        for item in provenance.values() {
            push_canonical_fact(&mut mutations, 1, item.id.0, item)?;
        }
        for node in nodes.values() {
            push_canonical_fact(&mut mutations, 2, node.id.0, node)?;
        }
        for edge in edges.values() {
            push_canonical_fact(&mut mutations, 3, edge.id.0, edge)?;
        }
        let mut cache = PersistentFactTreeCache::default();
        let fact_root = PersistentFactTree::build_sorted_owned(mutations, &mut cache);
        cache.discard_dirty();
        Ok((
            generation_root_v2(generation.0, fact_root),
            fact_root,
            cache,
        ))
    }

    fn apply_delta_for_schema(
        &mut self,
        delta: &GraphDelta,
        schema_version: u32,
    ) -> Result<GenerationManifest, StoreError> {
        apply_delta_to_memory(self, delta, schema_version)
    }
}

/// Compute the version-two canonical root for a complete graph snapshot.
///
/// Durable publishers use their persistent tree root for incremental updates;
/// this full-snapshot implementation is the reference, restore, and integrity
/// oracle shared across adapters.
///
/// # Errors
///
/// Returns an integrity error if a fact cannot be canonically serialized or
/// the persistent fact tree cannot be constructed.
pub fn graph_snapshot_root_v2(
    generation: GenerationId,
    snapshot: &GraphSnapshot,
) -> Result<[u8; 32], StoreError> {
    let mut mutations = BTreeMap::new();
    for file in &snapshot.files {
        insert_canonical_fact(&mut mutations, 0, file.file_id.0, file)?;
    }
    for item in &snapshot.provenance {
        insert_canonical_fact(&mut mutations, 1, item.id.0, item)?;
    }
    for node in &snapshot.nodes {
        insert_canonical_fact(&mut mutations, 2, node.id.0, node)?;
    }
    for edge in &snapshot.edges {
        insert_canonical_fact(&mut mutations, 3, edge.id.0, edge)?;
    }
    let mut cache = PersistentFactTreeCache::default();
    let fact_root =
        PersistentFactTree::build_sorted_owned(mutations.into_values().collect(), &mut cache);
    Ok(generation_root_v2(generation.0, fact_root))
}

fn push_canonical_fact<T: Serialize>(
    mutations: &mut Vec<PersistentFactMutation>,
    fact_kind: u8,
    fact_id: syntaxmesh_core::StableId,
    fact: &T,
) -> Result<(), StoreError> {
    let key = PersistentFactKey { fact_kind, fact_id };
    let value = serde_json::to_vec(fact)
        .map_err(|error| StoreError::Backend(format!("serialize canonical fact: {error}")))?;
    mutations.push(PersistentFactMutation::Upsert { key, value });
    Ok(())
}

fn insert_canonical_fact<T: Serialize>(
    mutations: &mut BTreeMap<PersistentFactKey, PersistentFactMutation>,
    fact_kind: u8,
    fact_id: syntaxmesh_core::StableId,
    fact: &T,
) -> Result<(), StoreError> {
    let key = PersistentFactKey { fact_kind, fact_id };
    let value = serde_json::to_vec(fact)
        .map_err(|error| StoreError::Backend(format!("serialize canonical fact: {error}")))?;
    mutations.insert(key, PersistentFactMutation::Upsert { key, value });
    Ok(())
}

fn remove_index_value<K: Clone + Ord, V: Clone + Ord>(
    index: &mut OrdMap<K, BTreeSet<V>>,
    key: &K,
    value: &V,
) {
    if let Some(values) = index.get_mut(key) {
        values.remove(value);
        if values.is_empty() {
            index.remove(key);
        }
    }
}

fn cascaded_edge_ids(store: &InMemoryGraphStore, delta: &GraphDelta) -> BTreeSet<EdgeId> {
    let mut removed_edges = delta.remove_edges.iter().copied().collect::<BTreeSet<_>>();
    for node_id in &delta.remove_nodes {
        if let Some(edge_ids) = store.outgoing_index.get(node_id) {
            removed_edges.extend(edge_ids.iter().copied());
        }
        if let Some(edge_ids) = store.incoming_index.get(node_id) {
            removed_edges.extend(edge_ids.iter().copied());
        }
    }
    removed_edges
}

fn validate_delta_references_in_place(
    store: &InMemoryGraphStore,
    delta: &GraphDelta,
) -> Result<(), StoreError> {
    let removed_nodes = delta.remove_nodes.iter().copied().collect::<BTreeSet<_>>();
    let upserted_nodes = delta
        .upsert_nodes
        .iter()
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    let upserted_provenance = delta
        .upsert_provenance
        .iter()
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let has_provenance =
        |id| store.provenance.contains_key(&id) || upserted_provenance.contains(&id);
    let has_node = |id| {
        upserted_nodes.contains(&id)
            || (store.nodes.contains_key(&id) && !removed_nodes.contains(&id))
    };
    for node in &delta.upsert_nodes {
        if !has_provenance(node.provenance) {
            return Err(StoreError::Integrity(format!(
                "node {:?} references missing provenance {:?}",
                node.id, node.provenance
            )));
        }
    }
    for edge in &delta.upsert_edges {
        if !has_provenance(edge.provenance) {
            return Err(StoreError::Integrity(format!(
                "edge {:?} references missing provenance {:?}",
                edge.id, edge.provenance
            )));
        }
        if !has_node(edge.source) || !has_node(edge.target) {
            return Err(StoreError::Integrity(format!(
                "edge {:?} references a missing endpoint",
                edge.id
            )));
        }
    }
    Ok(())
}

fn delta_tree_mutations(
    store: &InMemoryGraphStore,
    delta: &GraphDelta,
) -> Result<Vec<PersistentFactMutation>, StoreError> {
    let mut mutations = BTreeMap::new();
    for file in &delta.changed_files {
        insert_canonical_fact(&mut mutations, 0, file.file_id.0, file)?;
    }
    for file_id in &delta.removed_files {
        let key = PersistentFactKey {
            fact_kind: 0,
            fact_id: file_id.0,
        };
        mutations.insert(key, PersistentFactMutation::Remove { key });
    }
    for item in &delta.upsert_provenance {
        insert_canonical_fact(&mut mutations, 1, item.id.0, item)?;
    }
    for node_id in &delta.remove_nodes {
        let key = PersistentFactKey {
            fact_kind: 2,
            fact_id: node_id.0,
        };
        mutations.insert(key, PersistentFactMutation::Remove { key });
    }
    for node in &delta.upsert_nodes {
        insert_canonical_fact(&mut mutations, 2, node.id.0, node)?;
    }
    for edge_id in cascaded_edge_ids(store, delta) {
        let key = PersistentFactKey {
            fact_kind: 3,
            fact_id: edge_id.0,
        };
        mutations.insert(key, PersistentFactMutation::Remove { key });
    }
    for edge in &delta.upsert_edges {
        insert_canonical_fact(&mut mutations, 3, edge.id.0, edge)?;
    }
    Ok(mutations.into_values().collect())
}

fn apply_delta_to_memory(
    store: &mut InMemoryGraphStore,
    delta: &GraphDelta,
    schema_version: u32,
) -> Result<GenerationManifest, StoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let mut profile = DeltaApplicationProfile::new();
    delta
        .validate()
        .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
    if store
        .repository
        .is_some_and(|value| value != delta.repository)
        || store.worktree.is_some_and(|value| value != delta.worktree)
    {
        return Err(StoreError::Backend(
            "store is already bound to another repository or worktree".to_owned(),
        ));
    }
    let actual = store.manifest.as_ref().map(|manifest| manifest.generation);
    if actual != delta.expected_base {
        return Err(StoreError::StaleBase {
            expected: delta.expected_base,
            actual,
        });
    }
    if actual == Some(delta.next_generation) {
        return store.manifest.clone().ok_or_else(|| {
            StoreError::Integrity("idempotent generation has no manifest".to_owned())
        });
    }

    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("delta_preflight");

    let committed_delta = delta.clone();
    let delta_tree =
        if schema_version == 2 && (store.persistent_fact_root_ready || actual.is_none()) {
            Some(delta_tree_mutations(store, &committed_delta)?)
        } else {
            None
        };
    let cascaded_edges = cascaded_edge_ids(store, &committed_delta);
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("mutation_plan");
    let needs_candidate_maps = schema_version == 1 || delta_tree.is_none();
    let mut candidate_maps = needs_candidate_maps.then(|| {
        (
            store.files.clone(),
            store.provenance.clone(),
            store.nodes.clone(),
            store.edges.clone(),
        )
    });
    if let Some((files, provenance, nodes, edges)) = &mut candidate_maps {
        for file in &committed_delta.changed_files {
            files.insert(file.file_id, file.clone());
        }
        for file_id in &committed_delta.removed_files {
            files.remove(file_id);
        }
        for item in &committed_delta.upsert_provenance {
            provenance.insert(item.id, item.clone());
        }
        for edge_id in &cascaded_edges {
            edges.remove(edge_id);
        }
        for node_id in &committed_delta.remove_nodes {
            nodes.remove(node_id);
        }
        for node in &committed_delta.upsert_nodes {
            nodes.insert(node.id, node.clone());
        }
        for edge in &committed_delta.upsert_edges {
            edges.insert(edge.id, edge.clone());
        }
        InMemoryGraphStore::validate_delta_references(provenance, nodes, edges, &committed_delta)?;
    } else {
        validate_delta_references_in_place(store, &committed_delta)?;
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("candidate_state_and_reference_validation");
    let mut next_persistent_root = None;
    let mut next_persistent_cache = None;
    let graph_root = match schema_version {
        1 => {
            let (_, _, nodes, edges) = candidate_maps.as_ref().ok_or_else(|| {
                StoreError::Integrity("schema-v1 root is missing its candidate graph".to_owned())
            })?;
            InMemoryGraphStore::root(delta.next_generation, nodes, edges)?
        }
        2 => {
            if let Some(mutations) = delta_tree {
                let mut cache = std::mem::take(&mut store.persistent_fact_cache);
                let old_root = if store.persistent_fact_root_ready {
                    store.persistent_fact_root
                } else {
                    None
                };
                let fact_root = match PersistentFactTree::apply(old_root, &mutations, &mut cache) {
                    Ok(root) => root,
                    Err(error) => {
                        store.persistent_fact_root = None;
                        store.persistent_fact_root_ready = false;
                        store.persistent_fact_cache = PersistentFactTreeCache::default();
                        return Err(StoreError::Integrity(format!(
                            "cannot apply canonical fact-tree delta: {error:?}"
                        )));
                    }
                };
                cache.discard_dirty();
                let graph_root = generation_root_v2(delta.next_generation.0, fact_root);
                next_persistent_root = Some(fact_root);
                next_persistent_cache = Some(cache);
                graph_root
            } else {
                let (files, provenance, nodes, edges) =
                    candidate_maps.as_ref().ok_or_else(|| {
                        StoreError::Integrity(
                            "schema-v2 bootstrap is missing its candidate graph".to_owned(),
                        )
                    })?;
                let (graph_root, fact_root, cache) = InMemoryGraphStore::root_v2_with_cache(
                    delta.next_generation,
                    files,
                    provenance,
                    nodes,
                    edges,
                )?;
                next_persistent_root = Some(fact_root);
                next_persistent_cache = Some(cache);
                graph_root
            }
        }
        version => {
            return Err(StoreError::Unsupported(format!(
                "unsupported generation root schema version {version}"
            )));
        }
    };
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("persistent_root_update");
    let manifest = GenerationManifest {
        repository: delta.repository,
        worktree: delta.worktree,
        generation: delta.next_generation,
        parent: delta.expected_base,
        graph_root,
        configuration_hash: [0; 32],
        extractor_set_hash: [0; 32],
        schema_version,
        status: GenerationStatus::Durable,
    };
    store.repository = Some(delta.repository);
    store.worktree = Some(delta.worktree);
    if let Some((files, provenance, nodes, edges)) = candidate_maps {
        store.files = files;
        store.provenance = provenance;
        store.nodes = nodes;
        store.edges = edges;
        store.rebuild_indexes();
    } else {
        store.apply_delta_indexes(&committed_delta);
        for file in &committed_delta.changed_files {
            store.files.insert(file.file_id, file.clone());
        }
        for file_id in &committed_delta.removed_files {
            store.files.remove(file_id);
        }
        for item in &committed_delta.upsert_provenance {
            store.provenance.insert(item.id, item.clone());
        }
        for edge_id in &cascaded_edges {
            store.edges.remove(edge_id);
        }
        for node_id in &committed_delta.remove_nodes {
            store.nodes.remove(node_id);
        }
        for node in &committed_delta.upsert_nodes {
            store.nodes.insert(node.id, node.clone());
        }
        for edge in &committed_delta.upsert_edges {
            store.edges.insert(edge.id, edge.clone());
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("current_state_and_indexes");
    if let (Some(root), Some(cache)) = (next_persistent_root, next_persistent_cache) {
        store.persistent_fact_root = root;
        store.persistent_fact_root_ready = true;
        store.persistent_fact_cache = cache;
    } else {
        store.persistent_fact_root = None;
        store.persistent_fact_root_ready = false;
        store.persistent_fact_cache = PersistentFactTreeCache::default();
    }
    store.manifest = Some(manifest.clone());
    store.history.push_back(GenerationHistoryEntry {
        manifest: manifest.clone(),
        delta: Some(committed_delta),
        anchor: None,
    });
    store.lineage_history.push(GenerationLineageEntry {
        generation: manifest.generation,
        delta: ChangeSetDelta::default(),
    });
    store.consequence_history.push(GenerationConsequenceEntry {
        generation: manifest.generation,
        delta: ConsequenceDelta::default(),
    });
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("generation_history_append");
    Ok(manifest)
}

impl InMemoryGraphStore {
    /// Stage a graph transition and explicit ChangeSet lineage into an owned,
    /// detached candidate. The caller decides whether and when to publish the
    /// returned candidate. On error, this consumed candidate is dropped; callers
    /// must retain their original store separately and must not publish partial
    /// state.
    ///
    /// This is intended for adapters that already own an isolated candidate and
    /// must persist it before making it visible through their live handle.
    ///
    /// # Errors
    /// Returns an error when the request is invalid, stale, references missing
    /// facts, or fails lineage validation. The consumed candidate is dropped.
    pub fn stage_delta_with_lineage(
        mut self,
        request: GraphDeltaWithLineage,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<(Self, GenerationManifest), StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut profile = DeltaApplicationProfile::new();
        request
            .validate()
            .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_request_validation");
        if self.manifest.as_ref().map(|manifest| manifest.generation)
            == Some(request.graph.next_generation)
        {
            let existing = self
                .lineage_history
                .iter()
                .find(|entry| entry.generation == request.graph.next_generation)
                .map(|entry| &entry.delta);
            if existing == Some(&request.lineage) {
                let manifest = self.manifest.clone().ok_or_else(|| {
                    StoreError::Integrity("idempotent generation has no manifest".to_owned())
                })?;
                return Ok((self, manifest));
            }
            return Err(StoreError::InvalidDelta(
                "retry changed the accepted lineage delta".to_owned(),
            ));
        }
        let (prior_sets, prior_memberships) = match request.graph.expected_base {
            Some(base) => self.lineage_state_at(base)?,
            None => (BTreeMap::new(), BTreeMap::new()),
        };
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_prior_state_lookup");
        let GraphDeltaWithLineage { graph, lineage } = request;
        let manifest = self.apply_delta_with_acceptance_time(graph, accepted_at)?;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_graph_delta_apply");
        self.validate_lineage_delta(
            manifest.generation,
            &lineage,
            &prior_sets,
            &prior_memberships,
        )?;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_delta_validation");
        let entry = self
            .lineage_history
            .last_mut()
            .filter(|entry| entry.generation == manifest.generation)
            .ok_or_else(|| {
                StoreError::Integrity("accepted generation is missing its lineage slot".to_owned())
            })?;
        entry.delta = lineage;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_candidate_ready");
        Ok((self, manifest))
    }

    /// Stage a graph transition, explicit ChangeSet lineage, and explicit
    /// consequences into an owned, detached candidate. As with
    /// [`Self::stage_delta_with_lineage`], errors consume and drop the
    /// candidate; the caller must keep its committed store separately.
    ///
    /// # Errors
    /// Returns an error when graph, lineage, or consequence validation fails or
    /// when the request does not extend the candidate's current generation.
    pub fn stage_delta_with_consequences(
        self,
        request: GraphDeltaWithConsequences,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<(Self, GenerationManifest), StoreError> {
        request
            .validate()
            .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
        let generation = request.publication.graph.next_generation;
        if self.manifest.as_ref().map(|manifest| manifest.generation) == Some(generation) {
            let stored = self
                .consequence_history
                .iter()
                .find(|entry| entry.generation == generation)
                .map(|entry| &entry.delta);
            if stored != Some(&request.consequences) {
                return Err(StoreError::InvalidDelta(
                    "retry changed the accepted consequence delta".to_owned(),
                ));
            }
            return self.stage_delta_with_lineage(request.publication, accepted_at);
        }

        let (mut candidate, manifest) =
            self.stage_delta_with_lineage(request.publication, accepted_at)?;
        #[cfg(feature = "benchmark-instrumentation")]
        let mut profile = DeltaApplicationProfile::new();
        candidate.validate_consequence_delta(manifest.generation, &request.consequences)?;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("consequence_validation");
        let entry = candidate
            .consequence_history
            .last_mut()
            .filter(|entry| entry.generation == manifest.generation)
            .ok_or_else(|| {
                StoreError::Integrity(
                    "accepted generation is missing its consequence slot".to_owned(),
                )
            })?;
        entry.delta = request.consequences;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("consequence_candidate_ready");
        Ok((candidate, manifest))
    }
}

impl GraphStore for InMemoryGraphStore {
    fn current_generation(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationManifest>, StoreError> {
        if self.repository.is_some_and(|value| value != repository)
            || self.worktree.is_some_and(|value| value != worktree)
        {
            return Ok(None);
        }
        Ok(self.manifest.clone())
    }

    fn manifest(&self, generation: GenerationId) -> Result<GenerationManifest, StoreError> {
        self.history
            .iter()
            .find(|entry| entry.manifest.generation == generation)
            .map(|entry| entry.manifest.clone())
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(generation),
                actual: self.manifest.as_ref().map(|manifest| manifest.generation),
            })
    }

    fn generation_history(&self) -> Result<Vec<GenerationHistoryEntry>, StoreError> {
        Ok(self.history.iter().cloned().collect())
    }

    fn current_generation_entry(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationHistoryEntry>, StoreError> {
        if self.repository.is_some_and(|value| value != repository)
            || self.worktree.is_some_and(|value| value != worktree)
        {
            return Ok(None);
        }
        Ok(self.history.back().cloned())
    }

    fn generation_lineage_history(&self) -> Result<Vec<GenerationLineageEntry>, StoreError> {
        Ok(self.lineage_history.clone())
    }

    fn generation_consequence_history(
        &self,
    ) -> Result<Vec<GenerationConsequenceEntry>, StoreError> {
        Ok(self.consequence_history.clone())
    }

    fn apply_delta_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut profile = DeltaApplicationProfile::new();
        let candidate = self.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_candidate_clone");
        let (candidate, manifest) = candidate.stage_delta_with_lineage(request, accepted_at)?;
        *self = candidate;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_candidate_publish");
        Ok(manifest)
    }

    fn apply_delta_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut profile = DeltaApplicationProfile::new();
        let candidate = self.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_candidate_clone");
        let (candidate, manifest) =
            candidate.stage_delta_with_consequences(request, accepted_at)?;
        *self = candidate;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("lineage_candidate_publish");
        Ok(manifest)
    }

    fn events_for_change_set(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
        after: Option<EventsForChangeSetCursor>,
        limit: usize,
    ) -> Result<EventsForChangeSetPage, StoreError> {
        if limit == 0 {
            return Ok(EventsForChangeSetPage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        let as_of_sequence = self.generation_sequence(as_of)?;
        if let Some(cursor) = after
            && (cursor.change_set != change_set || cursor.as_of_generation != as_of)
        {
            return Err(StoreError::InvalidDelta(
                "change-set cursor is bound to a different set or snapshot".to_owned(),
            ));
        }
        let (_, memberships) = self.lineage_state_at(as_of)?;
        let history = self.history.iter().cloned().collect::<Vec<_>>();
        let events = crate::lineage::change_events_from_history(&history)?;
        let events_by_id = events
            .into_iter()
            .map(|event| (event.id, event))
            .collect::<BTreeMap<_, _>>();
        let mut items = memberships
            .into_iter()
            .filter(|(key, _)| key.change_set == change_set)
            .filter_map(|(key, (membership, valid_from))| {
                let event = events_by_id.get(&key.event)?.clone();
                let event_sequence = self.generation_sequence(event.generation_after).ok()?;
                (event_sequence <= as_of_sequence).then_some((
                    event_sequence,
                    ChangeSetEvent {
                        event,
                        membership,
                        membership_valid_from: valid_from,
                    },
                ))
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|(sequence, item)| (*sequence, item.event.id));
        if let Some(cursor) = after {
            let cursor_sequence = self.generation_sequence(cursor.after_event_generation)?;
            if cursor_sequence > as_of_sequence {
                return Err(StoreError::StaleBase {
                    expected: Some(as_of),
                    actual: Some(cursor.after_event_generation),
                });
            }
            items.retain(|(sequence, _)| *sequence > cursor_sequence);
        }
        let next_cursor = (items.len() > limit)
            .then(|| items.get(limit.saturating_sub(1)))
            .flatten()
            .map(|(_, item)| EventsForChangeSetCursor {
                change_set,
                as_of_generation: as_of,
                after_event_generation: item.event.generation_after,
            });
        items.truncate(limit);
        Ok(EventsForChangeSetPage {
            items: items.into_iter().map(|(_, item)| item).collect(),
            next_cursor,
        })
    }

    fn change_set_at(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
    ) -> Result<Option<ChangeSetVersion>, StoreError> {
        let position = self
            .history
            .iter()
            .position(|entry| entry.manifest.generation == as_of)
            .ok_or_else(|| StoreError::StaleBase {
                expected: Some(as_of),
                actual: self.history.last().map(|entry| entry.manifest.generation),
            })?;
        let version = self
            .lineage_history
            .iter()
            .take(position.saturating_add(1))
            .flat_map(|entry| {
                entry
                    .delta
                    .upsert_sets
                    .iter()
                    .filter(move |set| set.id == change_set)
                    .map(move |set| (set.clone(), entry.generation))
            })
            .last();
        let Some((declaration, valid_from)) = version else {
            return Ok(None);
        };
        let valid_until = self
            .lineage_history
            .iter()
            .skip(position.saturating_add(1))
            .find(|entry| {
                entry
                    .delta
                    .upsert_sets
                    .iter()
                    .any(|set| set.id == declaration.id)
            })
            .map(|entry| entry.generation);
        Ok(Some(ChangeSetVersion {
            change_set: declaration,
            valid_from,
            valid_until,
        }))
    }

    fn acceptance_time(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError> {
        if !self
            .history
            .iter()
            .any(|entry| entry.manifest.generation == generation)
        {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: self.manifest.as_ref().map(|manifest| manifest.generation),
            });
        }
        let Some(bytes) = self.records.get(&acceptance_record_key(generation)) else {
            return Ok(None);
        };
        let bytes: [u8; 8] = bytes.as_slice().try_into().map_err(|error| {
            StoreError::Integrity(format!(
                "stored generation acceptance time is not eight bytes: {error}"
            ))
        })?;
        Ok(Some(AcceptanceTime(u64::from_be_bytes(bytes))))
    }

    fn apply_delta_at(
        &mut self,
        delta: GraphDelta,
        accepted_at: AcceptanceTime,
    ) -> Result<GenerationManifest, StoreError> {
        self.apply_delta_with_acceptance_time(delta, Some(accepted_at))
    }

    fn apply_delta_with_acceptance_time(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        let manifest = apply_delta_to_memory(self, &delta, 2)?;
        if let Some(accepted_at) = accepted_at {
            self.records.insert(
                acceptance_record_key(manifest.generation),
                accepted_at.0.to_be_bytes().to_vec(),
            );
        }
        Ok(manifest)
    }

    fn apply_delta(&mut self, delta: GraphDelta) -> Result<GenerationManifest, StoreError> {
        apply_delta_to_memory(self, &delta, 2)
    }

    fn set_generation_status(
        &mut self,
        generation: GenerationId,
        status: GenerationStatus,
    ) -> Result<GenerationManifest, StoreError> {
        let mut manifest = self.manifest.clone().ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })?;
        if manifest.generation != generation {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: Some(manifest.generation),
            });
        }
        manifest.status = status;
        self.manifest = Some(manifest.clone());
        Ok(manifest)
    }

    fn node(&self, generation: GenerationId, id: NodeId) -> Result<Option<Node>, StoreError> {
        if self.manifest.as_ref().map(|item| item.generation) != Some(generation) {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: self.manifest.as_ref().map(|item| item.generation),
            });
        }
        Ok(self.nodes.get(&id).cloned())
    }

    fn nodes(&self, generation: GenerationId) -> Result<Vec<Node>, StoreError> {
        if self.manifest.as_ref().map(|item| item.generation) != Some(generation) {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: self.manifest.as_ref().map(|item| item.generation),
            });
        }
        Ok(self.nodes.values().cloned().collect())
    }

    fn symbol_candidates(
        &self,
        generation: GenerationId,
        exact_names: &[String],
        terminal_names: &[String],
    ) -> Result<Vec<Node>, StoreError> {
        self.manifest(generation)?;
        let ids = exact_names
            .iter()
            .filter_map(|name| self.nodes_by_name_index.get(name))
            .chain(
                terminal_names
                    .iter()
                    .filter_map(|name| self.nodes_by_terminal_index.get(name)),
            )
            .flat_map(|ids| ids.iter().copied())
            .collect::<BTreeSet<_>>();
        Ok(ids
            .into_iter()
            .filter_map(|id| self.nodes.get(&id).cloned())
            .collect())
    }

    fn files(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::FileVersion>, StoreError> {
        if self.manifest.as_ref().map(|item| item.generation) != Some(generation) {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: self.manifest.as_ref().map(|item| item.generation),
            });
        }
        Ok(self.files.values().cloned().collect())
    }

    fn edges(&self, generation: GenerationId) -> Result<Vec<Edge>, StoreError> {
        self.manifest(generation)?;
        Ok(self.edges.values().cloned().collect())
    }

    fn provenance(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::Provenance>, StoreError> {
        self.manifest(generation)?;
        Ok(self.provenance.values().cloned().collect())
    }

    fn outgoing(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        if self.manifest.as_ref().map(|item| item.generation) != Some(generation) {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: self.manifest.as_ref().map(|item| item.generation),
            });
        }
        Ok(self.outgoing_index.get(&id).map_or_else(Vec::new, |edges| {
            edges
                .iter()
                .filter_map(|edge_id| self.edges.get(edge_id).cloned())
                .collect()
        }))
    }

    fn incoming(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        self.manifest(generation)?;
        Ok(self.incoming_index.get(&id).map_or_else(Vec::new, |edges| {
            edges
                .iter()
                .filter_map(|edge_id| self.edges.get(edge_id).cloned())
                .collect()
        }))
    }

    fn nodes_for_file(
        &self,
        generation: GenerationId,
        file: syntaxmesh_core::FileId,
    ) -> Result<Vec<NodeId>, StoreError> {
        if self.manifest.as_ref().map(|item| item.generation) != Some(generation) {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: self.manifest.as_ref().map(|item| item.generation),
            });
        }
        Ok(self
            .nodes_by_file_index
            .get(&file)
            .map_or_else(Vec::new, |nodes| nodes.iter().copied().collect()))
    }
}
