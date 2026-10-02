//! SQLite persistence adapter sharing the canonical in-memory graph semantics.

mod connection;
mod file_pages;
mod index_store;
mod integrity;
mod migration_steps;
mod migrations;
mod record_store;
#[cfg(test)]
mod tests;
mod tree_store;

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;
#[cfg(feature = "benchmark-instrumentation")]
use std::time::Instant;

use connection::{
    ensure_sqlite_database_path_is_safe, ensure_wal_mode, prepare_sqlite_connection,
    sqlite_create_flags, sqlite_existing_flags, sqlite_read_only_flags,
};
use migrations::{SCHEMA_MIGRATIONS, SchemaMigration};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::de::DeserializeOwned;
use syntaxmesh_core::{
    AcceptanceTime, ChangeEvent, ChangeEventCursor, ChangeSetDelta, ChangeSetEvent, ChangeSetId,
    ConsequenceDelta, ConsequenceDerivation, ConsequenceEdge, ConsequenceEdgeCursor,
    ConsequenceEdgeVersion, ConsequenceRangeCursor, Edge, EdgeDirection, EdgeId,
    EventsForChangeSetCursor, FactPayload, FactRef, FileId, FileVersion,
    GenerationConsequenceEntry, GenerationHistoryEntry, GenerationId, GenerationLineageEntry,
    GenerationManifest, GenerationStatus, GraphDelta, GraphDeltaWithConsequences,
    GraphDeltaWithLineage, GraphSnapshot, LineageEndpoint, Node, NodeId, ObservationTime,
    ObservedFactCursor, Provenance, ProvenanceId, RepositoryId, StableId, WorktreeId,
};
use syntaxmesh_store::{
    AcceptedGeneration, AcceptedGenerationCursor, AcceptedGenerationPage, ChangeEventPage,
    ChangeSetVersion, ConsequenceEdgePage, ConsequenceRangePage, DurableRecordStore,
    EventsForChangeSetPage, FactHistoryCursor, FactHistoryEntry, FactHistoryPage,
    FactHistoryVersion, FactVersionChangeCursor, FactVersionChangePage, GenerationChange,
    GenerationChangeCursor, GenerationChangePage, GraphStore, HistoricalEdgePage,
    InMemoryGraphStore, MAX_FACT_VERSION_CHANGE_PAGE_SIZE, MAX_GENERATION_CHANGE_PAGE_SIZE,
    NodeHistoryVersion, ObservedFactPage, ObservedFactVersion, StoreError, generation_root_v2,
    node_kind_storage_code,
};

#[cfg(test)]
use record_store::prefix_upper_bound;
#[cfg(test)]
use syntaxmesh_store::{BackendIntegrityCheck, BackendIntegrityKind};
use tree_store::{
    delta_incidence_changes, delta_tree_mutations, publish_incidence_root, publish_persistent_root,
    publish_rebuilt_persistent_root, read_graph_at, read_parent_sequence, read_persistent_snapshot,
    read_temporal_snapshot, snapshot_incidence_changes, snapshot_root_for_schema,
    snapshot_tree_mutations,
};

const SCHEMA_VERSION: i64 = 21;
const CHECKPOINT_INTERVAL: i64 = 64;
const SCHEMA: &str = include_str!("../migrations/0000_initial_schema.sql");
const CONSEQUENCE_RANGE_QUERY: &str = "WITH bounds AS (SELECT first_generation.sequence AS from_sequence, last_generation.sequence AS until_sequence FROM syntaxmesh_generation_history AS first_generation JOIN syntaxmesh_generation_history AS last_generation ON 1 = 1 WHERE first_generation.generation = ?1 AND last_generation.generation = ?2) SELECT bounds.from_sequence, bounds.until_sequence, edges.payload, start.generation, finish.generation FROM bounds LEFT JOIN syntaxmesh_consequence_edge_versions AS edges ON bounds.from_sequence <= bounds.until_sequence AND edges.valid_from_sequence <= bounds.until_sequence AND (edges.valid_until_sequence IS NULL OR edges.valid_until_sequence > bounds.from_sequence) AND ((edges.source_kind = ?3 AND edges.source_id = ?4 AND edges.source_fact_kind IS ?5 AND edges.source_valid_from_generation IS ?6) OR (edges.target_kind = ?7 AND edges.target_id = ?8 AND edges.target_fact_kind IS ?9 AND edges.target_valid_from_generation IS ?10)) AND (?11 IS NULL OR edges.edge_id > ?11) LEFT JOIN syntaxmesh_generation_history AS start ON start.sequence = edges.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS finish ON finish.sequence = edges.valid_until_sequence ORDER BY edges.edge_id LIMIT ?12";
const FACT_HISTORY_FIRST_PAGE_QUERY: &str = "WITH bounds AS (SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1) SELECT versions.fact_kind, versions.fact_id, start_entry.payload, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN end_entry.payload ELSE NULL END, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos, versions.valid_from_sequence, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN versions.valid_until_sequence ELSE NULL END FROM bounds JOIN syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx ON versions.valid_from_sequence <= bounds.sequence JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation ORDER BY versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?2";
const FACT_HISTORY_AFTER_PAGE_QUERY: &str = "WITH bounds AS (SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1) SELECT versions.fact_kind, versions.fact_id, start_entry.payload, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN end_entry.payload ELSE NULL END, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos, versions.valid_from_sequence, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN versions.valid_until_sequence ELSE NULL END FROM bounds JOIN syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx ON versions.valid_from_sequence <= bounds.sequence AND (versions.fact_kind, versions.fact_id, versions.valid_from_sequence) > (?2, ?3, ?4) JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation ORDER BY versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?5";
const FACT_VERSION_CHANGE_PAGE_QUERY: &str = "SELECT versions.fact_kind, versions.fact_id, versions.valid_from_sequence, versions.valid_until_sequence, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM (SELECT * FROM (SELECT fact_kind, fact_id, valid_from_sequence, valid_until_sequence, payload, observed_at_unix_nanos FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_start_generation_idx WHERE valid_from_sequence = ?1 AND (?2 < 0 OR fact_kind > ?2 OR (fact_kind = ?2 AND fact_id > ?3) OR (fact_kind = ?2 AND fact_id = ?3 AND valid_from_sequence > ?4)) ORDER BY fact_kind, fact_id, valid_from_sequence LIMIT ?5) UNION ALL SELECT * FROM (SELECT fact_kind, fact_id, valid_from_sequence, valid_until_sequence, payload, observed_at_unix_nanos FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_end_generation_idx WHERE valid_until_sequence = ?1 AND valid_from_sequence <> ?1 AND (?2 < 0 OR fact_kind > ?2 OR (fact_kind = ?2 AND fact_id > ?3) OR (fact_kind = ?2 AND fact_id = ?3 AND valid_from_sequence > ?4)) ORDER BY fact_kind, fact_id, valid_from_sequence LIMIT ?5)) AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation ORDER BY versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?5";

const FILE_FACT: i64 = 0;
const PROVENANCE_FACT: i64 = 1;
const NODE_FACT: i64 = 2;
const EDGE_FACT: i64 = 3;
const INCIDENT_EDGE_NODE_BATCH_SIZE: usize = 128;

#[cfg(feature = "benchmark-instrumentation")]
struct TransactionProfile {
    enabled: bool,
    stage_started: Instant,
}

#[cfg(feature = "benchmark-instrumentation")]
#[derive(Default)]
struct TemporalFactProfile {
    enabled: bool,
    encode_elapsed: Duration,
    write_elapsed: Duration,
    close_elapsed: Duration,
}

#[cfg(feature = "benchmark-instrumentation")]
impl TemporalFactProfile {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("SYNTAXMESH_SQLITE_PROFILE").is_some(),
            ..Self::default()
        }
    }

    fn report(&self) {
        if !self.enabled {
            return;
        }
        for (stage, elapsed) in [
            ("temporal_fact_encode", self.encode_elapsed),
            ("temporal_sql_write", self.write_elapsed),
            ("temporal_sql_close", self.close_elapsed),
        ] {
            eprintln!(
                "sqlite_stage stage={stage} elapsed_us={}",
                elapsed.as_micros()
            );
        }
    }
}

#[cfg(feature = "benchmark-instrumentation")]
impl TransactionProfile {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("SYNTAXMESH_SQLITE_PROFILE").is_some(),
            stage_started: Instant::now(),
        }
    }

    fn mark(&mut self, stage: &str) {
        if self.enabled {
            eprintln!(
                "sqlite_stage stage={stage} elapsed_us={}",
                self.stage_started.elapsed().as_micros()
            );
            self.stage_started = Instant::now();
        }
    }
}

#[cfg(test)]
mod migration_test_sync {
    use std::{
        collections::{HashMap, HashSet},
        path::{Path, PathBuf},
        sync::{Arc, Barrier, LazyLock, Mutex},
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(super) enum Point {
        AfterSchemaProbe,
        AfterVersionRead,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(super) enum CommitBoundary {
        BeforeCommit,
        AfterCommit,
    }

    type BarrierMap = HashMap<(PathBuf, Point), Arc<Barrier>>;
    type FailureMap = HashSet<(PathBuf, i64, CommitBoundary)>;

    static BARRIERS: LazyLock<Mutex<BarrierMap>> = LazyLock::new(|| Mutex::new(HashMap::new()));
    static FAILURES: LazyLock<Mutex<FailureMap>> = LazyLock::new(|| Mutex::new(HashSet::new()));

    pub(super) struct Guard {
        path: PathBuf,
        point: Point,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            BARRIERS
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&(self.path.clone(), self.point));
        }
    }

    pub(super) fn arm(path: &Path, point: Point, barrier: Arc<Barrier>) -> Guard {
        let path = path.to_path_buf();
        BARRIERS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert((path.clone(), point), barrier);
        Guard { path, point }
    }

    pub(super) fn wait(path: &Path, point: Point) {
        let barrier = BARRIERS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&(path.to_path_buf(), point))
            .cloned();
        if let Some(barrier) = barrier {
            barrier.wait();
        }
    }

    pub(super) struct FailureGuard {
        path: PathBuf,
        version: i64,
        boundary: CommitBoundary,
    }

    impl Drop for FailureGuard {
        fn drop(&mut self) {
            FAILURES
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&(self.path.clone(), self.version, self.boundary));
        }
    }

    pub(super) fn arm_failure(path: &Path, version: i64, boundary: CommitBoundary) -> FailureGuard {
        let path = path.to_path_buf();
        FAILURES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert((path.clone(), version, boundary));
        FailureGuard {
            path,
            version,
            boundary,
        }
    }

    pub(super) fn hit_failure(path: &Path, version: i64, boundary: CommitBoundary) -> bool {
        FAILURES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&(path.to_path_buf(), version, boundary))
    }
}

/// SQLite implementation of the shared graph and durable-operation store ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationStatus {
    /// Database schema version, or `None` for a missing/empty database.
    pub current_version: Option<i64>,
    /// Latest version in the bundled registry.
    pub target_version: i64,
    /// Whether the persisted ordered migration ledger and SQL checksums were
    /// present and validated. Older schemas predate the ledger.
    pub migration_ledger_validated: bool,
    /// Registered migrations in order.
    pub migrations: Vec<SqliteMigrationStatusEntry>,
}

/// Status of one registered SQLite schema transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationStatusEntry {
    /// Target schema version for this transition.
    pub version: i64,
    /// Stable human-readable migration name.
    pub name: String,
    /// Whether this transition is included in the observed schema version.
    pub applied: bool,
}

pub struct SqliteGraphStore {
    connection: Connection,
    memory: InMemoryGraphStore,
}

impl SqliteGraphStore {
    fn require_current_generation(
        &self,
        generation: GenerationId,
    ) -> Result<GenerationManifest, StoreError> {
        let manifest = read_manifest(&self.connection)?;
        if manifest.as_ref().map(|value| value.generation) != Some(generation) {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: manifest.map(|value| value.generation),
            });
        }
        manifest.ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })
    }

    /// Explicitly bootstraps or upgrades a database to the current registered schema.
    ///
    /// # Errors
    /// Returns a backend error when migration fails or a registered schema is invalid.
    pub fn migrate(path: impl AsRef<Path>) -> Result<(), StoreError> {
        let path = path.as_ref();
        ensure_sqlite_database_path_is_safe(path)?;
        let mut connection =
            Connection::open_with_flags(path, sqlite_create_flags()).map_err(sql_error)?;
        prepare_sqlite_connection(&connection)?;
        initialize_or_migrate(&mut connection, path)?;
        ensure_wal_mode(&connection)
    }

    /// Inspects the registered SQLite migrations without modifying the database.
    ///
    /// A missing or empty file is reported as an uninitialized database with
    /// every registered migration pending. Existing application databases with
    /// an unknown or incompatible schema fail closed.
    ///
    /// # Errors
    /// Returns a backend error when the database cannot be read or its schema
    /// version is unsupported, and an integrity error when its migration
    /// ledger/checksums disagree with the registry.
    pub fn migration_status(path: impl AsRef<Path>) -> Result<SqliteMigrationStatus, StoreError> {
        let path = path.as_ref();
        ensure_sqlite_database_path_is_safe(path)?;
        match std::fs::metadata(path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return migration_status_report(None, false);
            }
            Err(error) => {
                return Err(StoreError::Backend(format!(
                    "inspect SQLite migration status path {}: {error}",
                    path.display()
                )));
            }
        }

        let connection =
            Connection::open_with_flags(path, sqlite_read_only_flags()).map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_secs(30))
            .map_err(sql_error)?;
        if !sqlite_table_exists(&connection, "syntaxmesh_schema")? {
            let has_application_tables = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?;
            if has_application_tables {
                return Err(StoreError::Backend(
                    "SQLite database has application tables but no SyntaxMesh schema version"
                        .to_owned(),
                ));
            }
            return migration_status_report(None, false);
        }

        let version = read_schema_version(&connection)?.ok_or_else(|| {
            StoreError::Backend("SQLite schema-version table has no singleton row".to_owned())
        })?;
        if !(1..=SCHEMA_VERSION).contains(&version) {
            return Err(StoreError::Backend(format!(
                "unsupported SQLite schema version {version}; registered range is 1..={SCHEMA_VERSION}"
            )));
        }
        let has_ledger = sqlite_table_exists(&connection, "syntaxmesh_schema_migrations")?;
        if version >= 10 && !has_ledger {
            return Err(StoreError::Integrity(
                "SQLite migration ledger is missing for a schema that requires it".to_owned(),
            ));
        }
        if has_ledger {
            validate_schema_migrations(&connection, version)?;
        }
        migration_status_report(Some(version), has_ledger)
    }

    /// Opens an existing current-schema database without applying migrations or repairs.
    ///
    /// Run [`Self::migrate`] explicitly before opening a new or stale database.
    ///
    /// # Errors
    /// Returns a backend error for an absent/stale/incompatible schema and an
    /// integrity error for an invalid snapshot or derived projection.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        ensure_sqlite_database_path_is_safe(path)?;
        let connection =
            Connection::open_with_flags(path, sqlite_existing_flags()).map_err(sql_error)?;
        prepare_sqlite_connection(&connection)?;
        validate_current_schema(&connection)?;
        let memory = restore_store(&connection)?;
        validate_derived_projections(&connection, &memory)?;
        Ok(Self { connection, memory })
    }

    /// Returns the latest generation restored or published by this handle.
    #[must_use]
    pub fn latest_generation(&self) -> Option<GenerationManifest> {
        self.memory.current_manifest()
    }

    fn persist(
        &mut self,
        memory: &InMemoryGraphStore,
        expected_database_manifest: Option<GenerationManifest>,
        delta: Option<&GraphDelta>,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<(), StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut profile = TransactionProfile::new();
        let manifest = memory
            .current_manifest()
            .ok_or_else(|| StoreError::Integrity("cannot persist an empty store".to_owned()))?;
        let change_event = delta
            .map(|delta| self.memory.change_event_for_delta(delta))
            .transpose()?;
        let mut cascaded_edge_ids = BTreeSet::new();
        if let Some(delta) = delta
            && let Some(parent) = delta.expected_base
        {
            for node in &delta.remove_nodes {
                cascaded_edge_ids.extend(
                    self.memory
                        .outgoing(parent, *node)?
                        .into_iter()
                        .map(|edge| edge.id),
                );
                cascaded_edge_ids.extend(
                    self.memory
                        .incoming(parent, *node)?
                        .into_iter()
                        .map(|edge| edge.id),
                );
            }
        }
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("incident_edge_cascade");
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let actual_database_manifest = read_manifest(&transaction)?;
        if actual_database_manifest != expected_database_manifest {
            return Err(StoreError::StaleBase {
                expected: expected_database_manifest.map(|value| value.generation),
                actual: actual_database_manifest.map(|value| value.generation),
            });
        }
        let history_sequence = if delta.is_some() {
            Some(next_history_sequence(&transaction)?)
        } else {
            None
        };
        if let (Some(delta), Some(sequence)) = (delta, history_sequence) {
            let parent_sequence = delta
                .expected_base
                .map(|parent| read_parent_sequence(&transaction, parent))
                .transpose()?;
            let rebuild_root = expected_database_manifest
                .as_ref()
                .is_none_or(|parent| parent.schema_version < 2);
            let (tree_mutations, incidence_changes) = if rebuild_root {
                let snapshot = GraphSnapshot {
                    files: memory.files(manifest.generation)?,
                    provenance: memory.provenance(manifest.generation)?,
                    nodes: memory.nodes(manifest.generation)?,
                    edges: memory.edges(manifest.generation)?,
                };
                (
                    snapshot_tree_mutations(&snapshot, 2)?,
                    snapshot_incidence_changes(&snapshot),
                )
            } else {
                (
                    delta_tree_mutations(
                        &transaction,
                        delta,
                        parent_sequence,
                        2,
                        Some(&cascaded_edge_ids),
                    )?,
                    delta_incidence_changes(&transaction, delta, &cascaded_edge_ids)?,
                )
            };
            #[cfg(feature = "benchmark-instrumentation")]
            profile.mark("begin_validate_and_build_delta_mutations");
            apply_temporal_delta(&transaction, delta, sequence, Some(&cascaded_edge_ids))?;
            #[cfg(feature = "benchmark-instrumentation")]
            profile.mark("temporal_fact_versions");
            let root = if rebuild_root {
                publish_rebuilt_persistent_root(
                    &transaction,
                    sequence,
                    delta.next_generation,
                    &tree_mutations,
                )?
            } else {
                publish_persistent_root(
                    &transaction,
                    sequence,
                    delta.next_generation,
                    delta.expected_base,
                    &tree_mutations,
                )?
            };
            let incidence_parent = if rebuild_root {
                None
            } else {
                delta.expected_base
            };
            publish_incidence_root(
                &transaction,
                sequence,
                delta.next_generation,
                incidence_parent,
                &incidence_changes,
            )?;
            #[cfg(feature = "benchmark-instrumentation")]
            profile.mark("persistent_fact_root");
            if generation_root_v2(delta.next_generation.0, root) != manifest.graph_root {
                return Err(StoreError::Integrity(
                    "incremental persistent root differs from the candidate manifest".to_owned(),
                ));
            }
        }
        transaction
            .execute(
                "INSERT INTO syntaxmesh_manifest (id, payload) VALUES (1, ?1) \
                 ON CONFLICT(id) DO UPDATE SET payload = excluded.payload",
                [encode(&manifest)?],
            )
            .map_err(sql_error)?;
        if let Some(delta) = delta {
            apply_current_projection(&transaction, delta, &cascaded_edge_ids)?;
        }
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("manifest_and_current_projection");
        if let Some(sequence) = history_sequence {
            let entry = memory.generation_history()?.pop().ok_or_else(|| {
                StoreError::Integrity("published delta has no history entry".to_owned())
            })?;
            if entry.manifest.generation != manifest.generation {
                return Err(StoreError::Integrity(
                    "latest history entry differs from persisted manifest".to_owned(),
                ));
            }
            transaction
                .execute(
                    "INSERT INTO syntaxmesh_generation_history (sequence, generation, payload) VALUES (?1, ?2, ?3)",
                    params![sequence, entry.manifest.generation.0.0.as_slice(), encode(&entry)?],
                )
                .map_err(sql_error)?;
            let lineage_entry = memory
                .generation_lineage_history()?
                .into_iter()
                .find(|lineage| lineage.generation == entry.manifest.generation)
                .unwrap_or_else(|| GenerationLineageEntry {
                    generation: entry.manifest.generation,
                    delta: ChangeSetDelta::default(),
                });
            transaction
                .execute(
                    "INSERT INTO syntaxmesh_generation_lineage (sequence, generation, payload) VALUES (?1, ?2, ?3)",
                    params![sequence, entry.manifest.generation.0.0.as_slice(), encode(&lineage_entry)?],
                )
                .map_err(sql_error)?;
            apply_change_set_lineage_projection(&transaction, &lineage_entry.delta, sequence)?;
            let consequence_entry = memory
                .generation_consequence_history()?
                .into_iter()
                .find(|consequence| consequence.generation == entry.manifest.generation)
                .unwrap_or_else(|| GenerationConsequenceEntry {
                    generation: entry.manifest.generation,
                    delta: ConsequenceDelta::default(),
                });
            transaction
                .execute(
                    "INSERT INTO syntaxmesh_generation_consequences (sequence, generation, payload) VALUES (?1, ?2, ?3)",
                    params![sequence, consequence_entry.generation.0.0.as_slice(), encode(&consequence_entry)?],
                )
                .map_err(sql_error)?;
            apply_consequence_projection(&transaction, &consequence_entry.delta, sequence)?;
            if let Some(event) = &change_event {
                insert_change_event(&transaction, event)?;
            }
            if let Some(accepted_at) = accepted_at {
                let accepted_through = if let Some(parent) =
                    delta.and_then(|delta| delta.expected_base)
                {
                    let previous = transaction.query_row(
                        "SELECT accepted_through_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
                        [parent.0.0.as_slice()],
                        |row| row.get::<_, Option<Vec<u8>>>(0),
                    ).optional().map_err(sql_error)?.flatten();
                    previous
                        .map(|bytes| {
                            let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                                StoreError::Integrity(format!(
                                    "accepted-through watermark has {} bytes",
                                    bytes.len()
                                ))
                            })?;
                            Ok::<_, StoreError>(u64::from_be_bytes(bytes).max(accepted_at.0))
                        })
                        .transpose()?
                } else {
                    Some(accepted_at.0)
                };
                transaction
                    .execute(
                        "INSERT INTO syntaxmesh_generation_acceptance (generation, accepted_at_unix_nanos, accepted_through_unix_nanos) VALUES (?1, ?2, ?3)",
                        params![entry.manifest.generation.0.0.as_slice(), accepted_at.0.to_be_bytes().as_slice(), accepted_through.map(|value| value.to_be_bytes().to_vec())],
                    )
                    .map_err(sql_error)?;
            }
        }
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("history_lineage_consequence_and_acceptance");
        if let Some(sequence) = history_sequence
            && (sequence == 1 || sequence % CHECKPOINT_INTERVAL == 0)
        {
            let files = memory.files(manifest.generation)?;
            let provenance = memory.provenance(manifest.generation)?;
            let nodes = memory.nodes(manifest.generation)?;
            let edges = memory.edges(manifest.generation)?;
            let snapshot = GraphSnapshot {
                files,
                provenance,
                nodes,
                edges,
            };
            transaction.execute(
                "INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (?1, ?2, ?3, ?4)",
                params![sequence, manifest.generation.0.0.as_slice(), encode(&manifest)?, encode(&snapshot)?],
            ).map_err(sql_error)?;
        }
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("checkpoint_writes");
        transaction.commit().map_err(sql_error)?;
        #[cfg(feature = "benchmark-instrumentation")]
        profile.mark("transaction_commit");
        Ok(())
    }
}

fn next_history_sequence(connection: &Connection) -> Result<i64, StoreError> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM syntaxmesh_generation_history",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn read_generation_history_rows(
    connection: &Connection,
) -> Result<Vec<(i64, GenerationHistoryEntry)>, StoreError> {
    let mut statement = connection
        .prepare("SELECT sequence, payload FROM syntaxmesh_generation_history ORDER BY sequence")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(sql_error)?;
    rows.map(|row| {
        let (sequence, payload) = row.map_err(sql_error)?;
        Ok((sequence, decode(&payload)?))
    })
    .collect()
}

fn read_generation_history_entry(
    connection: &Connection,
    generation: GenerationId,
) -> Result<Option<GenerationHistoryEntry>, StoreError> {
    connection
        .query_row(
            "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|payload| decode(&payload))
        .transpose()
}

fn read_canonical_snapshot(connection: &Connection) -> Result<GraphSnapshot, StoreError> {
    Ok(GraphSnapshot {
        files: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_files ORDER BY id",
        )?,
        provenance: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_provenance ORDER BY id",
        )?,
        nodes: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_nodes ORDER BY id",
        )?,
        edges: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_edges ORDER BY id",
        )?,
    })
}

#[derive(Clone, Copy, Default)]
struct TemporalFactMetadata<'metadata> {
    observed_at: Option<&'metadata [u8]>,
    source: Option<&'metadata [u8]>,
    target: Option<&'metadata [u8]>,
}

struct TemporalFactStatements<'connection> {
    close_fact: rusqlite::Statement<'connection>,
    close_edge_batch: rusqlite::Statement<'connection>,
    close_source_edges: rusqlite::Statement<'connection>,
    close_target_edges: rusqlite::Statement<'connection>,
    insert_fact: rusqlite::Statement<'connection>,
    #[cfg(feature = "benchmark-instrumentation")]
    profile: TemporalFactProfile,
}

impl<'connection> TemporalFactStatements<'connection> {
    fn new(connection: &'connection Connection) -> Result<Self, StoreError> {
        Ok(Self {
            close_fact: connection
                .prepare(
                    "UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 WHERE fact_kind = ?2 AND fact_id = ?3 AND valid_until_sequence IS NULL",
                )
                .map_err(sql_error)?,
            close_edge_batch: connection
                .prepare(&edge_id_batch_close_sql())
                .map_err(sql_error)?,
            close_source_edges: connection
                .prepare(&incident_edge_close_sql("source_id"))
                .map_err(sql_error)?,
            close_target_edges: connection
                .prepare(&incident_edge_close_sql("target_id"))
                .map_err(sql_error)?,
            insert_fact: connection
                .prepare(
                    "INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, valid_until_sequence, observed_at_unix_nanos, source_id, target_id, payload) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7)",
                )
                .map_err(sql_error)?,
            #[cfg(feature = "benchmark-instrumentation")]
            profile: TemporalFactProfile::new(),
        })
    }

    fn close(&mut self, kind: i64, id: &[u8], sequence: i64) -> Result<(), StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let started = self.profile.enabled.then(Instant::now);
        self.close_fact
            .execute(params![sequence, kind, id])
            .map_err(sql_error)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = started {
            self.profile.close_elapsed =
                self.profile.close_elapsed.saturating_add(started.elapsed());
        }
        Ok(())
    }

    fn close_incident_edges(
        &mut self,
        node_ids: &[&[u8]],
        sequence: i64,
    ) -> Result<(), StoreError> {
        let mut parameters = Vec::with_capacity(INCIDENT_EDGE_NODE_BATCH_SIZE + 2);
        parameters.push(rusqlite::types::Value::Integer(sequence));
        parameters.push(rusqlite::types::Value::Integer(EDGE_FACT));
        parameters.extend(
            node_ids
                .iter()
                .map(|id| rusqlite::types::Value::Blob(id.to_vec())),
        );
        parameters.resize(
            INCIDENT_EDGE_NODE_BATCH_SIZE + 2,
            rusqlite::types::Value::Null,
        );
        #[cfg(feature = "benchmark-instrumentation")]
        let started = self.profile.enabled.then(Instant::now);
        self.close_source_edges
            .execute(rusqlite::params_from_iter(parameters.iter()))
            .map_err(sql_error)?;
        self.close_target_edges
            .execute(rusqlite::params_from_iter(parameters.iter()))
            .map_err(sql_error)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = started {
            self.profile.close_elapsed =
                self.profile.close_elapsed.saturating_add(started.elapsed());
        }
        Ok(())
    }

    fn close_edge_ids(&mut self, edge_ids: &[EdgeId], sequence: i64) -> Result<(), StoreError> {
        for batch in edge_ids.chunks(INCIDENT_EDGE_NODE_BATCH_SIZE) {
            let mut parameters = Vec::with_capacity(INCIDENT_EDGE_NODE_BATCH_SIZE + 2);
            parameters.push(rusqlite::types::Value::Integer(sequence));
            parameters.push(rusqlite::types::Value::Integer(EDGE_FACT));
            parameters.extend(
                batch
                    .iter()
                    .map(|id| rusqlite::types::Value::Blob(id.0.0.to_vec())),
            );
            parameters.resize(
                INCIDENT_EDGE_NODE_BATCH_SIZE + 2,
                rusqlite::types::Value::Null,
            );
            #[cfg(feature = "benchmark-instrumentation")]
            let started = self.profile.enabled.then(Instant::now);
            self.close_edge_batch
                .execute(rusqlite::params_from_iter(parameters.iter()))
                .map_err(sql_error)?;
            #[cfg(feature = "benchmark-instrumentation")]
            if let Some(started) = started {
                self.profile.close_elapsed =
                    self.profile.close_elapsed.saturating_add(started.elapsed());
            }
        }
        Ok(())
    }

    fn write_fact<T: serde::Serialize>(
        &mut self,
        kind: i64,
        id: &[u8],
        fact: &T,
        sequence: i64,
        close_existing: bool,
        metadata: TemporalFactMetadata<'_>,
    ) -> Result<(), StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let started = self.profile.enabled.then(Instant::now);
        let payload = encode(fact)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = started {
            self.profile.encode_elapsed = self
                .profile
                .encode_elapsed
                .saturating_add(started.elapsed());
        }
        self.write(kind, id, &payload, sequence, close_existing, metadata)
    }

    fn write(
        &mut self,
        kind: i64,
        id: &[u8],
        payload: &[u8],
        sequence: i64,
        close_existing: bool,
        metadata: TemporalFactMetadata<'_>,
    ) -> Result<(), StoreError> {
        if close_existing {
            self.close(kind, id, sequence)?;
        }
        #[cfg(feature = "benchmark-instrumentation")]
        let started = self.profile.enabled.then(Instant::now);
        self.insert_fact
            .execute(params![
                kind,
                id,
                sequence,
                metadata.observed_at,
                metadata.source,
                metadata.target,
                payload
            ])
            .map_err(sql_error)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = started {
            self.profile.write_elapsed =
                self.profile.write_elapsed.saturating_add(started.elapsed());
        }
        Ok(())
    }

    #[cfg(feature = "benchmark-instrumentation")]
    fn report_profile(&self) {
        self.profile.report();
    }
}

fn incident_edge_close_sql(endpoint_column: &str) -> String {
    let parameters = std::iter::repeat_n("?", INCIDENT_EDGE_NODE_BATCH_SIZE)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 \
         WHERE fact_kind = ?2 AND valid_until_sequence IS NULL \
         AND {endpoint_column} IN ({parameters})"
    )
}

fn edge_id_batch_close_sql() -> String {
    let parameters = std::iter::repeat_n("?", INCIDENT_EDGE_NODE_BATCH_SIZE)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 \
         WHERE fact_kind = ?2 AND valid_until_sequence IS NULL AND fact_id IN ({parameters})"
    )
}

fn runtime_observed_at(node: &Node) -> Option<Vec<u8>> {
    let payload = node.extension_payload.as_ref()?;
    let value: serde_json::Value = serde_json::from_slice(&payload.bytes).ok()?;
    Some(
        value
            .get("observed_at_unix_nanos")?
            .as_u64()?
            .to_be_bytes()
            .to_vec(),
    )
}

fn seed_temporal_snapshot(
    connection: &Connection,
    snapshot: &GraphSnapshot,
    sequence: i64,
) -> Result<(), StoreError> {
    let mut statements = TemporalFactStatements::new(connection)?;
    for file in &snapshot.files {
        statements.write(
            FILE_FACT,
            &file.file_id.0.0,
            &encode(file)?,
            sequence,
            true,
            TemporalFactMetadata::default(),
        )?;
    }
    for item in &snapshot.provenance {
        statements.write(
            PROVENANCE_FACT,
            &item.id.0.0,
            &encode(item)?,
            sequence,
            true,
            TemporalFactMetadata::default(),
        )?;
    }
    for node in &snapshot.nodes {
        let observed = runtime_observed_at(node);
        statements.write(
            NODE_FACT,
            &node.id.0.0,
            &encode(node)?,
            sequence,
            true,
            TemporalFactMetadata {
                observed_at: observed.as_deref(),
                ..TemporalFactMetadata::default()
            },
        )?;
    }
    for edge in &snapshot.edges {
        statements.write(
            EDGE_FACT,
            &edge.id.0.0,
            &encode(edge)?,
            sequence,
            true,
            TemporalFactMetadata {
                source: Some(&edge.source.0.0),
                target: Some(&edge.target.0.0),
                ..TemporalFactMetadata::default()
            },
        )?;
    }
    Ok(())
}

fn apply_temporal_delta(
    connection: &Connection,
    delta: &GraphDelta,
    sequence: i64,
    cascaded_edge_ids: Option<&BTreeSet<EdgeId>>,
) -> Result<(), StoreError> {
    let close_existing = delta.expected_base.is_some();
    let mut statements = TemporalFactStatements::new(connection)?;
    if let Some(edge_ids) = cascaded_edge_ids {
        let mut edges_to_close = edge_ids.clone();
        edges_to_close.extend(delta.remove_edges.iter().copied());
        statements.close_edge_ids(&edges_to_close.into_iter().collect::<Vec<_>>(), sequence)?;
    } else {
        for node_batch in delta.remove_nodes.chunks(INCIDENT_EDGE_NODE_BATCH_SIZE) {
            let node_ids = node_batch
                .iter()
                .map(|node_id| node_id.0.0.as_slice())
                .collect::<Vec<_>>();
            statements.close_incident_edges(&node_ids, sequence)?;
        }
    }
    for file_id in &delta.removed_files {
        statements.close(FILE_FACT, &file_id.0.0, sequence)?;
    }
    if cascaded_edge_ids.is_none() {
        for edge_id in &delta.remove_edges {
            statements.close(EDGE_FACT, &edge_id.0.0, sequence)?;
        }
    }
    for node_id in &delta.remove_nodes {
        statements.close(NODE_FACT, &node_id.0.0, sequence)?;
    }
    for file in &delta.changed_files {
        statements.write_fact(
            FILE_FACT,
            &file.file_id.0.0,
            file,
            sequence,
            close_existing,
            TemporalFactMetadata::default(),
        )?;
    }
    for item in &delta.upsert_provenance {
        statements.write_fact(
            PROVENANCE_FACT,
            &item.id.0.0,
            item,
            sequence,
            close_existing,
            TemporalFactMetadata::default(),
        )?;
    }
    for node in &delta.upsert_nodes {
        let observed = runtime_observed_at(node);
        statements.write_fact(
            NODE_FACT,
            &node.id.0.0,
            node,
            sequence,
            close_existing,
            TemporalFactMetadata {
                observed_at: observed.as_deref(),
                ..TemporalFactMetadata::default()
            },
        )?;
    }
    for edge in &delta.upsert_edges {
        statements.write_fact(
            EDGE_FACT,
            &edge.id.0.0,
            edge,
            sequence,
            close_existing,
            TemporalFactMetadata {
                source: Some(&edge.source.0.0),
                target: Some(&edge.target.0.0),
                ..TemporalFactMetadata::default()
            },
        )?;
    }
    #[cfg(feature = "benchmark-instrumentation")]
    statements.report_profile();
    Ok(())
}

struct CurrentProjectionStatements<'connection> {
    upsert_file: rusqlite::Statement<'connection>,
    delete_file: rusqlite::Statement<'connection>,
    upsert_provenance: rusqlite::Statement<'connection>,
    delete_edge: rusqlite::Statement<'connection>,
    delete_node: rusqlite::Statement<'connection>,
    upsert_node: rusqlite::Statement<'connection>,
    upsert_edge: rusqlite::Statement<'connection>,
}

impl<'connection> CurrentProjectionStatements<'connection> {
    fn new(connection: &'connection Connection) -> Result<Self, StoreError> {
        Ok(Self {
            upsert_file: connection
                .prepare(
                    "INSERT INTO syntaxmesh_files (id, payload) VALUES (?1, ?2) \
                     ON CONFLICT(id) DO UPDATE SET payload = excluded.payload",
                )
                .map_err(sql_error)?,
            delete_file: connection
                .prepare("DELETE FROM syntaxmesh_files WHERE id = ?1")
                .map_err(sql_error)?,
            upsert_provenance: connection
                .prepare(
                    "INSERT INTO syntaxmesh_provenance (id, payload) VALUES (?1, ?2) \
                     ON CONFLICT(id) DO UPDATE SET payload = excluded.payload",
                )
                .map_err(sql_error)?,
            delete_edge: connection
                .prepare("DELETE FROM syntaxmesh_edges WHERE id = ?1")
                .map_err(sql_error)?,
            delete_node: connection
                .prepare("DELETE FROM syntaxmesh_nodes WHERE id = ?1")
                .map_err(sql_error)?,
            upsert_node: connection
                .prepare(
                    "INSERT INTO syntaxmesh_nodes (id, payload, node_kind) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, node_kind = excluded.node_kind",
                )
                .map_err(sql_error)?,
            upsert_edge: connection
                .prepare(
                    "INSERT INTO syntaxmesh_edges (id, payload) VALUES (?1, ?2) \
                     ON CONFLICT(id) DO UPDATE SET payload = excluded.payload",
                )
                .map_err(sql_error)?,
        })
    }
}

fn apply_current_projection(
    connection: &Connection,
    delta: &GraphDelta,
    cascaded_edge_ids: &BTreeSet<EdgeId>,
) -> Result<(), StoreError> {
    let mut statements = CurrentProjectionStatements::new(connection)?;
    for file in &delta.changed_files {
        statements
            .upsert_file
            .execute(params![file.file_id.0.0.as_slice(), encode(file)?])
            .map_err(sql_error)?;
    }
    for file_id in &delta.removed_files {
        statements
            .delete_file
            .execute([file_id.0.0.as_slice()])
            .map_err(sql_error)?;
    }
    for item in &delta.upsert_provenance {
        statements
            .upsert_provenance
            .execute(params![item.id.0.0.as_slice(), encode(item)?])
            .map_err(sql_error)?;
    }
    for edge_id in delta.remove_edges.iter().chain(cascaded_edge_ids) {
        statements
            .delete_edge
            .execute([edge_id.0.0.as_slice()])
            .map_err(sql_error)?;
    }
    for node_id in &delta.remove_nodes {
        statements
            .delete_node
            .execute([node_id.0.0.as_slice()])
            .map_err(sql_error)?;
    }
    for node in &delta.upsert_nodes {
        statements
            .upsert_node
            .execute(params![
                node.id.0.0.as_slice(),
                encode(node)?,
                node_kind_storage_code(&node.kind),
            ])
            .map_err(sql_error)?;
    }
    for edge in &delta.upsert_edges {
        statements
            .upsert_edge
            .execute(params![edge.id.0.0.as_slice(), encode(edge)?])
            .map_err(sql_error)?;
    }
    Ok(())
}

fn initialize_or_migrate(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let schema_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'syntaxmesh_schema')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
    #[cfg(test)]
    migration_test_sync::wait(path, migration_test_sync::Point::AfterSchemaProbe);
    if !schema_exists {
        let has_application_tables = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
        if has_application_tables {
            return Err(StoreError::Backend(
                "SQLite database has application tables but no SyntaxMesh schema version; refusing implicit initialization".to_owned(),
            ));
        }
        match initialize_fresh_schema(connection) {
            Ok(()) => return Ok(()),
            Err(error) => {
                if read_schema_version(connection).ok().flatten() == Some(SCHEMA_VERSION) {
                    return validate_schema_migrations(connection, SCHEMA_VERSION);
                }
                return Err(error);
            }
        }
    }

    loop {
        let version = read_schema_version(connection)?.ok_or_else(|| {
            StoreError::Backend("SQLite schema-version table has no singleton row".to_owned())
        })?;
        #[cfg(test)]
        migration_test_sync::wait(path, migration_test_sync::Point::AfterVersionRead);
        if version == SCHEMA_VERSION {
            return validate_schema_migrations(connection, version);
        }
        let Some(migration) = SCHEMA_MIGRATIONS.iter().find(|item| item.from == version) else {
            return Err(StoreError::Backend(format!(
                "unsupported SQLite schema version {version}; no registered next migration"
            )));
        };
        if let Err(error) = (migration.apply)(connection, path) {
            if read_schema_version(connection)
                .is_ok_and(|observed| observed.is_some_and(|observed| observed > version))
            {
                continue;
            }
            return Err(StoreError::Backend(format!(
                "SQLite migration {} ({} -> {}) failed for {}: {error}",
                migration.name,
                migration.from,
                migration.to,
                path.display()
            )));
        }
    }
}

fn initialize_fresh_schema(connection: &mut Connection) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction.execute_batch(SCHEMA).map_err(sql_error)?;
    if SCHEMA_MIGRATIONS.last().map(|migration| migration.to) != Some(SCHEMA_VERSION) {
        return Err(StoreError::Integrity(
            "SQLite fresh-schema version disagrees with migration registry".to_owned(),
        ));
    }
    // This is a direct bootstrap, not a replay of the historical transition
    // functions. Its SQL overlays are nevertheless declared on the ordered
    // migration registry so a schema change cannot silently bypass the
    // bootstrap path.
    for migration in SCHEMA_MIGRATIONS {
        if let Some(sql) = migration.fresh_bootstrap_sql {
            transaction.execute_batch(sql).map_err(sql_error)?;
        }
    }
    transaction
        .execute(
            "INSERT INTO syntaxmesh_schema (id, version) VALUES (1, ?1)",
            [SCHEMA_VERSION],
        )
        .map_err(sql_error)?;
    for migration in SCHEMA_MIGRATIONS {
        transaction
            .execute(
                "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, NULL, 1, ?3)",
                params![migration.to, migration.name, migration_checksum(migration)],
            )
            .map_err(sql_error)?;
    }
    transaction.commit().map_err(sql_error)
}

fn validate_current_schema(connection: &Connection) -> Result<(), StoreError> {
    let version = read_schema_version(connection)?.ok_or_else(|| {
        StoreError::Backend(
            "SQLite schema is missing; run SqliteGraphStore::migrate before open".to_owned(),
        )
    })?;
    if version != SCHEMA_VERSION {
        return Err(StoreError::Backend(format!(
            "SQLite schema version {version} is not current (expected {SCHEMA_VERSION}); run SqliteGraphStore::migrate before open"
        )));
    }
    validate_schema_migrations(connection, version)?;

    let required_indexes = [
        "syntaxmesh_fact_versions_identity_time_idx",
        "syntaxmesh_fact_versions_time_idx",
        "syntaxmesh_fact_versions_observed_time_idx",
        "syntaxmesh_generation_acceptance_time_idx",
        "syntaxmesh_change_event_fact_lookup_idx",
        "syntaxmesh_change_events_generation_idx",
        "syntaxmesh_change_set_validity_idx",
        "syntaxmesh_change_set_membership_set_idx",
        "syntaxmesh_change_set_membership_event_idx",
        "syntaxmesh_consequence_source_idx",
        "syntaxmesh_consequence_target_idx",
        "syntaxmesh_consequence_evidence_idx",
        "syntaxmesh_consequence_parent_idx",
        "syntaxmesh_nodes_kind_idx",
    ];
    for index in required_indexes {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1)",
                [index],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Err(StoreError::Integrity(format!(
                "SQLite current schema is missing required index {index}; run SqliteGraphStore::migrate"
            )));
        }
    }

    let acceptance_prefix_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('syntaxmesh_generation_acceptance') WHERE name = 'accepted_through_unix_nanos')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
    if !acceptance_prefix_exists {
        return Err(StoreError::Backend(
            "SQLite schema is missing syntaxmesh_generation_acceptance.accepted_through_unix_nanos"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_derived_projections(
    connection: &Connection,
    memory: &InMemoryGraphStore,
) -> Result<(), StoreError> {
    let mut statement = connection
        .prepare("SELECT id, payload, node_kind FROM syntaxmesh_nodes ORDER BY id")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        let (id, payload, actual_kind) = row.map_err(sql_error)?;
        let node: Node = decode(&payload)?;
        if id != node.id.0.0 || actual_kind != node_kind_storage_code(&node.kind) {
            return Err(StoreError::Integrity(
                "SQLite node-kind projection disagrees with canonical payload".to_owned(),
            ));
        }
    }

    let history_count = connection
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_generation_history",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    if memory.current_manifest().is_some() && history_count == 0 {
        return Err(StoreError::Integrity(
            "SQLite accepted graph is missing its generation-history anchor; run SqliteGraphStore::migrate"
                .to_owned(),
        ));
    }

    let expected_events = i64::try_from(
        memory
            .generation_history()?
            .iter()
            .filter(|entry| entry.delta.is_some())
            .count(),
    )
    .map_err(|error| StoreError::Backend(format!("change-event count is too large: {error}")))?;
    let actual_events = connection
        .query_row("SELECT COUNT(*) FROM syntaxmesh_change_events", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(sql_error)?;
    if actual_events != expected_events {
        return Err(StoreError::Integrity(format!(
            "SQLite change-event projection contains {actual_events} entries; expected {expected_events}; run SqliteGraphStore::migrate"
        )));
    }
    let expected_lineage = history_count;
    let actual_lineage = connection
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_generation_lineage",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    if actual_lineage != expected_lineage {
        return Err(StoreError::Integrity(format!(
            "SQLite generation-lineage journal contains {actual_lineage} entries; expected {expected_lineage}; run SqliteGraphStore::migrate"
        )));
    }
    let actual_consequences = connection
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_generation_consequences",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    if actual_consequences != history_count {
        return Err(StoreError::Integrity(format!(
            "SQLite consequence journal contains {actual_consequences} entries; expected {history_count}; run SqliteGraphStore::migrate"
        )));
    }
    Ok(())
}

fn sqlite_table_exists(connection: &Connection, table_name: &str) -> Result<bool, StoreError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table_name],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)
}

fn migration_status_report(
    current_version: Option<i64>,
    migration_ledger_validated: bool,
) -> Result<SqliteMigrationStatus, StoreError> {
    let target_version = SCHEMA_MIGRATIONS
        .last()
        .map(|migration| migration.to)
        .ok_or_else(|| StoreError::Integrity("SQLite migration registry is empty".to_owned()))?;
    let migrations = SCHEMA_MIGRATIONS
        .iter()
        .map(|migration| SqliteMigrationStatusEntry {
            version: migration.to,
            name: migration.name.to_owned(),
            applied: current_version.is_some_and(|version| migration.to <= version),
        })
        .collect();
    Ok(SqliteMigrationStatus {
        current_version,
        target_version,
        migration_ledger_validated,
        migrations,
    })
}

fn read_schema_version(connection: &Connection) -> Result<Option<i64>, StoreError> {
    connection
        .query_row(
            "SELECT version FROM syntaxmesh_schema WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(sql_error)
}

fn commit_schema_migration(
    transaction: Transaction<'_>,
    path: &Path,
    target_version: i64,
) -> Result<(), StoreError> {
    #[cfg(test)]
    if migration_test_sync::hit_failure(
        path,
        target_version,
        migration_test_sync::CommitBoundary::BeforeCommit,
    ) {
        return Err(StoreError::Backend(format!(
            "injected SQLite migration interruption before version {target_version} commit"
        )));
    }

    transaction.commit().map_err(|error| {
        StoreError::Backend(format!(
            "commit SQLite migration to version {target_version} for {} failed: {error}",
            path.display()
        ))
    })?;

    #[cfg(test)]
    if migration_test_sync::hit_failure(
        path,
        target_version,
        migration_test_sync::CommitBoundary::AfterCommit,
    ) {
        return Err(StoreError::Backend(format!(
            "injected SQLite migration interruption after version {target_version} commit"
        )));
    }
    Ok(())
}

fn record_schema_migration(
    transaction: &Transaction<'_>,
    applied_version: i64,
) -> Result<(), StoreError> {
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == applied_version)
        .ok_or_else(|| {
            StoreError::Integrity(format!(
                "SQLite migration registry has no version {applied_version}"
            ))
        })?;
    transaction
        .execute_batch(include_str!(
            "../migrations/0010_schema_migration_ledger.up.sql"
        ))
        .map_err(sql_error)?;
    let ledger_count = transaction
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    if ledger_count == 0 {
        for prior in SCHEMA_MIGRATIONS
            .iter()
            .filter(|prior| prior.to <= migration.from)
        {
            transaction
                .execute(
                    "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted) VALUES (?1, ?2, NULL, 1)",
                    params![prior.to, prior.name],
                )
                .map_err(sql_error)?;
        }
    }
    if migration.to >= 11 {
        transaction
            .execute(
                "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, unixepoch(), 0, ?3)",
                params![migration.to, migration.name, migration_checksum(migration)],
            )
            .map_err(sql_error)?;
    } else {
        transaction
            .execute(
                "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted) VALUES (?1, ?2, unixepoch(), 0)",
                params![migration.to, migration.name],
            )
            .map_err(sql_error)?;
    }
    Ok(())
}

fn migration_checksum(migration: &SchemaMigration) -> Option<String> {
    let sql = migration.checksum_sql?;
    let Some(rollback_sql) = migration.rollback_sql else {
        return Some(blake3::hash(sql.as_bytes()).to_hex().to_string());
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"syntaxmesh.sqlite.migration.up-down.v1\0");
    hasher.update(sql.as_bytes());
    hasher.update(b"\0down\0");
    hasher.update(rollback_sql.as_bytes());
    Some(hasher.finalize().to_hex().to_string())
}

fn validate_schema_migrations(
    connection: &Connection,
    schema_version: i64,
) -> Result<(), StoreError> {
    let records = {
        let mut statement = connection
            .prepare("SELECT version, name, applied_at_unix_seconds, adopted, checksum FROM syntaxmesh_schema_migrations ORDER BY version")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(sql_error)?;
        rows.map(|row| row.map_err(sql_error))
            .collect::<Result<Vec<_>, _>>()?
    };
    let expected = SCHEMA_MIGRATIONS
        .iter()
        .filter(|migration| migration.to <= schema_version)
        .collect::<Vec<_>>();
    if records.len() != expected.len() {
        return Err(StoreError::Integrity(format!(
            "SQLite applied-migration ledger contains {} entries; expected {}",
            records.len(),
            expected.len()
        )));
    }
    for ((version, name, applied_at, adopted, checksum), migration) in records.iter().zip(expected)
    {
        if *version != migration.to || name != migration.name {
            return Err(StoreError::Integrity(format!(
                "SQLite applied-migration ledger entry {version} ({name}) disagrees with registered migration {} ({})",
                migration.to, migration.name
            )));
        }
        if !matches!((*adopted, applied_at), (0, Some(_)) | (1, None)) {
            return Err(StoreError::Integrity(format!(
                "SQLite migration {version} has inconsistent adopted/timestamp metadata"
            )));
        }
        let expected_checksum = migration_checksum(migration);
        if checksum != &expected_checksum {
            return Err(StoreError::Integrity(format!(
                "SQLite migration checksum mismatch for version {version}: expected {expected_checksum:?}, observed {checksum:?}"
            )));
        }
    }
    Ok(())
}

fn insert_change_event(connection: &Connection, event: &ChangeEvent) -> Result<(), StoreError> {
    let sequence = connection
        .query_row(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [event.generation_after.0.0.as_slice()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| {
            StoreError::Integrity("change event has no accepted generation row".to_owned())
        })?;
    connection
        .execute(
            "INSERT OR IGNORE INTO syntaxmesh_change_events (generation, event_id, payload) VALUES (?1, ?2, ?3)",
            params![event.generation_after.0.0.as_slice(), event.id.0.0.as_slice(), encode(event)?],
        )
        .map_err(sql_error)?;
    for changed in &event.changed_facts {
        let (fact_kind, fact_id) = syntaxmesh_store::fact_storage_key(changed.fact);
        connection
            .execute(
                "INSERT OR IGNORE INTO syntaxmesh_change_event_facts (generation, generation_sequence, fact_kind, fact_id, change_kind) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![event.generation_after.0.0.as_slice(), sequence, fact_kind, fact_id, syntaxmesh_store::fact_change_kind_code(changed.kind)],
            )
            .map_err(sql_error)?;
    }
    Ok(())
}

fn apply_change_set_lineage_projection(
    connection: &Connection,
    delta: &ChangeSetDelta,
    sequence: i64,
) -> Result<(), StoreError> {
    for set in &delta.upsert_sets {
        connection
            .execute(
                "UPDATE syntaxmesh_change_set_versions SET valid_until_sequence = ?1 WHERE change_set_id = ?2 AND valid_until_sequence IS NULL",
                params![sequence, set.id.0.0.as_slice()],
            )
            .map_err(sql_error)?;
        connection
            .execute(
                "INSERT INTO syntaxmesh_change_set_versions (change_set_id, valid_from_sequence, valid_until_sequence, payload) VALUES (?1, ?2, NULL, ?3)",
                params![set.id.0.0.as_slice(), sequence, encode(set)?],
            )
            .map_err(sql_error)?;
    }
    for membership in &delta.assign_events {
        let active_provenance = connection
            .query_row(
                "SELECT provenance_id FROM syntaxmesh_change_set_membership_versions WHERE change_set_id = ?1 AND event_id = ?2 AND valid_until_sequence IS NULL",
                params![membership.change_set.0.0.as_slice(), membership.event.0.0.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(sql_error)?;
        if active_provenance.as_deref() == Some(membership.provenance.0.0.as_slice()) {
            continue;
        }
        connection
            .execute(
                "UPDATE syntaxmesh_change_set_membership_versions SET valid_until_sequence = ?1 WHERE change_set_id = ?2 AND event_id = ?3 AND valid_until_sequence IS NULL",
                params![sequence, membership.change_set.0.0.as_slice(), membership.event.0.0.as_slice()],
            )
            .map_err(sql_error)?;
        connection
            .execute(
                "INSERT INTO syntaxmesh_change_set_membership_versions (change_set_id, event_id, valid_from_sequence, valid_until_sequence, provenance_id) VALUES (?1, ?2, ?3, NULL, ?4)",
                params![membership.change_set.0.0.as_slice(), membership.event.0.0.as_slice(), sequence, membership.provenance.0.0.as_slice()],
            )
            .map_err(sql_error)?;
    }
    for removal in &delta.unassign_events {
        let closed = connection
            .execute(
                "UPDATE syntaxmesh_change_set_membership_versions SET valid_until_sequence = ?1 WHERE change_set_id = ?2 AND event_id = ?3 AND valid_until_sequence IS NULL",
                params![sequence, removal.key.change_set.0.0.as_slice(), removal.key.event.0.0.as_slice()],
            )
            .map_err(sql_error)?;
        if closed != 1 {
            return Err(StoreError::Integrity(
                "accepted lineage removal did not close exactly one active membership".to_owned(),
            ));
        }
    }
    Ok(())
}

fn consequence_endpoint_storage(
    endpoint: LineageEndpoint,
) -> (&'static str, Vec<u8>, Option<i64>, Option<Vec<u8>>) {
    match endpoint {
        LineageEndpoint::ChangeEvent(id) => ("change_event", id.0.0.to_vec(), None, None),
        LineageEndpoint::FactVersion(reference) => {
            let (kind, id) = syntaxmesh_store::fact_storage_key(reference.fact);
            (
                "fact_version",
                id,
                Some(kind),
                Some(reference.valid_from.0.0.to_vec()),
            )
        }
    }
}

fn apply_consequence_projection(
    connection: &Connection,
    delta: &ConsequenceDelta,
    sequence: i64,
) -> Result<(), StoreError> {
    for retraction in &delta.retract {
        let closed = connection
            .execute(
                "UPDATE syntaxmesh_consequence_edge_versions SET valid_until_sequence = ?1 WHERE edge_id = ?2 AND valid_until_sequence IS NULL",
                params![sequence, retraction.edge.0.0.as_slice()],
            )
            .map_err(sql_error)?;
        if closed != 1 {
            return Err(StoreError::Integrity(
                "consequence retraction did not close exactly one active edge".to_owned(),
            ));
        }
    }
    for edge in &delta.add {
        let (source_kind, source_id, source_fact_kind, source_generation) =
            consequence_endpoint_storage(edge.source);
        let (target_kind, target_id, target_fact_kind, target_generation) =
            consequence_endpoint_storage(edge.target);
        connection
            .execute(
                "INSERT INTO syntaxmesh_consequence_edge_versions (edge_id, valid_from_sequence, valid_until_sequence, source_kind, source_id, source_fact_kind, source_valid_from_generation, target_kind, target_id, target_fact_kind, target_valid_from_generation, payload) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    edge.id.0.0.as_slice(), sequence, source_kind, source_id,
                    source_fact_kind, source_generation, target_kind, target_id,
                    target_fact_kind, target_generation, encode(edge)?,
                ],
            )
            .map_err(sql_error)?;
        for (ordinal, evidence) in edge.evidence.iter().enumerate() {
            let ordinal = i64::try_from(ordinal).map_err(|error| {
                StoreError::Backend(format!("evidence ordinal overflow: {error}"))
            })?;
            let (fact_kind, fact_id) = syntaxmesh_store::fact_storage_key(evidence.fact);
            connection
                .execute(
                    "INSERT INTO syntaxmesh_consequence_evidence (edge_id, ordinal, fact_kind, fact_id, valid_from_generation) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![edge.id.0.0.as_slice(), ordinal, fact_kind, fact_id, evidence.valid_from.0.0.as_slice()],
                )
                .map_err(sql_error)?;
        }
        if let ConsequenceDerivation::DerivedFrom(parents) = &edge.derivation {
            for parent in parents {
                connection
                    .execute(
                        "INSERT INTO syntaxmesh_consequence_parents (edge_id, parent_edge_id) VALUES (?1, ?2)",
                        params![edge.id.0.0.as_slice(), parent.0.0.as_slice()],
                    )
                    .map_err(sql_error)?;
            }
        }
    }
    Ok(())
}

fn backfill_change_events_in_transaction(
    transaction: &Transaction<'_>,
    memory: &InMemoryGraphStore,
) -> Result<(), StoreError> {
    let history = memory.generation_history()?;
    let expected_events = i64::try_from(
        history.iter().filter(|entry| entry.delta.is_some()).count(),
    )
    .map_err(|error| StoreError::Backend(format!("change-event count is too large: {error}")))?;
    let actual_events = transaction
        .query_row("SELECT COUNT(*) FROM syntaxmesh_change_events", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(sql_error)?;
    if actual_events == expected_events {
        return Ok(());
    }
    let events = syntaxmesh_store::change_events_from_history(&history)?;
    transaction
        .execute_batch(
            "DELETE FROM syntaxmesh_change_event_facts; DELETE FROM syntaxmesh_change_events;",
        )
        .map_err(sql_error)?;
    for event in &events {
        insert_change_event(transaction, event)?;
    }
    Ok(())
}

fn read_change_events_for_fact(
    connection: &Connection,
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
    let (fact_kind, fact_id) = syntaxmesh_store::fact_storage_key(fact);
    let after_sequence = if let Some(cursor) = after {
        Some(
            connection
                .query_row(
                    "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
                    [cursor.generation.0.0.as_slice()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(sql_error)?
                .ok_or(StoreError::StaleBase {
                    expected: Some(cursor.generation),
                    actual: read_manifest(connection)?.map(|manifest| manifest.generation),
                })?,
        )
    } else {
        None
    };
    let sql_limit = i64::try_from(limit.saturating_add(1)).map_err(|error| {
        StoreError::Backend(format!("change-event limit is too large: {error}"))
    })?;
    let mut statement = connection
        .prepare("SELECT event.payload FROM syntaxmesh_change_event_facts AS fact JOIN syntaxmesh_change_events AS event ON event.generation = fact.generation WHERE fact.fact_kind = ?1 AND fact.fact_id = ?2 AND (?3 IS NULL OR fact.generation_sequence > ?3) ORDER BY fact.generation_sequence LIMIT ?4")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![fact_kind, fact_id, after_sequence, sql_limit],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(sql_error)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(decode(&row.map_err(sql_error)?)?);
    }
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = has_more
        .then(|| items.last())
        .flatten()
        .map(|event: &ChangeEvent| ChangeEventCursor {
            generation: event.generation_after,
        });
    Ok(ChangeEventPage { items, next_cursor })
}

fn read_change_event(
    connection: &Connection,
    generation: GenerationId,
) -> Result<Option<ChangeEvent>, StoreError> {
    let known_generation = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_history WHERE generation = ?1)",
            [generation.0.0.as_slice()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
    if !known_generation {
        return Err(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_manifest(connection)?.map(|manifest| manifest.generation),
        });
    }
    connection
        .query_row(
            "SELECT payload FROM syntaxmesh_change_events WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|payload| decode(&payload))
        .transpose()
}

fn read_events_for_change_set(
    connection: &Connection,
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
    if let Some(cursor) = after
        && (cursor.change_set != change_set || cursor.as_of_generation != as_of)
    {
        return Err(StoreError::InvalidDelta(
            "change-set cursor is bound to a different set or snapshot".to_owned(),
        ));
    }
    let as_of_sequence = history_sequence(connection, as_of)?;
    let after_sequence = after
        .map(|cursor| history_sequence(connection, cursor.after_event_generation))
        .transpose()?;
    if after_sequence.is_some_and(|sequence| sequence > as_of_sequence) {
        return Err(StoreError::StaleBase {
            expected: Some(as_of),
            actual: after.map(|cursor| cursor.after_event_generation),
        });
    }
    let limit_plus_one = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let sql = if after_sequence.is_some() {
        "SELECT event.payload, membership.provenance_id, membership_generation.generation FROM syntaxmesh_change_set_membership_versions AS membership JOIN syntaxmesh_change_events AS event ON event.event_id = membership.event_id JOIN syntaxmesh_generation_history AS event_generation ON event_generation.generation = event.generation JOIN syntaxmesh_generation_history AS membership_generation ON membership_generation.sequence = membership.valid_from_sequence WHERE membership.change_set_id = ?1 AND membership.valid_from_sequence <= ?2 AND (membership.valid_until_sequence IS NULL OR membership.valid_until_sequence > ?2) AND event_generation.sequence <= ?2 AND event_generation.sequence > ?3 ORDER BY event_generation.sequence, event.event_id LIMIT ?4"
    } else {
        "SELECT event.payload, membership.provenance_id, membership_generation.generation FROM syntaxmesh_change_set_membership_versions AS membership JOIN syntaxmesh_change_events AS event ON event.event_id = membership.event_id JOIN syntaxmesh_generation_history AS event_generation ON event_generation.generation = event.generation JOIN syntaxmesh_generation_history AS membership_generation ON membership_generation.sequence = membership.valid_from_sequence WHERE membership.change_set_id = ?1 AND membership.valid_from_sequence <= ?2 AND (membership.valid_until_sequence IS NULL OR membership.valid_until_sequence > ?2) AND event_generation.sequence <= ?2 ORDER BY event_generation.sequence, event.event_id LIMIT ?3"
    };
    let mut statement = connection.prepare(sql).map_err(sql_error)?;
    let mapper = |row: &rusqlite::Row<'_>| {
        Ok((
            row.get::<_, Vec<u8>>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, Vec<u8>>(2)?,
        ))
    };
    let rows = if let Some(after_sequence) = after_sequence {
        statement.query_map(
            params![
                change_set.0.0.as_slice(),
                as_of_sequence,
                after_sequence,
                limit_plus_one,
            ],
            mapper,
        )
    } else {
        statement.query_map(
            params![change_set.0.0.as_slice(), as_of_sequence, limit_plus_one],
            mapper,
        )
    }
    .map_err(sql_error)?;
    let mut items = rows
        .map(|row| {
            let (event_payload, provenance_bytes, valid_from_bytes) = row.map_err(sql_error)?;
            let event: ChangeEvent = decode(&event_payload)?;
            let provenance_id: [u8; 32] =
                provenance_bytes.try_into().map_err(|bytes: Vec<u8>| {
                    StoreError::Integrity(format!(
                        "membership provenance id has {} bytes",
                        bytes.len()
                    ))
                })?;
            let valid_from_id: [u8; 32] =
                valid_from_bytes.try_into().map_err(|bytes: Vec<u8>| {
                    StoreError::Integrity(format!(
                        "membership generation id has {} bytes",
                        bytes.len()
                    ))
                })?;
            Ok(ChangeSetEvent {
                membership: syntaxmesh_core::ChangeSetMembership {
                    change_set,
                    event: event.id,
                    provenance: ProvenanceId(StableId(provenance_id)),
                },
                event,
                membership_valid_from: GenerationId(StableId(valid_from_id)),
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let next_cursor = if items.len() > limit {
        items
            .get(limit.saturating_sub(1))
            .map(|item| EventsForChangeSetCursor {
                change_set,
                as_of_generation: as_of,
                after_event_generation: item.event.generation_after,
            })
    } else {
        None
    };
    items.truncate(limit);
    Ok(EventsForChangeSetPage { items, next_cursor })
}

fn read_change_set_at(
    connection: &Connection,
    change_set: ChangeSetId,
    as_of: GenerationId,
) -> Result<Option<ChangeSetVersion>, StoreError> {
    let sequence = history_sequence(connection, as_of)?;
    let row = connection
        .query_row(
            "SELECT version.payload, valid_from.generation, valid_until.generation \
             FROM syntaxmesh_change_set_versions AS version \
             JOIN syntaxmesh_generation_history AS valid_from ON valid_from.sequence = version.valid_from_sequence \
             LEFT JOIN syntaxmesh_generation_history AS valid_until ON valid_until.sequence = version.valid_until_sequence \
             WHERE version.change_set_id = ?1 AND version.valid_from_sequence <= ?2 \
               AND (version.valid_until_sequence IS NULL OR version.valid_until_sequence > ?2) \
             ORDER BY version.valid_from_sequence DESC LIMIT 1",
            params![change_set.0.0.as_slice(), sequence],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    row.map(|(payload, from, until)| {
        let from: [u8; 32] = from.try_into().map_err(|bytes: Vec<u8>| {
            StoreError::Integrity(format!(
                "ChangeSet valid-from generation has {} bytes",
                bytes.len()
            ))
        })?;
        let until = until
            .map(|bytes| {
                let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    StoreError::Integrity(format!(
                        "ChangeSet valid-until generation has {} bytes",
                        bytes.len()
                    ))
                })?;
                Ok(GenerationId(StableId(bytes)))
            })
            .transpose()?;
        Ok(ChangeSetVersion {
            change_set: decode(&payload)?,
            valid_from: GenerationId(StableId(from)),
            valid_until: until,
        })
    })
    .transpose()
}

fn read_consequence_edges_for_endpoint(
    connection: &Connection,
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
    let sequence = history_sequence(connection, as_of)?;
    let (kind, id, fact_kind, valid_from) = consequence_endpoint_storage(endpoint);
    let after_id = after.map(|cursor| cursor.after_edge.0.0.to_vec());
    let sql_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut statement = connection.prepare(
        "SELECT payload FROM syntaxmesh_consequence_edge_versions WHERE valid_from_sequence <= ?1 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?1) AND ((source_kind = ?2 AND source_id = ?3 AND source_fact_kind IS ?4 AND source_valid_from_generation IS ?5) OR (target_kind = ?6 AND target_id = ?7 AND target_fact_kind IS ?8 AND target_valid_from_generation IS ?9)) AND (?10 IS NULL OR edge_id > ?10) ORDER BY edge_id LIMIT ?11",
    ).map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![
                sequence, kind, id, fact_kind, valid_from, kind, id, fact_kind, valid_from,
                after_id, sql_limit
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(sql_error)?;
    let mut items = rows
        .map(|row| decode(&row.map_err(sql_error)?))
        .collect::<Result<Vec<ConsequenceEdge>, StoreError>>()?;
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

fn read_consequence_edges_for_endpoint_range(
    connection: &Connection,
    endpoint: LineageEndpoint,
    from_generation: GenerationId,
    until_generation: GenerationId,
    after: Option<ConsequenceRangeCursor>,
    limit: usize,
) -> Result<ConsequenceRangePage, StoreError> {
    if limit == 0 {
        return Ok(ConsequenceRangePage::new(Vec::new(), None));
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
    let (kind, id, fact_kind, endpoint_valid_from) = consequence_endpoint_storage(endpoint);
    let after_id = after.map(|cursor| cursor.after_edge.0.0.to_vec());
    let sql_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut statement = connection
        .prepare(CONSEQUENCE_RANGE_QUERY)
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![
                from_generation.0.0.as_slice(),
                until_generation.0.0.as_slice(),
                kind,
                id,
                fact_kind,
                endpoint_valid_from,
                kind,
                id,
                fact_kind,
                endpoint_valid_from,
                after_id,
                sql_limit
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                ))
            },
        )
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    #[cfg(feature = "benchmark-instrumentation")]
    let sql_read_statements = 1;
    let Some((from_sequence, until_sequence, ..)) = rows.first() else {
        // Preserve the established stale-generation diagnostic on this error
        // path; valid pages (including empty ones) use a single SQL read.
        history_sequence(connection, from_generation)?;
        history_sequence(connection, until_generation)?;
        return Err(StoreError::Integrity(
            "consequence range bounds disappeared during lookup".to_owned(),
        ));
    };
    if from_sequence > until_sequence {
        return Err(StoreError::InvalidDelta(
            "consequence range starts after it ends".to_owned(),
        ));
    }
    let mut items = rows
        .into_iter()
        .filter_map(|(_, _, payload, start, finish)| {
            payload.map(|payload| (payload, start, finish))
        })
        .map(|(payload, start, finish)| {
            let edge = decode(&payload)?;
            let start = start.ok_or_else(|| {
                StoreError::Integrity("consequence start generation is missing".to_owned())
            })?;
            let valid_from = GenerationId(syntaxmesh_core::StableId(start.try_into().map_err(
                |error| {
                    StoreError::Integrity(format!(
                        "invalid consequence start generation id: {error:?}"
                    ))
                },
            )?));
            let valid_until = finish
                .map(|bytes| {
                    bytes
                        .try_into()
                        .map(syntaxmesh_core::StableId)
                        .map(GenerationId)
                        .map_err(|error| {
                            StoreError::Integrity(format!(
                                "invalid consequence end generation id: {error:?}"
                            ))
                        })
                })
                .transpose()?;
            Ok::<_, StoreError>(ConsequenceEdgeVersion {
                edge,
                valid_from,
                valid_until,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
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
    let metrics = syntaxmesh_store::ConsequenceRangeReadMetrics {
        sql_read_statements,
        consequence_rows_returned: items.len(),
        ..syntaxmesh_store::ConsequenceRangeReadMetrics::default()
    };
    let page_result = ConsequenceRangePage::new(items, next_cursor);
    #[cfg(feature = "benchmark-instrumentation")]
    let page_result = page_result.with_read_metrics(metrics);
    Ok(page_result)
}

fn initialize_history_anchor_in_transaction(
    transaction: &Transaction<'_>,
    memory: &InMemoryGraphStore,
) -> Result<(), StoreError> {
    let count = transaction
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_generation_history",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    if count != 0 {
        return Ok(());
    }
    let Some(manifest) = memory.current_manifest() else {
        return Ok(());
    };
    let anchor = memory
        .generation_history()?
        .into_iter()
        .find(|entry| entry.manifest.generation == manifest.generation)
        .ok_or_else(|| StoreError::Integrity("legacy manifest has no history anchor".to_owned()))?;
    let snapshot = anchor.anchor.as_ref().ok_or_else(|| {
        StoreError::Integrity("legacy history entry is missing its snapshot anchor".to_owned())
    })?;
    transaction
        .execute_batch(
            "DELETE FROM syntaxmesh_temporal_roots; DELETE FROM syntaxmesh_temporal_tree_pages;",
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "INSERT INTO syntaxmesh_generation_history (sequence, generation, payload) VALUES (1, ?1, ?2)",
            params![manifest.generation.0.0.as_slice(), encode(&anchor)?],
        )
        .map_err(sql_error)?;
    seed_temporal_snapshot(transaction, snapshot, 1)?;
    let mutations = snapshot_tree_mutations(snapshot, manifest.schema_version)?;
    publish_persistent_root(transaction, 1, manifest.generation, None, &mutations)?;
    let has_incidence_roots = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'syntaxmesh_temporal_incidence_roots')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
    if has_incidence_roots {
        transaction
            .execute("DELETE FROM syntaxmesh_temporal_incidence_roots", [])
            .map_err(sql_error)?;
        publish_incidence_root(
            transaction,
            1,
            manifest.generation,
            None,
            &snapshot_incidence_changes(snapshot),
        )?;
    }
    transaction.execute(
        "INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (1, ?1, ?2, ?3)",
        params![manifest.generation.0.0.as_slice(), encode(&manifest)?, encode(snapshot)?],
    ).map_err(sql_error)?;
    Ok(())
}

fn stable_id(bytes: &[u8]) -> Result<StableId, StoreError> {
    let value: [u8; 32] = bytes.try_into().map_err(|error| {
        StoreError::Integrity(format!(
            "persistent temporal index ID is not 32 bytes: {error}"
        ))
    })?;
    Ok(StableId(value))
}

fn read_node_history(
    connection: &Connection,
    id: NodeId,
) -> Result<Vec<NodeHistoryVersion>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT start_entry.payload, end_entry.payload, versions.payload FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 ORDER BY versions.valid_from_sequence",
    ).map_err(sql_error)?;
    let rows = statement
        .query_map(params![NODE_FACT, id.0.0.as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Option<Vec<u8>>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(sql_error)?;
    rows.map(|row| {
        let (start_bytes, end_bytes, node_bytes) = row.map_err(sql_error)?;
        let start: GenerationHistoryEntry = decode(&start_bytes)?;
        let until = end_bytes
            .map(|bytes| {
                decode::<GenerationHistoryEntry>(&bytes).map(|entry| entry.manifest.generation)
            })
            .transpose()?;
        let node = decode(&node_bytes)?;
        Ok(NodeHistoryVersion {
            valid_from: start.manifest.generation,
            valid_until: until,
            node,
        })
    })
    .collect()
}

fn read_fact_history(
    connection: &Connection,
    fact: FactRef,
) -> Result<Vec<FactHistoryVersion>, StoreError> {
    let (kind, id) = match fact {
        FactRef::File(id) => (FILE_FACT, id.0.0),
        FactRef::Provenance(id) => (PROVENANCE_FACT, id.0.0),
        FactRef::Node(id) => (NODE_FACT, id.0.0),
        FactRef::Edge(id) => (EDGE_FACT, id.0.0),
    };
    let mut statement = connection.prepare(
        "SELECT start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 ORDER BY versions.valid_from_sequence",
    ).map_err(sql_error)?;
    let rows = statement
        .query_map(params![kind, id.as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Option<Vec<u8>>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
            ))
        })
        .map_err(sql_error)?;
    rows.map(|row| {
        let (start_bytes, end_bytes, payload_bytes, observed_bytes, accepted_bytes) =
            row.map_err(sql_error)?;
        let start: GenerationHistoryEntry = decode(&start_bytes)?;
        let valid_until = end_bytes
            .map(|bytes| {
                decode::<GenerationHistoryEntry>(&bytes).map(|entry| entry.manifest.generation)
            })
            .transpose()?;
        let payload = match fact {
            FactRef::File(_) => FactPayload::File(decode::<FileVersion>(&payload_bytes)?),
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode::<Provenance>(&payload_bytes)?)
            }
            FactRef::Node(_) => FactPayload::Node(decode::<Node>(&payload_bytes)?),
            FactRef::Edge(_) => FactPayload::Edge(decode::<Edge>(&payload_bytes)?),
        };
        let observed_at =
            decode_optional_time(observed_bytes, "observed_at_unix_nanos")?.map(ObservationTime);
        let accepted_at =
            decode_optional_time(accepted_bytes, "accepted_at_unix_nanos")?.map(AcceptanceTime);
        Ok(FactHistoryVersion {
            valid_from: start.manifest.generation,
            valid_until,
            observed_at,
            accepted_at,
            payload,
        })
    })
    .collect()
}

fn read_fact_versions_changed_at(
    connection: &Connection,
    fact: FactRef,
    generation: GenerationId,
) -> Result<Vec<FactHistoryEntry>, StoreError> {
    let changed_sequence = history_sequence(connection, generation)?;
    let (kind, id) = syntaxmesh_store::fact_storage_key(fact);
    let mut statement = connection
        .prepare(
            "SELECT versions.valid_from_sequence, versions.valid_until_sequence, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 AND versions.valid_from_sequence = ?3 UNION ALL SELECT versions.valid_from_sequence, versions.valid_until_sequence, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_end_idx JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 AND versions.valid_until_sequence = ?3 AND versions.valid_from_sequence <> ?3 ORDER BY valid_from_sequence",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![kind, id, changed_sequence], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?,
                row.get::<_, Option<Vec<u8>>>(6)?,
            ))
        })
        .map_err(sql_error)?;
    rows.map(|row| {
        let (
            valid_from_sequence,
            valid_until_sequence,
            start_bytes,
            end_bytes,
            payload_bytes,
            observed_bytes,
            accepted_bytes,
        ) = row.map_err(sql_error)?;
        let start: GenerationHistoryEntry = decode(&start_bytes)?;
        let valid_until = end_bytes
            .map(|bytes| {
                decode::<GenerationHistoryEntry>(&bytes).map(|entry| entry.manifest.generation)
            })
            .transpose()?;
        let payload = match fact {
            FactRef::File(_) => FactPayload::File(decode::<FileVersion>(&payload_bytes)?),
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode::<Provenance>(&payload_bytes)?)
            }
            FactRef::Node(_) => FactPayload::Node(decode::<Node>(&payload_bytes)?),
            FactRef::Edge(_) => FactPayload::Edge(decode::<Edge>(&payload_bytes)?),
        };
        Ok(FactHistoryEntry {
            fact,
            valid_from_sequence: u64::try_from(valid_from_sequence).map_err(|error| {
                StoreError::Integrity(format!("invalid fact version start sequence: {error}"))
            })?,
            valid_until_sequence: valid_until_sequence
                .map(|end_sequence| {
                    u64::try_from(end_sequence).map_err(|error| {
                        StoreError::Integrity(format!("invalid fact version end sequence: {error}"))
                    })
                })
                .transpose()?,
            version: FactHistoryVersion {
                valid_from: start.manifest.generation,
                valid_until,
                observed_at: decode_optional_time(observed_bytes, "observed_at_unix_nanos")?
                    .map(ObservationTime),
                accepted_at: decode_optional_time(accepted_bytes, "accepted_at_unix_nanos")?
                    .map(AcceptanceTime),
                payload,
            },
        })
    })
    .collect()
}

fn read_fact_version_changes_at_page(
    connection: &Connection,
    generation: GenerationId,
    after: Option<FactVersionChangeCursor>,
    limit: usize,
) -> Result<FactVersionChangePage, StoreError> {
    if limit > MAX_FACT_VERSION_CHANGE_PAGE_SIZE {
        return Err(StoreError::InvalidPageLimit);
    }
    if after.is_some_and(|cursor| cursor.generation != generation) {
        return Err(StoreError::InvalidDelta(
            "fact-version-change cursor is bound to a different generation".to_owned(),
        ));
    }
    if limit == 0 {
        return Ok(FactVersionChangePage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let changed_sequence = history_sequence(connection, generation)?;
    let (after_kind, after_id, after_sequence) =
        after.map_or((-1_i64, Vec::new(), 0_i64), |cursor| {
            let (kind, id) = syntaxmesh_store::fact_storage_key(cursor.after_fact);
            (
                kind,
                id,
                i64::try_from(cursor.after_valid_from_sequence).unwrap_or(i64::MAX),
            )
        });
    if after_sequence > changed_sequence {
        return Err(StoreError::InvalidDelta(
            "fact-version-change cursor is after its generation".to_owned(),
        ));
    }
    if after.is_some() && after_sequence == 0 {
        return Err(StoreError::InvalidDelta(
            "fact-version-change cursor has a zero start sequence".to_owned(),
        ));
    }
    let fetch_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut statement = connection
        .prepare(FACT_VERSION_CHANGE_PAGE_QUERY)
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![
                changed_sequence,
                after_kind,
                after_id,
                after_sequence,
                fetch_limit
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, Option<Vec<u8>>>(7)?,
                    row.get::<_, Option<Vec<u8>>>(8)?,
                ))
            },
        )
        .map_err(sql_error)?;
    let mut items = rows
        .map(|row| {
            let (
                kind,
                id,
                valid_from_sequence,
                valid_until_sequence,
                start_bytes,
                end_bytes,
                payload_bytes,
                observed_bytes,
                accepted_bytes,
            ) = row.map_err(sql_error)?;
            let fact = fact_ref_from_parts(kind, id)?;
            let start: GenerationHistoryEntry = decode(&start_bytes)?;
            let valid_until = end_bytes
                .map(|bytes| {
                    decode::<GenerationHistoryEntry>(&bytes).map(|entry| entry.manifest.generation)
                })
                .transpose()?;
            let payload = match fact {
                FactRef::File(_) => FactPayload::File(decode::<FileVersion>(&payload_bytes)?),
                FactRef::Provenance(_) => {
                    FactPayload::Provenance(decode::<Provenance>(&payload_bytes)?)
                }
                FactRef::Node(_) => FactPayload::Node(decode::<Node>(&payload_bytes)?),
                FactRef::Edge(_) => FactPayload::Edge(decode::<Edge>(&payload_bytes)?),
            };
            Ok(FactHistoryEntry {
                fact,
                valid_from_sequence: u64::try_from(valid_from_sequence).map_err(|error| {
                    StoreError::Integrity(format!("invalid fact-version start sequence: {error}"))
                })?,
                valid_until_sequence: valid_until_sequence
                    .map(|sequence| {
                        u64::try_from(sequence).map_err(|error| {
                            StoreError::Integrity(format!(
                                "invalid fact-version end sequence: {error}"
                            ))
                        })
                    })
                    .transpose()?,
                version: FactHistoryVersion {
                    valid_from: start.manifest.generation,
                    valid_until,
                    observed_at: decode_optional_time(observed_bytes, "observed_at_unix_nanos")?
                        .map(ObservationTime),
                    accepted_at: decode_optional_time(accepted_bytes, "accepted_at_unix_nanos")?
                        .map(AcceptanceTime),
                    payload,
                },
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
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

fn read_fact_history_page(
    connection: &Connection,
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
    let as_of_sequence = history_sequence(connection, as_of_generation)?;
    let (after_kind, after_id, after_sequence) = if let Some(cursor) = after {
        let (kind, id) = syntaxmesh_store::fact_storage_key(cursor.after_fact);
        let sequence = history_sequence(connection, cursor.after_valid_from)?;
        if sequence > as_of_sequence {
            return Err(StoreError::InvalidDelta(
                "fact-history cursor is outside its retained snapshot".to_owned(),
            ));
        }
        (Some(kind), Some(id), Some(sequence))
    } else {
        (None, None, None)
    };
    let sql_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut statement = connection
        .prepare(if after.is_some() {
            FACT_HISTORY_AFTER_PAGE_QUERY
        } else {
            FACT_HISTORY_FIRST_PAGE_QUERY
        })
        .map_err(sql_error)?;
    let row_mapper = |row: &rusqlite::Row<'_>| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, Vec<u8>>(2)?,
            row.get::<_, Option<Vec<u8>>>(3)?,
            row.get::<_, Vec<u8>>(4)?,
            row.get::<_, Option<Vec<u8>>>(5)?,
            row.get::<_, Option<Vec<u8>>>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, Option<i64>>(8)?,
        ))
    };
    let rows =
        if let (Some(kind), Some(id), Some(sequence)) = (after_kind, after_id, after_sequence) {
            statement
                .query_map(
                    params![
                        as_of_generation.0.0.as_slice(),
                        kind,
                        id,
                        sequence,
                        sql_limit
                    ],
                    row_mapper,
                )
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?
        } else {
            statement
                .query_map(
                    params![as_of_generation.0.0.as_slice(), sql_limit],
                    row_mapper,
                )
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?
        };
    let mut items = rows
        .into_iter()
        .map(
            |(
                kind,
                id,
                start_bytes,
                end_bytes,
                payload_bytes,
                observed_bytes,
                accepted_bytes,
                valid_from_sequence,
                valid_until_sequence,
            )| {
                let id: [u8; 32] = id.try_into().map_err(|bytes: Vec<u8>| {
                    StoreError::Integrity(format!(
                        "fact history identity has {} bytes",
                        bytes.len()
                    ))
                })?;
                let fact = match kind {
                    FILE_FACT => FactRef::File(FileId(StableId(id))),
                    PROVENANCE_FACT => FactRef::Provenance(ProvenanceId(StableId(id))),
                    NODE_FACT => FactRef::Node(NodeId(StableId(id))),
                    EDGE_FACT => FactRef::Edge(EdgeId(StableId(id))),
                    _ => {
                        return Err(StoreError::Integrity(format!(
                            "unknown fact history family {kind}"
                        )));
                    }
                };
                let start: GenerationHistoryEntry = decode(&start_bytes)?;
                let valid_until = end_bytes
                    .map(|bytes| {
                        decode::<GenerationHistoryEntry>(&bytes)
                            .map(|entry| entry.manifest.generation)
                    })
                    .transpose()?;
                let payload = match fact {
                    FactRef::File(_) => FactPayload::File(decode::<FileVersion>(&payload_bytes)?),
                    FactRef::Provenance(_) => {
                        FactPayload::Provenance(decode::<Provenance>(&payload_bytes)?)
                    }
                    FactRef::Node(_) => FactPayload::Node(decode::<Node>(&payload_bytes)?),
                    FactRef::Edge(_) => FactPayload::Edge(decode::<Edge>(&payload_bytes)?),
                };
                let observed_at = decode_optional_time(observed_bytes, "observed_at_unix_nanos")?
                    .map(ObservationTime);
                let accepted_at = decode_optional_time(accepted_bytes, "accepted_at_unix_nanos")?
                    .map(AcceptanceTime);
                let valid_from_sequence = u64::try_from(valid_from_sequence).map_err(|error| {
                    StoreError::Integrity(format!("invalid fact start sequence: {error}"))
                })?;
                let valid_until_sequence = valid_until_sequence
                    .map(|sequence| {
                        u64::try_from(sequence).map_err(|error| {
                            StoreError::Integrity(format!("invalid fact end sequence: {error}"))
                        })
                    })
                    .transpose()?;
                Ok(FactHistoryEntry {
                    fact,
                    valid_from_sequence,
                    valid_until_sequence,
                    version: FactHistoryVersion {
                        valid_from: start.manifest.generation,
                        valid_until,
                        observed_at,
                        accepted_at,
                        payload,
                    },
                })
            },
        )
        .collect::<Result<Vec<_>, StoreError>>()?;
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

fn decode_optional_time(bytes: Option<Vec<u8>>, name: &str) -> Result<Option<u64>, StoreError> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let bytes: [u8; 8] = bytes.try_into().map_err(|invalid: Vec<u8>| {
        StoreError::Integrity(format!(
            "{name} has {} bytes instead of eight",
            invalid.len()
        ))
    })?;
    Ok(Some(u64::from_be_bytes(bytes)))
}

fn fact_ref_from_parts(kind: i64, bytes: Vec<u8>) -> Result<FactRef, StoreError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|invalid: Vec<u8>| {
        StoreError::Integrity(format!(
            "fact history identity has {} bytes instead of 32",
            invalid.len()
        ))
    })?;
    let id = StableId(bytes);
    match kind {
        FILE_FACT => Ok(FactRef::File(FileId(id))),
        PROVENANCE_FACT => Ok(FactRef::Provenance(ProvenanceId(id))),
        NODE_FACT => Ok(FactRef::Node(NodeId(id))),
        EDGE_FACT => Ok(FactRef::Edge(EdgeId(id))),
        _ => Err(StoreError::Integrity(format!(
            "unknown temporal fact kind {kind}"
        ))),
    }
}

fn fact_ref_parts(fact: FactRef) -> (i64, Vec<u8>) {
    match fact {
        FactRef::File(id) => (FILE_FACT, id.0.0.to_vec()),
        FactRef::Provenance(id) => (PROVENANCE_FACT, id.0.0.to_vec()),
        FactRef::Node(id) => (NODE_FACT, id.0.0.to_vec()),
        FactRef::Edge(id) => (EDGE_FACT, id.0.0.to_vec()),
    }
}

fn read_observed_facts_between(
    connection: &Connection,
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
    let fetch_count = limit
        .checked_add(1)
        .ok_or_else(|| StoreError::Backend("observation query limit overflow".to_owned()))?;
    let fetch_count = i64::try_from(fetch_count)
        .map_err(|error| StoreError::Backend(format!("observation query limit: {error}")))?;
    let from = from_inclusive.0.to_be_bytes();
    let until = until_exclusive.0.to_be_bytes();
    let query = "SELECT versions.fact_kind, versions.fact_id, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.observed_at_unix_nanos >= ?1 AND versions.observed_at_unix_nanos < ?2 ORDER BY versions.observed_at_unix_nanos, versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?3";
    let mut items = if let Some(cursor) = after {
        let (kind, id) = fact_ref_parts(cursor.fact);
        let sequence = history_sequence(connection, cursor.valid_from)?;
        let cursor_time = cursor.observed_at.0.to_be_bytes();
        let cursor_query = "SELECT versions.fact_kind, versions.fact_id, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.observed_at_unix_nanos >= ?1 AND versions.observed_at_unix_nanos < ?2 AND (versions.observed_at_unix_nanos, versions.fact_kind, versions.fact_id, versions.valid_from_sequence) > (?3, ?4, ?5, ?6) ORDER BY versions.observed_at_unix_nanos, versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?7";
        read_observed_rows(
            connection,
            cursor_query,
            params![
                from.as_slice(),
                until.as_slice(),
                cursor_time.as_slice(),
                kind,
                id.as_slice(),
                sequence,
                fetch_count
            ],
        )?
    } else {
        read_observed_rows(
            connection,
            query,
            params![from.as_slice(), until.as_slice(), fetch_count],
        )?
    };
    let next_cursor = if items.len() > limit {
        limit
            .checked_sub(1)
            .and_then(|index| items.get(index))
            .and_then(|item| {
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
    items.truncate(limit);
    Ok(ObservedFactPage { items, next_cursor })
}

fn read_observed_rows<P: rusqlite::Params>(
    connection: &Connection,
    query: &str,
    parameters: P,
) -> Result<Vec<ObservedFactVersion>, StoreError> {
    let mut statement = connection.prepare(query).map_err(sql_error)?;
    let rows = statement
        .query_map(parameters, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, Option<Vec<u8>>>(6)?,
            ))
        })
        .map_err(sql_error)?;
    rows.map(|row| {
        let (kind, id_bytes, start_bytes, end_bytes, payload_bytes, observed_bytes, accepted_bytes) =
            row.map_err(sql_error)?;
        let fact = fact_ref_from_parts(kind, id_bytes)?;
        let start: GenerationHistoryEntry = decode(&start_bytes)?;
        let valid_until = end_bytes
            .map(|bytes| {
                decode::<GenerationHistoryEntry>(&bytes).map(|entry| entry.manifest.generation)
            })
            .transpose()?;
        let observed_at = decode_optional_time(Some(observed_bytes), "observed_at_unix_nanos")?
            .map(ObservationTime)
            .ok_or_else(|| StoreError::Integrity("indexed observation time is NULL".to_owned()))?;
        let accepted_at =
            decode_optional_time(accepted_bytes, "accepted_at_unix_nanos")?.map(AcceptanceTime);
        let payload = match fact {
            FactRef::File(_) => FactPayload::File(decode::<FileVersion>(&payload_bytes)?),
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode::<Provenance>(&payload_bytes)?)
            }
            FactRef::Node(_) => FactPayload::Node(decode::<Node>(&payload_bytes)?),
            FactRef::Edge(_) => FactPayload::Edge(decode::<Edge>(&payload_bytes)?),
        };
        Ok(ObservedFactVersion {
            fact,
            version: FactHistoryVersion {
                valid_from: start.manifest.generation,
                valid_until,
                observed_at: Some(observed_at),
                accepted_at,
                payload,
            },
        })
    })
    .collect()
}

fn read_changes_between(
    connection: &Connection,
    from: GenerationId,
    to: GenerationId,
) -> Result<Vec<GenerationChange>, StoreError> {
    let from_sequence = history_sequence(connection, from)?;
    let to_sequence = history_sequence(connection, to)?;
    if from_sequence >= to_sequence {
        return Err(StoreError::InvalidDelta(
            "change range must move forward between retained generations".to_owned(),
        ));
    }
    let entries = read_change_history_rows(connection, from_sequence, to_sequence, i64::MAX)?;
    let mut parent = from;
    let mut expected_sequence = from_sequence
        .checked_add(1)
        .ok_or_else(|| StoreError::Integrity("generation sequence overflow".to_owned()))?;
    let mut changes = Vec::with_capacity(entries.len());
    for (sequence, entry) in entries {
        if sequence != expected_sequence || entry.manifest.parent != Some(parent) {
            return Err(StoreError::Integrity(
                "change range is not a contiguous generation chain".to_owned(),
            ));
        }
        let delta = entry.delta.ok_or_else(|| {
            StoreError::Integrity("change range crosses a non-transition anchor".to_owned())
        })?;
        parent = entry.manifest.generation;
        expected_sequence = expected_sequence
            .checked_add(1)
            .ok_or_else(|| StoreError::Integrity("generation sequence overflow".to_owned()))?;
        changes.push(GenerationChange {
            sequence: u64::try_from(sequence).map_err(|error| {
                StoreError::Integrity(format!("invalid generation sequence: {error}"))
            })?,
            manifest: entry.manifest,
            delta,
        });
    }
    if parent != to {
        return Err(StoreError::Integrity(
            "change range did not reach its requested end generation".to_owned(),
        ));
    }
    Ok(changes)
}

fn read_changes_between_page(
    connection: &Connection,
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
    let from_sequence = history_sequence(connection, from)?;
    let to_sequence = history_sequence(connection, to)?;
    if from_sequence >= to_sequence {
        return Err(StoreError::InvalidDelta(
            "change range must move forward between retained generations".to_owned(),
        ));
    }
    let (after_sequence, mut parent) = match after {
        Some(cursor) if cursor.from_generation == from && cursor.to_generation == to => {
            let cursor_sequence = history_sequence(connection, cursor.after_generation)?;
            let stored_cursor_sequence = u64::try_from(cursor_sequence).map_err(|error| {
                StoreError::Integrity(format!("invalid generation sequence: {error}"))
            })?;
            if stored_cursor_sequence != cursor.after_sequence
                || cursor_sequence < from_sequence
                || cursor_sequence > to_sequence
            {
                return Err(StoreError::InvalidDelta(
                    "generation-change cursor is outside its pinned range".to_owned(),
                ));
            }
            (cursor_sequence, cursor.after_generation)
        }
        Some(_) => {
            return Err(StoreError::InvalidDelta(
                "generation-change cursor is bound to a different range".to_owned(),
            ));
        }
        None => (from_sequence, from),
    };
    let mut expected_sequence = after_sequence
        .checked_add(1)
        .ok_or_else(|| StoreError::Integrity("generation sequence overflow".to_owned()))?;
    let query_limit =
        i64::try_from(limit.saturating_add(1)).map_err(|_error| StoreError::InvalidPageLimit)?;
    let mut entries =
        read_change_history_rows(connection, after_sequence, to_sequence, query_limit)?;
    let has_more = entries.len() > limit;
    if has_more {
        entries.truncate(limit);
    }
    let mut items = Vec::with_capacity(entries.len());
    for (sequence, entry) in entries {
        if sequence != expected_sequence || entry.manifest.parent != Some(parent) {
            return Err(StoreError::Integrity(
                "change range is not a contiguous generation chain".to_owned(),
            ));
        }
        let delta = entry.delta.ok_or_else(|| {
            StoreError::Integrity("change range crosses a non-transition anchor".to_owned())
        })?;
        parent = entry.manifest.generation;
        expected_sequence = expected_sequence
            .checked_add(1)
            .ok_or_else(|| StoreError::Integrity("generation sequence overflow".to_owned()))?;
        items.push(GenerationChange {
            sequence: u64::try_from(sequence).map_err(|error| {
                StoreError::Integrity(format!("invalid generation sequence: {error}"))
            })?,
            manifest: entry.manifest,
            delta,
        });
    }
    if !has_more && parent != to {
        return Err(StoreError::Integrity(
            "change range did not reach its requested end generation".to_owned(),
        ));
    }
    let next_cursor = has_more
        .then(|| {
            items.last().map(|change| GenerationChangeCursor {
                from_generation: from,
                to_generation: to,
                after_sequence: change.sequence,
                after_generation: change.manifest.generation,
            })
        })
        .flatten();
    Ok(GenerationChangePage { items, next_cursor })
}

fn read_change_history_rows(
    connection: &Connection,
    after_sequence: i64,
    to_sequence: i64,
    limit: i64,
) -> Result<Vec<(i64, GenerationHistoryEntry)>, StoreError> {
    let mut statement = connection
        .prepare("SELECT sequence, payload FROM syntaxmesh_generation_history WHERE sequence > ?1 AND sequence <= ?2 ORDER BY sequence LIMIT ?3")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![after_sequence, to_sequence, limit], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(sql_error)?;
    rows.map(|row| {
        let (sequence, payload) = row.map_err(sql_error)?;
        Ok((sequence, decode(&payload)?))
    })
    .collect()
}

fn history_sequence(connection: &Connection, generation: GenerationId) -> Result<i64, StoreError> {
    connection
        .query_row(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_manifest(connection)?.map(|manifest| manifest.generation),
        })
}

fn read_many_with_params<T, P>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<T>, StoreError>
where
    T: DeserializeOwned,
    P: rusqlite::Params,
{
    let mut statement = connection.prepare(sql).map_err(sql_error)?;
    let rows = statement
        .query_map(params, |row| row.get::<_, Vec<u8>>(0))
        .map_err(sql_error)?;
    rows.map(|row| decode(&row.map_err(sql_error)?)).collect()
}

fn restore_store(connection: &Connection) -> Result<InMemoryGraphStore, StoreError> {
    let mut memory = InMemoryGraphStore::new();
    for (key, payload) in read_records(connection)? {
        memory.compare_exchange_record(&key, None, &payload)?;
    }
    let Some(manifest) = read_manifest(connection)? else {
        return Ok(memory);
    };
    let files = read_many(
        connection,
        "SELECT payload FROM syntaxmesh_files ORDER BY id",
    )?;
    let provenance = read_many(
        connection,
        "SELECT payload FROM syntaxmesh_provenance ORDER BY id",
    )?;
    let nodes = read_many(
        connection,
        "SELECT payload FROM syntaxmesh_nodes ORDER BY id",
    )?;
    let edges = read_many(
        connection,
        "SELECT payload FROM syntaxmesh_edges ORDER BY id",
    )?;
    let snapshot = GraphSnapshot {
        files,
        provenance,
        nodes,
        edges,
    };
    let anchor = GenerationHistoryEntry {
        manifest: manifest.clone(),
        delta: None,
        anchor: Some(snapshot),
    };
    memory.restore_history_anchor(&anchor)?;
    memory.restore_generation_history(vec![anchor])?;
    memory.set_generation_status(manifest.generation, manifest.status)?;
    let mut history = read_many(
        connection,
        "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence",
    )?;
    if history.is_empty() {
        history.push(GenerationHistoryEntry {
            manifest: manifest.clone(),
            delta: None,
            anchor: Some(GraphSnapshot {
                files: memory.files(manifest.generation)?,
                provenance: memory.provenance(manifest.generation)?,
                nodes: memory.nodes(manifest.generation)?,
                edges: memory.edges(manifest.generation)?,
            }),
        });
    }
    memory.restore_generation_history(history)?;
    let lineage_history = if connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'syntaxmesh_generation_lineage')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?
    {
        read_many(
            connection,
            "SELECT payload FROM syntaxmesh_generation_lineage ORDER BY sequence",
        )?
    } else {
        Vec::new()
    };
    memory.restore_generation_lineage_history(lineage_history)?;
    let consequence_history = if connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'syntaxmesh_generation_consequences')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?
    {
        read_many(
            connection,
            "SELECT payload FROM syntaxmesh_generation_consequences ORDER BY sequence",
        )?
    } else {
        Vec::new()
    };
    memory.restore_generation_consequence_history(consequence_history)?;
    Ok(memory)
}

fn read_manifest(connection: &Connection) -> Result<Option<GenerationManifest>, StoreError> {
    connection
        .query_row(
            "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|payload| decode(&payload))
        .transpose()
}

fn read_many<T>(connection: &Connection, sql: &str) -> Result<Vec<T>, StoreError>
where
    T: DeserializeOwned,
{
    let mut statement = connection.prepare(sql).map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(sql_error)?;
    let mut values = Vec::new();
    for row in rows {
        values.push(decode(&row.map_err(sql_error)?)?);
    }
    Ok(values)
}

fn read_records(connection: &Connection) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
    let mut statement = connection
        .prepare("SELECT record_key, payload FROM syntaxmesh_records ORDER BY record_key")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(sql_error)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(sql_error)?);
    }
    Ok(records)
}

fn encode<T>(value: &T) -> Result<Vec<u8>, StoreError>
where
    T: serde::Serialize,
{
    bincode::serialize(value)
        .map_err(|error| StoreError::Backend(format!("encode SQLite payload: {error}")))
}

fn encode_tree_fact<T>(value: &T, schema_version: u32) -> Result<Vec<u8>, StoreError>
where
    T: serde::Serialize,
{
    match schema_version {
        1 => encode(value),
        2 => {
            const COMPACT_TREE_FACT_MAGIC: &[u8; 4] = b"SMB1";
            let mut payload = COMPACT_TREE_FACT_MAGIC.to_vec();
            payload.extend_from_slice(&encode(value)?);
            Ok(payload)
        }
        version => Err(StoreError::Unsupported(format!(
            "unsupported persistent tree value schema version {version}"
        ))),
    }
}

fn decode<T>(payload: &[u8]) -> Result<T, StoreError>
where
    T: DeserializeOwned,
{
    bincode::deserialize(payload)
        .map_err(|error| StoreError::Integrity(format!("decode SQLite payload: {error}")))
}

fn decode_tree_fact<T>(payload: &[u8], schema_version: u32) -> Result<T, StoreError>
where
    T: DeserializeOwned,
{
    match schema_version {
        1 => decode(payload),
        2 => {
            const COMPACT_TREE_FACT_MAGIC: &[u8; 4] = b"SMB1";
            payload.strip_prefix(COMPACT_TREE_FACT_MAGIC).map_or_else(
                || {
                    serde_json::from_slice(payload).map_err(|error| {
                        StoreError::Integrity(format!("decode canonical tree fact: {error}"))
                    })
                },
                decode,
            )
        }
        version => Err(StoreError::Unsupported(format!(
            "unsupported persistent tree value schema version {version}"
        ))),
    }
}

fn sql_error(error: rusqlite::Error) -> StoreError {
    let message = error.to_string();
    drop(error);
    StoreError::Backend(format!("SQLite operation failed: {message}"))
}
