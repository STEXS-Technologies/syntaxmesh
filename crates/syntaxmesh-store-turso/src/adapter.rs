//! Synchronous graph-store port over Turso's async local API.

mod file_pages;
mod history_schema_cache;
mod index_store;
mod integrity;
mod migration_steps;
mod paths;
mod record_store;
mod temporal_tree;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
#[cfg(feature = "benchmark-instrumentation")]
use std::time::{Duration, Instant};

#[cfg(test)]
use crate::migrations::MigrationKind;
use crate::migrations::{SCHEMA_MIGRATIONS, SchemaMigration};
use index_store::history_sequence_for_generation;
use migration_steps::apply_schema_migration;
#[cfg(test)]
use migration_steps::{migrate_v2_to_v3, migrate_v3_to_v4, migrate_v4_to_v5, migrate_v18_to_v19};
#[cfg(test)]
use record_store::prefix_upper_bound;
use syntaxmesh_core::{
    AcceptanceTime, ChangeEvent, ChangeEventCursor, ChangeSetDelta, ChangeSetEvent, ChangeSetId,
    ChangeSetMembership, ConsequenceDelta, ConsequenceDerivation, ConsequenceEdge,
    ConsequenceEdgeCursor, ConsequenceEdgeVersion, ConsequenceRangeCursor, Edge, EdgeDirection,
    EdgeId, EventsForChangeSetCursor, FactPayload, FactRef, FactVersionRef, FileId,
    GenerationConsequenceEntry, GenerationHistoryEntry, GenerationId, GenerationLineageEntry,
    GenerationManifest, GenerationStatus, GraphDelta, GraphDeltaWithConsequences,
    GraphDeltaWithLineage, GraphSnapshot, LineageEndpoint, Node, NodeId, ObservationTime,
    ObservedFactCursor, Provenance, ProvenanceId, RepositoryId, StableId, WorktreeId,
};
#[cfg(test)]
use syntaxmesh_store::DurableRecordStore;
use syntaxmesh_store::{
    AcceptedGeneration, AcceptedGenerationCursor, AcceptedGenerationPage, BackendIntegrityCheck,
    BackendIntegrityKind, BackendIntegrityReport, ChangeEventPage, ChangeSetVersion,
    ConsequenceEdgePage, ConsequenceRangePage, EventsForChangeSetPage, FactHistoryCursor,
    FactHistoryEntry, FactHistoryPage, FactHistoryVersion, FactVersionChangeCursor,
    FactVersionChangePage, GenerationChange, GenerationChangeCursor, GenerationChangePage,
    GraphStore, HistoricalEdgePage, MAX_FACT_VERSION_CHANGE_PAGE_SIZE,
    MAX_GENERATION_CHANGE_PAGE_SIZE, NodeHistoryVersion, ObservedFactPage, ObservedFactVersion,
    StoreError, generation_root_v2, graph_snapshot_root_v2, node_kind_storage_code,
};
use temporal_tree::{
    current_projection_cascade_edge_ids, delta_incidence_changes, delta_tree_mutations,
    historical_incident_edge_page, merge_persistent_mutations, publish_incidence_root,
    publish_persistent_root, publish_rebuilt_persistent_root, read_graph_at, read_incidence_root,
    read_parent_sequence, read_persistent_snapshot, read_temporal_node, read_temporal_snapshot,
    snapshot_incidence_changes, snapshot_root_for_schema, snapshot_tree_mutations,
};

const SCHEMA_VERSION: i64 = 24;
const CHECKPOINT_INTERVAL: i64 = 64;
const CONSEQUENCE_RANGE_QUERY: &str = "WITH bounds AS (SELECT first_generation.sequence AS from_sequence, last_generation.sequence AS until_sequence FROM syntaxmesh_generation_history AS first_generation JOIN syntaxmesh_generation_history AS last_generation ON 1 = 1 WHERE first_generation.generation = ?1 AND last_generation.generation = ?2) SELECT bounds.from_sequence, bounds.until_sequence, edges.payload, start.generation, finish.generation FROM bounds LEFT JOIN syntaxmesh_consequence_edge_versions AS edges ON bounds.from_sequence <= bounds.until_sequence AND edges.valid_from_sequence <= bounds.until_sequence AND (edges.valid_until_sequence IS NULL OR edges.valid_until_sequence > bounds.from_sequence) AND ((edges.source_kind = ?3 AND edges.source_id = ?4 AND edges.source_fact_kind IS ?5 AND edges.source_valid_from_generation IS ?6) OR (edges.target_kind = ?7 AND edges.target_id = ?8 AND edges.target_fact_kind IS ?9 AND edges.target_valid_from_generation IS ?10)) AND (?11 IS NULL OR edges.edge_id > ?11) LEFT JOIN syntaxmesh_generation_history AS start ON start.sequence = edges.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS finish ON finish.sequence = edges.valid_until_sequence ORDER BY edges.edge_id LIMIT ?12";
const FACT_HISTORY_FIRST_PAGE_QUERY: &str = "WITH bounds AS (SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1) SELECT versions.fact_kind, versions.fact_id, start_entry.payload, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN end_entry.payload ELSE NULL END, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos, versions.valid_from_sequence, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN versions.valid_until_sequence ELSE NULL END FROM bounds JOIN syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx ON versions.valid_from_sequence <= bounds.sequence JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation ORDER BY versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?2";
const FACT_HISTORY_AFTER_PAGE_QUERY: &str = "WITH bounds AS (SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1) SELECT versions.fact_kind, versions.fact_id, start_entry.payload, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN end_entry.payload ELSE NULL END, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos, versions.valid_from_sequence, CASE WHEN versions.valid_until_sequence <= bounds.sequence THEN versions.valid_until_sequence ELSE NULL END FROM bounds JOIN syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx ON versions.valid_from_sequence <= bounds.sequence AND (versions.fact_kind, versions.fact_id, versions.valid_from_sequence) > (?2, ?3, ?4) JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation ORDER BY versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?5";
const FACT_VERSION_CHANGE_PAGE_QUERY: &str = "SELECT versions.fact_kind, versions.fact_id, versions.valid_from_sequence, versions.valid_until_sequence, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM (SELECT * FROM (SELECT fact_kind, fact_id, valid_from_sequence, valid_until_sequence, payload, observed_at_unix_nanos FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_start_generation_idx WHERE valid_from_sequence = ?1 AND (?2 < 0 OR fact_kind > ?2 OR (fact_kind = ?2 AND fact_id > ?3) OR (fact_kind = ?2 AND fact_id = ?3 AND valid_from_sequence > ?4)) ORDER BY fact_kind, fact_id, valid_from_sequence LIMIT ?5) UNION ALL SELECT * FROM (SELECT fact_kind, fact_id, valid_from_sequence, valid_until_sequence, payload, observed_at_unix_nanos FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_end_generation_idx WHERE valid_until_sequence = ?1 AND valid_from_sequence <> ?1 AND (?2 < 0 OR fact_kind > ?2 OR (fact_kind = ?2 AND fact_id > ?3) OR (fact_kind = ?2 AND fact_id = ?3 AND valid_from_sequence > ?4)) ORDER BY fact_kind, fact_id, valid_from_sequence LIMIT ?5)) AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation ORDER BY versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?5";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS syntaxmesh_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_manifest (id INTEGER PRIMARY KEY CHECK (id = 1), payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_files (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_provenance (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_nodes (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL, name TEXT NOT NULL, owner_file BLOB, terminal_name TEXT NOT NULL, provenance BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_edges (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL, source BLOB NOT NULL, target BLOB NOT NULL, provenance BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_records (record_key TEXT PRIMARY KEY NOT NULL, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_generation_history (sequence INTEGER PRIMARY KEY AUTOINCREMENT, generation BLOB NOT NULL UNIQUE, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_generation_acceptance (generation BLOB PRIMARY KEY NOT NULL, accepted_at_unix_nanos BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_fact_versions (fact_kind INTEGER NOT NULL, fact_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, observed_at_unix_nanos BLOB, source_id BLOB, target_id BLOB, payload BLOB NOT NULL, PRIMARY KEY (fact_kind, fact_id, valid_from_sequence)); CREATE TABLE IF NOT EXISTS syntaxmesh_graph_checkpoints (sequence INTEGER PRIMARY KEY NOT NULL, generation BLOB NOT NULL UNIQUE, manifest BLOB NOT NULL, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_temporal_tree_pages (page_id BLOB PRIMARY KEY NOT NULL, left_page BLOB, right_page BLOB, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_temporal_roots (sequence INTEGER PRIMARY KEY NOT NULL, generation BLOB NOT NULL UNIQUE, root_id BLOB); CREATE TABLE IF NOT EXISTS syntaxmesh_temporal_incidence_roots (sequence INTEGER PRIMARY KEY NOT NULL, generation BLOB NOT NULL UNIQUE, root_id BLOB); CREATE TABLE IF NOT EXISTS syntaxmesh_change_events (generation BLOB PRIMARY KEY NOT NULL, event_id BLOB NOT NULL UNIQUE, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_change_event_facts (generation BLOB NOT NULL, generation_sequence INTEGER NOT NULL, fact_kind INTEGER NOT NULL, fact_id BLOB NOT NULL, change_kind INTEGER NOT NULL, PRIMARY KEY (generation, fact_kind, fact_id)); CREATE TABLE IF NOT EXISTS syntaxmesh_generation_lineage (sequence INTEGER PRIMARY KEY NOT NULL, generation BLOB NOT NULL UNIQUE, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_change_set_versions (change_set_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, payload BLOB NOT NULL, PRIMARY KEY (change_set_id, valid_from_sequence)); CREATE TABLE IF NOT EXISTS syntaxmesh_change_set_membership_versions (change_set_id BLOB NOT NULL, event_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, provenance_id BLOB NOT NULL, PRIMARY KEY (change_set_id, event_id, valid_from_sequence))";

const FILE_FACT: i64 = 0;
const PROVENANCE_FACT: i64 = 1;
const NODE_FACT: i64 = 2;
const EDGE_FACT: i64 = 3;
const EDGE_CLOSE_BATCH_SIZE: usize = 128;
const CURRENT_PROJECTION_BATCH_SIZE: usize = 100;

#[cfg(feature = "benchmark-instrumentation")]
struct TransactionProfile {
    enabled: bool,
    stage_started: Instant,
}

#[cfg(feature = "benchmark-instrumentation")]
impl TransactionProfile {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("SYNTAXMESH_TURSO_PROFILE").is_some(),
            stage_started: Instant::now(),
        }
    }

    fn mark(&mut self, stage: &str) {
        if self.enabled {
            eprintln!(
                "turso_stage stage={stage} elapsed_us={}",
                self.stage_started.elapsed().as_micros()
            );
            self.stage_started = Instant::now();
        }
    }

    fn report_elapsed(&self, stage: &str, elapsed: Duration) {
        if self.enabled {
            eprintln!(
                "turso_stage stage={stage} elapsed_us={}",
                elapsed.as_micros()
            );
        }
    }
}

/// Errors raised by the Turso adapter.
#[derive(Debug)]
pub enum TursoStoreError {
    /// Turso or runtime failure.
    Backend(String),
    /// Graph-store validation or stale-generation failure.
    Store(StoreError),
    /// Canonical-row serialization or integrity failure.
    Snapshot(String),
}

impl std::fmt::Display for TursoStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TursoStoreError {}

impl From<StoreError> for TursoStoreError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Membership indexes reused while validating facts against one pending delta.
/// Without these sets each edge endpoint check scans the complete upsert list.
pub(super) struct DeltaLookupIndex {
    upserted_nodes: BTreeSet<NodeId>,
    removed_nodes: BTreeSet<NodeId>,
    upserted_provenance: BTreeSet<ProvenanceId>,
}

impl DeltaLookupIndex {
    fn new(delta: &GraphDelta) -> Self {
        Self {
            upserted_nodes: delta.upsert_nodes.iter().map(|node| node.id).collect(),
            removed_nodes: delta.remove_nodes.iter().copied().collect(),
            upserted_provenance: delta.upsert_provenance.iter().map(|item| item.id).collect(),
        }
    }
}

/// Read-only view of the registered Turso schema migration path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TursoMigrationStatus {
    /// Schema version currently recorded by the database, if initialized.
    pub current_version: Option<i64>,
    /// Latest schema version supported by this adapter.
    pub target_version: i64,
    /// Whether the persisted ledger matches the registered migration path.
    pub migration_ledger_validated: bool,
    /// Registered transitions in upgrade order.
    pub migrations: Vec<TursoMigrationEntry>,
}

/// One registered Turso schema transition and whether it has been applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TursoMigrationEntry {
    /// Schema version before the transition.
    pub from: i64,
    /// Schema version after the transition.
    pub to: i64,
    /// Stable migration identifier.
    pub name: &'static str,
    /// Whether the current database version includes this transition.
    pub applied: bool,
}

/// Turso-backed implementation of the canonical graph-store port.
///
/// The adapter owns a current-thread runtime because Turso's published API is
/// asynchronous. Core, indexer, query, and engine crates remain runtime
/// agnostic; hosts that already own an async runtime should use the adapter at
/// a synchronous composition boundary.
pub struct TursoGraphStore {
    runtime: tokio::runtime::Runtime,
    connection: turso::Connection,
    last_manifest: Option<GenerationManifest>,
    history_schema_cache: history_schema_cache::HistorySchemaCache,
}

impl TursoGraphStore {
    fn database_manifest(&self) -> Result<Option<GenerationManifest>, StoreError> {
        self.runtime
            .block_on(read_single::<GenerationManifest>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
            ))
            .map_err(Self::map_backend)
    }

    fn require_database_generation(
        &self,
        generation: GenerationId,
    ) -> Result<GenerationManifest, StoreError> {
        let manifest = self.database_manifest()?;
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

    /// Opens or creates a local Turso database and applies pending migrations.
    ///
    /// # Errors
    /// Returns an adapter error when Turso cannot initialize or migrate the
    /// database. Call this before [`Self::open`] for a new or stale database.
    pub fn migrate(path: impl AsRef<Path>) -> Result<(), TursoStoreError> {
        let initialized = Self::open_and_migrate(path)?;
        drop(initialized);
        Ok(())
    }

    /// Inspects registered schema migration status without modifying the database.
    ///
    /// A missing or empty path reports an uninitialized database. An existing
    /// application database without a SyntaxMesh version, or an unsupported
    /// version, fails closed.
    ///
    /// # Errors
    /// Returns an adapter error when the database cannot be inspected or has
    /// an unsupported/inconsistent schema version.
    pub fn migration_status(
        path: impl AsRef<Path>,
    ) -> Result<TursoMigrationStatus, TursoStoreError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(migration_status_report(None, false));
        }
        if std::fs::metadata(path)
            .map_err(|error| TursoStoreError::Backend(format!("inspect Turso database: {error}")))?
            .len()
            == 0
        {
            return Ok(migration_status_report(None, false));
        }
        let resolved = paths::database_path(path)?;
        let path_text = resolved
            .to_str()
            .ok_or_else(|| TursoStoreError::Backend("database path is not UTF-8".to_owned()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| TursoStoreError::Backend(format!("build runtime: {error}")))?;
        let database = runtime
            .block_on(turso::Builder::new_local(path_text).read_only(true).build())
            .map_err(|error| {
                TursoStoreError::Backend(format!("open Turso database read-only: {error}"))
            })?;
        let connection = database.connect().map_err(|error| {
            TursoStoreError::Backend(format!("connect Turso database read-only: {error}"))
        })?;
        let current_version = runtime.block_on(async {
            let has_schema = table_exists(&connection, "syntaxmesh_schema").await?;
            if !has_schema {
                let has_application_tables = application_tables_exist(&connection).await?;
                if has_application_tables {
                    return Err(TursoStoreError::Snapshot(
                        "Turso database has application tables but no SyntaxMesh schema version".to_owned(),
                    ));
                }
                return Ok(None);
            }
            let version = read_schema_version(&connection).await?;
            if !(1..=SCHEMA_VERSION).contains(&version) {
                return Err(TursoStoreError::Snapshot(format!(
                    "unsupported Turso schema version {version}; registered range is 1..={SCHEMA_VERSION}"
                )));
            }
            let ledger_validated = if version >= 17 {
                validate_schema_migrations(&connection, version).await?;
                true
            } else {
                false
            };
            Ok(Some((version, ledger_validated)))
        })?;
        let (current_version, migration_ledger_validated) =
            current_version.map_or((None, false), |(version, ledger)| (Some(version), ledger));
        Ok(migration_status_report(
            current_version,
            migration_ledger_validated,
        ))
    }

    /// Opens an existing current-schema database and validates canonical rows.
    ///
    /// Opening never creates, migrates, repairs, or backfills the database.
    /// Run [`Self::migrate`] explicitly for a new or stale database.
    ///
    /// # Errors
    /// Returns an adapter error when the schema, row projections, history
    /// indexes, or canonical graph root are missing or inconsistent.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, TursoStoreError> {
        let path_ref = path.as_ref();
        if !path_ref.exists() {
            return Err(TursoStoreError::Snapshot(
                "Turso database is missing; run TursoGraphStore::migrate before open".to_owned(),
            ));
        }
        let resolved = paths::database_path(path_ref)?;
        let path_text = resolved
            .to_str()
            .ok_or_else(|| TursoStoreError::Backend("database path is not UTF-8".to_owned()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| TursoStoreError::Backend(format!("build runtime: {error}")))?;
        let database = runtime
            .block_on(turso::Builder::new_local(path_text).build())
            .map_err(|error| TursoStoreError::Backend(format!("open Turso database: {error}")))?;
        let connection = database.connect().map_err(|error| {
            TursoStoreError::Backend(format!("connect Turso database: {error}"))
        })?;
        let last_manifest = runtime.block_on(async {
            validate_wal_mode(&connection).await?;
            let version = read_schema_version(&connection).await?;
            if version != SCHEMA_VERSION {
                return Err(TursoStoreError::Snapshot(format!(
                    "Turso schema version {version} is not current (expected {SCHEMA_VERSION}); run TursoGraphStore::migrate before open"
                )));
            }
            validate_schema_migrations(&connection, version).await?;
            integrity::validate_node_kind_column(&connection).await?;
            integrity::validate_index_columns(&connection).await?;
            integrity::validate_canonical_state(&connection).await?;
            let manifest = read_single::<GenerationManifest>(
                &connection,
                "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
            )
            .await?;
            let latest_history = read_single::<GenerationHistoryEntry>(
                &connection,
                "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence DESC LIMIT 1",
            )
            .await?;
            if latest_history.as_ref().zip(manifest.as_ref()).is_some_and(|(entry, current)| {
                entry.manifest.generation != current.generation
                    || entry.manifest.repository != current.repository
                    || entry.manifest.worktree != current.worktree
                    || entry.manifest.graph_root != current.graph_root
            }) || (latest_history.is_none() != manifest.is_none()) {
                return Err(TursoStoreError::Snapshot(
                    "Turso latest generation history does not match current manifest".to_owned(),
                ));
            }
            if let Some(manifest) = &manifest {
                read_incidence_root(&connection, manifest.generation).await?;
            }
            Ok(manifest)
        })?;
        Ok(Self {
            runtime,
            connection,
            last_manifest,
            history_schema_cache: history_schema_cache::HistorySchemaCache::default(),
        })
    }

    fn open_and_migrate(path: impl AsRef<Path>) -> Result<Self, TursoStoreError> {
        let resolved = paths::database_path(path.as_ref())?;
        let path_text = resolved
            .to_str()
            .ok_or_else(|| TursoStoreError::Backend("database path is not UTF-8".to_owned()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| TursoStoreError::Backend(format!("build runtime: {error}")))?;
        let database = runtime
            .block_on(turso::Builder::new_local(path_text).build())
            .map_err(|error| TursoStoreError::Backend(format!("open Turso database: {error}")))?;
        let mut connection = database.connect().map_err(|error| {
            TursoStoreError::Backend(format!("connect Turso database: {error}"))
        })?;
        let fresh_schema = runtime.block_on(async {
            if table_exists(&connection, "syntaxmesh_schema").await? {
                let version = read_schema_version(&connection).await?;
                if !(1..=SCHEMA_VERSION).contains(&version) {
                    return Err(TursoStoreError::Snapshot(format!(
                        "unsupported Turso schema version {version}; registered range is 1..={SCHEMA_VERSION}"
                    )));
                }
                Ok(false)
            } else if application_tables_exist(&connection).await? {
                Err(TursoStoreError::Snapshot(
                    "Turso database has application tables but no SyntaxMesh schema version".to_owned(),
                ))
            } else {
                Ok(true)
            }
        })?;
        let journal_mode = runtime
            .block_on(connection.pragma_update("journal_mode", "'wal'"))
            .map_err(|error| TursoStoreError::Backend(format!("enable Turso WAL: {error}")))?;
        let mode = journal_mode
            .first()
            .ok_or_else(|| TursoStoreError::Backend("Turso returned no journal mode".to_owned()))?
            .get_value(0)
            .map_err(|error| {
                TursoStoreError::Backend(format!("read Turso journal mode: {error}"))
            })?;
        if mode != turso::Value::Text("wal".to_owned()) {
            return Err(TursoStoreError::Backend(format!(
                "Turso did not enter WAL mode: {mode:?}"
            )));
        }
        if fresh_schema {
            let transaction =
                runtime
                    .block_on(connection.transaction_with_behavior(
                        turso::transaction::TransactionBehavior::Immediate,
                    ))
                    .map_err(|error| {
                        TursoStoreError::Backend(format!("begin Turso schema bootstrap: {error}"))
                    })?;
            runtime
                .block_on(transaction.execute_batch(SCHEMA))
                .map_err(|error| {
                    TursoStoreError::Backend(format!("initialize Turso schema: {error}"))
                })?;
            runtime
                .block_on(transaction.execute(
                    "INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 14)",
                    (),
                ))
                .map_err(|error| {
                    TursoStoreError::Backend(format!(
                        "record initial Turso schema version: {error}"
                    ))
                })?;
            runtime.block_on(transaction.commit()).map_err(|error| {
                TursoStoreError::Backend(format!("commit Turso schema bootstrap: {error}"))
            })?;
        } else {
            runtime
                .block_on(connection.execute_batch(SCHEMA))
                .map_err(|error| {
                    TursoStoreError::Backend(format!("prepare Turso schema for upgrade: {error}"))
                })?;
        }
        runtime.block_on(async {
            let mut schema_version = read_schema_version(&connection).await?;
            while schema_version < SCHEMA_VERSION {
                let migration = SCHEMA_MIGRATIONS
                    .iter()
                    .find(|migration| migration.from == schema_version)
                    .ok_or_else(|| {
                        TursoStoreError::Snapshot(format!(
                            "no registered Turso migration from schema {schema_version}"
                        ))
                    })?;
                apply_schema_migration(&mut connection, migration).await?;
                schema_version = read_schema_version(&connection).await?;
                if schema_version != migration.to {
                    return Err(TursoStoreError::Snapshot(format!(
                        "Turso migration {} did not advance schema to {} (observed {schema_version})",
                        migration.name, migration.to
                    )));
                }
            }
            if schema_version != SCHEMA_VERSION {
                return Err(TursoStoreError::Snapshot(format!(
                    "unsupported Turso schema version {schema_version}; expected {SCHEMA_VERSION}"
                )));
            }
            connection.execute_batch("CREATE INDEX IF NOT EXISTS syntaxmesh_nodes_name_idx ON syntaxmesh_nodes(name, id); CREATE INDEX IF NOT EXISTS syntaxmesh_nodes_terminal_name_idx ON syntaxmesh_nodes(terminal_name, id); CREATE INDEX IF NOT EXISTS syntaxmesh_nodes_owner_file_idx ON syntaxmesh_nodes(owner_file, id); CREATE INDEX IF NOT EXISTS syntaxmesh_nodes_provenance_idx ON syntaxmesh_nodes(provenance, id); CREATE INDEX IF NOT EXISTS syntaxmesh_edges_source_idx ON syntaxmesh_edges(source, id); CREATE INDEX IF NOT EXISTS syntaxmesh_edges_target_idx ON syntaxmesh_edges(target, id); CREATE INDEX IF NOT EXISTS syntaxmesh_edges_provenance_idx ON syntaxmesh_edges(provenance, id); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_identity_time_idx ON syntaxmesh_fact_versions(fact_kind, fact_id, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_time_idx ON syntaxmesh_fact_versions(fact_kind, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_observed_time_idx ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id, valid_from_sequence) WHERE observed_at_unix_nanos IS NOT NULL; CREATE INDEX IF NOT EXISTS syntaxmesh_generation_acceptance_time_idx ON syntaxmesh_generation_acceptance(accepted_at_unix_nanos, generation);").await.map_err(|error| TursoStoreError::Backend(format!("create Turso read indexes: {error}")))?;
            connection.execute_batch("CREATE INDEX IF NOT EXISTS syntaxmesh_change_event_fact_lookup_idx ON syntaxmesh_change_event_facts(fact_kind, fact_id, generation_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_change_events_generation_idx ON syntaxmesh_change_events(generation); CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_validity_idx ON syntaxmesh_change_set_versions(change_set_id, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_membership_set_idx ON syntaxmesh_change_set_membership_versions(change_set_id, valid_from_sequence, valid_until_sequence, event_id); CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_membership_event_idx ON syntaxmesh_change_set_membership_versions(event_id, valid_from_sequence, valid_until_sequence, change_set_id);").await.map_err(|error| TursoStoreError::Backend(format!("create Turso lineage indexes: {error}")))?;
            integrity::validate_index_columns(&connection).await?;
            integrity::validate_canonical_state(&connection).await?;
            Ok(())
        })?;
        let last_manifest = runtime
            .block_on(read_single::<GenerationManifest>(
                &connection,
                "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
            ))
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        runtime.block_on(async {
            let history = read_many::<GenerationHistoryEntry>(&connection, "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence").await?;
            if history.is_empty() && let Some(manifest) = &last_manifest {
                let transaction = connection
                    .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
                    .await
                    .map_err(|error| TursoStoreError::Backend(format!("begin legacy history anchor: {error}")))?;
                let snapshot = read_canonical_snapshot(&transaction).await?;
                let anchor = GenerationHistoryEntry {
                    manifest: manifest.clone(),
                    delta: None,
                    anchor: Some(snapshot.clone()),
                };
                transaction.execute("INSERT INTO syntaxmesh_generation_history (sequence, generation, payload) VALUES (1, ?1, ?2)", (manifest.generation.0.0.to_vec(), bincode::serialize(&anchor).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?)).await.map_err(|error| TursoStoreError::Backend(format!("write generation history anchor: {error}")))?;
                seed_temporal_snapshot(&transaction, &snapshot, 1).await?;
                let mutations = snapshot_tree_mutations(&snapshot, manifest.schema_version)?;
                publish_persistent_root(
                    &transaction,
                    1,
                    manifest.generation,
                    None,
                    &mutations,
                )
                .await?;
                publish_incidence_root(
                    &transaction,
                    1,
                    manifest.generation,
                    None,
                    &snapshot_incidence_changes(&snapshot),
                )
                .await?;
                transaction.execute("INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (1, ?1, ?2, ?3)", (manifest.generation.0.0.to_vec(), bincode::serialize(manifest).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?, bincode::serialize(&snapshot).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?)).await.map_err(|error| TursoStoreError::Backend(format!("write initial graph checkpoint: {error}")))?;
                transaction
                    .commit()
                    .await
                    .map_err(|error| TursoStoreError::Backend(format!("commit legacy history anchor: {error}")))?;
            }
            let retained = read_many::<GenerationHistoryEntry>(&connection, "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence").await?;
            backfill_change_events(&mut connection, &retained).await?;
            Ok::<(), TursoStoreError>(())
        })?;
        Ok(Self {
            runtime,
            connection,
            last_manifest,
            history_schema_cache: history_schema_cache::HistorySchemaCache::default(),
        })
    }

    fn apply_database_delta(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, TursoStoreError> {
        self.apply_database_delta_with_lineage(delta, accepted_at, ChangeSetDelta::default())
    }

    fn apply_database_delta_with_lineage(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
        lineage: ChangeSetDelta,
    ) -> Result<GenerationManifest, TursoStoreError> {
        self.apply_database_delta_with_consequences(
            delta,
            accepted_at,
            lineage,
            ConsequenceDelta::default(),
        )
    }

    fn apply_database_delta_with_consequences(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
        lineage: ChangeSetDelta,
        consequences: ConsequenceDelta,
    ) -> Result<GenerationManifest, TursoStoreError> {
        let expected = self.last_manifest.clone();
        let manifest = self.runtime.block_on(apply_database_delta(
            &mut self.connection,
            delta,
            expected,
            accepted_at,
            lineage,
            consequences,
        ))?;
        self.last_manifest = Some(manifest.clone());
        Ok(manifest)
    }

    fn map_backend(error: TursoStoreError) -> StoreError {
        match error {
            TursoStoreError::Store(error) => error,
            TursoStoreError::Backend(error) | TursoStoreError::Snapshot(error) => {
                StoreError::Backend(error)
            }
        }
    }
}

impl BackendIntegrityCheck for TursoGraphStore {
    fn backend_integrity_check(&self) -> Result<BackendIntegrityReport, StoreError> {
        let findings = match self
            .runtime
            .block_on(integrity::turso_integrity_findings(&self.connection))
        {
            Ok(findings) => findings,
            Err(error) => vec![error.to_string()],
        };
        Ok(BackendIntegrityReport::from_database_findings(
            BackendIntegrityKind::TursoDatabase,
            findings,
        ))
    }
}

async fn read_schema_version(connection: &turso::Connection) -> Result<i64, TursoStoreError> {
    let mut rows = connection
        .query("SELECT version FROM syntaxmesh_schema WHERE id = 1", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read Turso schema version: {error}")))?;
    let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read Turso schema row: {error}")))?
    else {
        return Ok(0);
    };
    row.get(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode Turso schema version: {error}")))
}

async fn validate_wal_mode(connection: &turso::Connection) -> Result<(), TursoStoreError> {
    let mut rows = connection
        .query("PRAGMA journal_mode", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read Turso journal mode: {error}")))?;
    let mode = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read Turso journal mode row: {error}")))?
        .ok_or_else(|| TursoStoreError::Snapshot("Turso journal mode is missing".to_owned()))?
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode Turso journal mode: {error}")))?;
    if mode != turso::Value::Text("wal".to_owned()) {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso database is not in WAL mode: {mode:?}; run TursoGraphStore::migrate"
        )));
    }
    Ok(())
}

fn migration_checksum(migration: &SchemaMigration) -> Option<String> {
    migration
        .checksum_sql
        .map(|sql| blake3::hash(sql.as_bytes()).to_hex().to_string())
}

async fn validate_schema_migrations(
    connection: &turso::Connection,
    schema_version: i64,
) -> Result<(), TursoStoreError> {
    if schema_version < 17 {
        return Ok(());
    }
    let mut rows = connection
        .query(
            "SELECT version, name, applied_at_unix_seconds, adopted, checksum FROM syntaxmesh_schema_migrations ORDER BY version",
            (),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration ledger: {error}")))?;
    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration ledger row: {error}")))?
    {
        records.push((
            row.get::<i64>(0).map_err(|error| {
                TursoStoreError::Backend(format!("decode ledger version: {error}"))
            })?,
            row.get::<String>(1).map_err(|error| {
                TursoStoreError::Backend(format!("decode ledger name: {error}"))
            })?,
            row.get::<Option<i64>>(2).map_err(|error| {
                TursoStoreError::Backend(format!("decode ledger timestamp: {error}"))
            })?,
            row.get::<i64>(3).map_err(|error| {
                TursoStoreError::Backend(format!("decode ledger adoption state: {error}"))
            })?,
            row.get::<Option<String>>(4).map_err(|error| {
                TursoStoreError::Backend(format!("decode ledger checksum: {error}"))
            })?,
        ));
    }
    let expected = SCHEMA_MIGRATIONS
        .iter()
        .filter(|migration| migration.to <= schema_version)
        .collect::<Vec<_>>();
    if records.len() != expected.len() {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso migration ledger contains {} entries; expected {}",
            records.len(),
            expected.len()
        )));
    }
    for ((version, name, applied_at, adopted, checksum), migration) in records.iter().zip(expected)
    {
        if *version != migration.to || name != migration.name {
            return Err(TursoStoreError::Snapshot(format!(
                "Turso migration ledger entry {version} ({name}) disagrees with registered migration {} ({})",
                migration.to, migration.name
            )));
        }
        if !matches!((*adopted, applied_at), (0, Some(_)) | (1, None)) {
            return Err(TursoStoreError::Snapshot(format!(
                "Turso migration {version} has inconsistent adopted/timestamp metadata"
            )));
        }
        if checksum != &migration_checksum(migration) {
            return Err(TursoStoreError::Snapshot(format!(
                "Turso migration checksum mismatch for version {version}"
            )));
        }
    }
    Ok(())
}

async fn table_exists(connection: &turso::Connection, name: &str) -> Result<bool, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [name],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("inspect Turso schema table: {error}"))
        })?;
    rows.next()
        .await
        .map(|row| row.is_some())
        .map_err(|error| TursoStoreError::Backend(format!("read Turso schema table: {error}")))
}

async fn application_tables_exist(connection: &turso::Connection) -> Result<bool, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' LIMIT 1",
            (),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("inspect Turso application tables: {error}"))
        })?;
    rows.next().await.map(|row| row.is_some()).map_err(|error| {
        TursoStoreError::Backend(format!("read Turso application tables: {error}"))
    })
}

fn migration_status_report(
    current_version: Option<i64>,
    migration_ledger_validated: bool,
) -> TursoMigrationStatus {
    let target_version = SCHEMA_MIGRATIONS
        .last()
        .map_or(SCHEMA_VERSION, |migration| migration.to);
    TursoMigrationStatus {
        current_version,
        target_version,
        migration_ledger_validated,
        migrations: SCHEMA_MIGRATIONS
            .iter()
            .map(|migration: &SchemaMigration| TursoMigrationEntry {
                from: migration.from,
                to: migration.to,
                name: migration.name,
                applied: current_version.is_some_and(|current| current >= migration.to),
            })
            .collect(),
    }
}

async fn insert_change_event(
    connection: &turso::Connection,
    event: &ChangeEvent,
) -> Result<(), TursoStoreError> {
    let sequence = history_sequence_for_generation(connection, event.generation_after).await?;
    connection
        .execute(
            "INSERT OR IGNORE INTO syntaxmesh_change_events (generation, event_id, payload) VALUES (?1, ?2, ?3)",
            (
                event.generation_after.0.0.to_vec(),
                event.id.0.0.to_vec(),
                bincode::serialize(event)
                    .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            ),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("persist change event: {error}")))?;
    for changed in &event.changed_facts {
        let (fact_kind, fact_id) = syntaxmesh_store::fact_storage_key(changed.fact);
        connection
            .execute(
                "INSERT OR IGNORE INTO syntaxmesh_change_event_facts (generation, generation_sequence, fact_kind, fact_id, change_kind) VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    event.generation_after.0.0.to_vec(),
                    sequence,
                    fact_kind,
                    fact_id,
                    syntaxmesh_store::fact_change_kind_code(changed.kind),
                ),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("index changed fact for event: {error}")))?;
    }
    Ok(())
}

async fn change_set_exists(
    connection: &turso::Connection,
    change_set: ChangeSetId,
) -> Result<bool, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM syntaxmesh_change_set_versions WHERE change_set_id = ?1 AND valid_until_sequence IS NULL LIMIT 1",
            [change_set.0.0.to_vec()],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("check ChangeSet identity: {error}")))?;
    Ok(rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read ChangeSet identity: {error}")))?
        .is_some())
}

async fn provenance_exists(
    connection: &turso::Connection,
    lookup: &DeltaLookupIndex,
    provenance: ProvenanceId,
) -> Result<bool, TursoStoreError> {
    if lookup.upserted_provenance.contains(&provenance) {
        return Ok(true);
    }
    let mut rows = connection
        .query(
            "SELECT 1 FROM syntaxmesh_provenance WHERE id = ?1 LIMIT 1",
            [provenance.0.0.to_vec()],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("check lineage provenance: {error}")))?;
    Ok(rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read lineage provenance: {error}")))?
        .is_some())
}

async fn node_exists_after_delta(
    connection: &turso::Connection,
    lookup: &DeltaLookupIndex,
    node: NodeId,
) -> Result<bool, TursoStoreError> {
    // Publication deletes first and upserts afterward, so a re-created node
    // present in both sets exists in the resulting generation.
    if lookup.upserted_nodes.contains(&node) {
        return Ok(true);
    }
    if lookup.removed_nodes.contains(&node) {
        return Ok(false);
    }
    let mut rows = connection
        .query(
            "SELECT 1 FROM syntaxmesh_nodes WHERE id = ?1 LIMIT 1",
            [node.0.0.to_vec()],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("check lineage node: {error}")))?;
    Ok(rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read lineage node: {error}")))?
        .is_some())
}

async fn validate_change_set_lineage_sql(
    connection: &turso::Connection,
    graph: &GraphDelta,
    lineage: &ChangeSetDelta,
    sequence: i64,
    event: &ChangeEvent,
) -> Result<(), TursoStoreError> {
    GraphDeltaWithLineage {
        graph: graph.clone(),
        lineage: lineage.clone(),
    }
    .validate()
    .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;

    let lookup = DeltaLookupIndex::new(graph);
    let mut sets = BTreeSet::new();
    for set in &lineage.upsert_sets {
        sets.insert(set.id);
        if !provenance_exists(connection, &lookup, set.provenance).await? {
            return Err(StoreError::InvalidDelta(format!(
                "change set {:?} references unknown provenance {:?}",
                set.id, set.provenance
            ))
            .into());
        }
        if let Some(intent) = set.originating_intent
            && !node_exists_after_delta(connection, &lookup, intent).await?
        {
            return Err(StoreError::InvalidDelta(format!(
                "change set {:?} references an unknown originating intent",
                set.id
            ))
            .into());
        }
        for adr in &set.adrs {
            if !node_exists_after_delta(connection, &lookup, *adr).await? {
                return Err(StoreError::InvalidDelta(format!(
                    "change set {:?} references an unknown ADR node",
                    set.id
                ))
                .into());
            }
        }
        let first = if set.first_generation == graph.next_generation {
            sequence
        } else {
            history_sequence_for_generation(connection, set.first_generation).await?
        };
        let last = if let Some(last) = set.last_generation {
            if last == graph.next_generation {
                sequence
            } else {
                history_sequence_for_generation(connection, last).await?
            }
        } else {
            sequence
        };
        if first > last {
            return Err(StoreError::InvalidDelta(format!(
                "change set {:?} has inverted generation bounds",
                set.id
            ))
            .into());
        }
    }
    for set in &lineage.upsert_sets {
        for parent in &set.parent_changes {
            if !sets.contains(parent) && !change_set_exists(connection, *parent).await? {
                return Err(StoreError::InvalidDelta(format!(
                    "change set {:?} references an unknown parent change set",
                    set.id
                ))
                .into());
            }
        }
    }
    for membership in &lineage.assign_events {
        if !sets.contains(&membership.change_set)
            && !change_set_exists(connection, membership.change_set).await?
        {
            return Err(StoreError::InvalidDelta(format!(
                "membership references unknown change set {:?}",
                membership.change_set
            ))
            .into());
        }
        let is_new_event = membership.event == event.id;
        if !is_new_event {
            let mut rows = connection
                .query(
                    "SELECT 1 FROM syntaxmesh_change_events WHERE event_id = ?1 LIMIT 1",
                    [membership.event.0.0.to_vec()],
                )
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("check membership event: {error}"))
                })?;
            if rows
                .next()
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("read membership event: {error}"))
                })?
                .is_none()
            {
                return Err(StoreError::InvalidDelta(format!(
                    "membership references unknown change event {:?}",
                    membership.event
                ))
                .into());
            }
        }
        if !provenance_exists(connection, &lookup, membership.provenance).await? {
            return Err(StoreError::InvalidDelta(format!(
                "membership references unknown provenance {:?}",
                membership.provenance
            ))
            .into());
        }
    }
    for removal in &lineage.unassign_events {
        let mut rows = connection
            .query(
                "SELECT 1 FROM syntaxmesh_change_set_membership_versions WHERE change_set_id = ?1 AND event_id = ?2 AND valid_until_sequence IS NULL LIMIT 1",
                (removal.key.change_set.0.0.to_vec(), removal.key.event.0.0.to_vec()),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("check active ChangeSet membership: {error}"))
            })?;
        if rows
            .next()
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read active ChangeSet membership: {error}"))
            })?
            .is_none()
        {
            return Err(StoreError::InvalidDelta(format!(
                "cannot remove inactive change-set membership {:?}",
                removal.key
            ))
            .into());
        }
        if !provenance_exists(connection, &lookup, removal.provenance).await? {
            return Err(StoreError::InvalidDelta(format!(
                "membership removal references unknown provenance {:?}",
                removal.provenance
            ))
            .into());
        }
    }
    Ok(())
}

async fn apply_change_set_lineage_projection(
    connection: &turso::Connection,
    delta: &ChangeSetDelta,
    sequence: i64,
) -> Result<(), TursoStoreError> {
    for set in &delta.upsert_sets {
        connection
            .execute(
                "UPDATE syntaxmesh_change_set_versions SET valid_until_sequence = ?1 WHERE change_set_id = ?2 AND valid_until_sequence IS NULL",
                (sequence, set.id.0.0.to_vec()),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("close ChangeSet version: {error}")))?;
        connection
            .execute(
                "INSERT INTO syntaxmesh_change_set_versions (change_set_id, valid_from_sequence, valid_until_sequence, payload) VALUES (?1, ?2, NULL, ?3)",
                (set.id.0.0.to_vec(), sequence, bincode::serialize(set).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("append ChangeSet version: {error}")))?;
    }
    for membership in &delta.assign_events {
        let mut rows = connection
            .query(
                "SELECT provenance_id FROM syntaxmesh_change_set_membership_versions WHERE change_set_id = ?1 AND event_id = ?2 AND valid_until_sequence IS NULL",
                (membership.change_set.0.0.to_vec(), membership.event.0.0.to_vec()),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("read active membership: {error}")))?;
        let same_provenance = if let Some(row) = rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read active membership provenance: {error}"))
        })? {
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?
                == turso::Value::Blob(membership.provenance.0.0.to_vec())
        } else {
            false
        };
        if same_provenance {
            continue;
        }
        connection
            .execute(
                "UPDATE syntaxmesh_change_set_membership_versions SET valid_until_sequence = ?1 WHERE change_set_id = ?2 AND event_id = ?3 AND valid_until_sequence IS NULL",
                (sequence, membership.change_set.0.0.to_vec(), membership.event.0.0.to_vec()),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("close membership version: {error}")))?;
        connection
            .execute(
                "INSERT INTO syntaxmesh_change_set_membership_versions (change_set_id, event_id, valid_from_sequence, valid_until_sequence, provenance_id) VALUES (?1, ?2, ?3, NULL, ?4)",
                (membership.change_set.0.0.to_vec(), membership.event.0.0.to_vec(), sequence, membership.provenance.0.0.to_vec()),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("append membership version: {error}")))?;
    }
    for removal in &delta.unassign_events {
        let changed = connection
            .execute(
                "UPDATE syntaxmesh_change_set_membership_versions SET valid_until_sequence = ?1 WHERE change_set_id = ?2 AND event_id = ?3 AND valid_until_sequence IS NULL",
                (sequence, removal.key.change_set.0.0.to_vec(), removal.key.event.0.0.to_vec()),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("close membership removal: {error}")))?;
        if changed != 1 {
            return Err(TursoStoreError::Snapshot(
                "membership removal did not close exactly one active version".to_owned(),
            ));
        }
    }
    Ok(())
}

async fn consequence_fact_version_exists(
    connection: &turso::Connection,
    reference: FactVersionRef,
) -> Result<bool, TursoStoreError> {
    let (fact_kind, fact_id) = syntaxmesh_store::fact_storage_key(reference.fact);
    let valid_from = i64::try_from(
        read_generation_sequence(connection, reference.valid_from).await?,
    )
    .map_err(|error| {
        TursoStoreError::Snapshot(format!("fact generation sequence overflow: {error}"))
    })?;
    let mut rows = connection
        .query(
            "SELECT 1 FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence = ?3 LIMIT 1",
            (fact_kind, fact_id, valid_from),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("lookup consequence fact version: {error}")))?;
    Ok(rows
        .next()
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read consequence fact version: {error}"))
        })?
        .is_some())
}

async fn consequence_endpoint_exists(
    connection: &turso::Connection,
    endpoint: LineageEndpoint,
) -> Result<bool, TursoStoreError> {
    match endpoint {
        LineageEndpoint::ChangeEvent(id) => {
            let mut rows = connection
                .query(
                    "SELECT 1 FROM syntaxmesh_change_events WHERE event_id = ?1 LIMIT 1",
                    [id.0.0.to_vec()],
                )
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("lookup consequence event: {error}"))
                })?;
            Ok(rows
                .next()
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("read consequence event: {error}"))
                })?
                .is_some())
        }
        LineageEndpoint::FactVersion(reference) => {
            consequence_fact_version_exists(connection, reference).await
        }
    }
}

async fn consequence_provenance_exists(
    connection: &turso::Connection,
    provenance: ProvenanceId,
) -> Result<bool, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM syntaxmesh_provenance WHERE id = ?1 LIMIT 1",
            [provenance.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("lookup consequence provenance: {error}"))
        })?;
    Ok(rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read consequence provenance: {error}")))?
        .is_some())
}

async fn validate_consequence_delta_sql(
    connection: &turso::Connection,
    delta: &ConsequenceDelta,
    _sequence: i64,
) -> Result<(), TursoStoreError> {
    delta
        .validate()
        .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
    for retraction in &delta.retract {
        let mut rows = connection
            .query(
                "SELECT 1 FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1 AND valid_until_sequence IS NULL LIMIT 1",
                [retraction.edge.0.0.to_vec()],
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("lookup active consequence edge: {error}")))?;
        if rows
            .next()
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read active consequence edge: {error}"))
            })?
            .is_none()
        {
            return Err(StoreError::InvalidDelta(format!(
                "cannot retract inactive consequence edge {:?}",
                retraction.edge
            ))
            .into());
        }
        if !consequence_provenance_exists(connection, retraction.provenance).await? {
            return Err(StoreError::InvalidDelta(format!(
                "consequence retraction references unknown provenance {:?}",
                retraction.provenance
            ))
            .into());
        }
    }
    for edge in &delta.add {
        let mut prior = connection
            .query(
                "SELECT 1 FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1 LIMIT 1",
                [edge.id.0.0.to_vec()],
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("check consequence identity: {error}"))
            })?;
        if prior
            .next()
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read consequence identity: {error}"))
            })?
            .is_some()
        {
            return Err(StoreError::InvalidDelta(format!(
                "consequence edge ID {:?} was already used",
                edge.id
            ))
            .into());
        }
        if !consequence_provenance_exists(connection, edge.provenance).await? {
            return Err(StoreError::InvalidDelta(format!(
                "consequence edge references unknown provenance {:?}",
                edge.provenance
            ))
            .into());
        }
        if !consequence_endpoint_exists(connection, edge.source).await?
            || !consequence_endpoint_exists(connection, edge.target).await?
        {
            return Err(StoreError::InvalidDelta(
                "consequence edge references an unknown endpoint".to_owned(),
            )
            .into());
        }
        for evidence in &edge.evidence {
            if !consequence_fact_version_exists(connection, *evidence).await? {
                return Err(StoreError::InvalidDelta(format!(
                    "consequence evidence references unknown fact version {evidence:?}"
                ))
                .into());
            }
        }
        if let ConsequenceDerivation::DerivedFrom(parents) = &edge.derivation {
            for parent in parents {
                let mut rows = connection
                    .query(
                        "SELECT 1 FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1 AND valid_until_sequence IS NULL LIMIT 1",
                        [parent.0.0.to_vec()],
                    )
                    .await
                    .map_err(|error| TursoStoreError::Backend(format!("lookup consequence parent: {error}")))?;
                if rows
                    .next()
                    .await
                    .map_err(|error| {
                        TursoStoreError::Backend(format!("read consequence parent: {error}"))
                    })?
                    .is_none()
                {
                    return Err(StoreError::InvalidDelta(
                        "derived consequence references an inactive parent edge".to_owned(),
                    )
                    .into());
                }
            }
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

async fn apply_consequence_projection(
    connection: &turso::Connection,
    delta: &ConsequenceDelta,
    sequence: i64,
) -> Result<(), TursoStoreError> {
    for retraction in &delta.retract {
        let changed = connection
            .execute(
                "UPDATE syntaxmesh_consequence_edge_versions SET valid_until_sequence = ?1 WHERE edge_id = ?2 AND valid_until_sequence IS NULL",
                (sequence, retraction.edge.0.0.to_vec()),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("retract consequence edge: {error}")))?;
        if changed != 1 {
            return Err(TursoStoreError::Snapshot(
                "consequence retraction did not close exactly one edge".to_owned(),
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
                turso::params![edge.id.0.0.to_vec(), sequence, source_kind, source_id, source_fact_kind, source_generation, target_kind, target_id, target_fact_kind, target_generation, bincode::serialize(edge).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?],
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("append consequence edge: {error}")))?;
        for (ordinal, evidence) in edge.evidence.iter().enumerate() {
            let ordinal = i64::try_from(ordinal).map_err(|error| {
                TursoStoreError::Snapshot(format!("consequence evidence ordinal overflow: {error}"))
            })?;
            let (fact_kind, fact_id) = syntaxmesh_store::fact_storage_key(evidence.fact);
            connection
                .execute(
                    "INSERT INTO syntaxmesh_consequence_evidence (edge_id, ordinal, fact_kind, fact_id, valid_from_generation) VALUES (?1, ?2, ?3, ?4, ?5)",
                    (edge.id.0.0.to_vec(), ordinal, fact_kind, fact_id, evidence.valid_from.0.0.to_vec()),
                )
                .await
                .map_err(|error| TursoStoreError::Backend(format!("append consequence evidence: {error}")))?;
        }
        if let ConsequenceDerivation::DerivedFrom(parents) = &edge.derivation {
            for parent in parents {
                connection
                    .execute(
                        "INSERT INTO syntaxmesh_consequence_parents (edge_id, parent_edge_id) VALUES (?1, ?2)",
                        (edge.id.0.0.to_vec(), parent.0.0.to_vec()),
                    )
                    .await
                    .map_err(|error| TursoStoreError::Backend(format!("append consequence parent: {error}")))?;
            }
        }
    }
    Ok(())
}

async fn change_event_for_delta(
    connection: &turso::Connection,
    delta: &GraphDelta,
) -> Result<ChangeEvent, TursoStoreError> {
    // The only accepted delta without a base generation is the first write to
    // an empty store (the caller validates that precondition transactionally).
    // Avoid one indexed SQL probe per candidate fact in this common, often
    // very large, initial-import path: there is no prior projection to inspect.
    if delta.expected_base.is_none() {
        return syntaxmesh_store::change_event_from_prior(delta, &BTreeSet::new(), &[])
            .map_err(TursoStoreError::Store);
    }

    let mut prior_facts = BTreeSet::new();
    for fact in syntaxmesh_store::delta_fact_candidates(delta) {
        let (kind, id) = syntaxmesh_store::fact_storage_key(fact);
        let table = match kind {
            FILE_FACT => "syntaxmesh_files",
            PROVENANCE_FACT => "syntaxmesh_provenance",
            NODE_FACT => "syntaxmesh_nodes",
            EDGE_FACT => "syntaxmesh_edges",
            _ => {
                return Err(TursoStoreError::Snapshot(
                    "unknown fact kind in event projection".to_owned(),
                ));
            }
        };
        let sql = format!("SELECT 1 FROM {table} WHERE id = ?1 LIMIT 1");
        let mut rows = connection.query(&sql, [id]).await.map_err(|error| {
            TursoStoreError::Backend(format!("probe prior fact for event: {error}"))
        })?;
        if rows
            .next()
            .await
            .map_err(|error| TursoStoreError::Backend(format!("read prior fact probe: {error}")))?
            .is_some()
        {
            prior_facts.insert(fact);
        }
    }
    let mut incident_edges = BTreeMap::<EdgeId, Edge>::new();
    for node in &delta.remove_nodes {
        let mut rows = connection
            .query(
                "SELECT payload FROM syntaxmesh_edges WHERE source = ?1 OR target = ?1",
                [node.0.0.to_vec()],
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read incident edges for change event: {error}"))
            })?;
        while let Some(row) = rows
            .next()
            .await
            .map_err(|error| TursoStoreError::Backend(format!("read incident edge row: {error}")))?
        {
            let edge: Edge = decode_blob(
                row.get_value(0).map_err(|error| {
                    TursoStoreError::Backend(format!("read incident edge payload: {error}"))
                })?,
                "incident edge payload",
            )?;
            incident_edges.insert(edge.id, edge);
        }
    }
    prior_facts.extend(incident_edges.keys().copied().map(FactRef::Edge));
    syntaxmesh_store::change_event_from_prior(
        delta,
        &prior_facts,
        &incident_edges.into_values().collect::<Vec<_>>(),
    )
    .map_err(TursoStoreError::Store)
}

async fn backfill_change_events(
    connection: &mut turso::Connection,
    history: &[GenerationHistoryEntry],
) -> Result<(), TursoStoreError> {
    let expected = i64::try_from(history.iter().filter(|entry| entry.delta.is_some()).count())
        .map_err(|error| {
            TursoStoreError::Snapshot(format!("change-event count is too large: {error}"))
        })?;
    let mut rows = connection
        .query("SELECT COUNT(*) FROM syntaxmesh_change_events", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("count persisted change events: {error}"))
        })?;
    let actual =
        match rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read change-event count: {error}"))
        })? {
            Some(row) => match row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("decode change-event count: {error}"))
            })? {
                turso::Value::Integer(value) => value,
                other @ (turso::Value::Null
                | turso::Value::Real(_)
                | turso::Value::Text(_)
                | turso::Value::Blob(_)) => {
                    return Err(TursoStoreError::Backend(format!(
                        "change-event count was not an integer: {other:?}"
                    )));
                }
            },
            None => {
                return Err(TursoStoreError::Backend(
                    "change-event count query returned no row".to_owned(),
                ));
            }
        };
    if actual == expected {
        return Ok(());
    }
    drop(rows);
    let events =
        syntaxmesh_store::change_events_from_history(history).map_err(TursoStoreError::Store)?;
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin change-event backfill: {error}"))
        })?;
    transaction
        .execute_batch(
            "DELETE FROM syntaxmesh_change_event_facts; DELETE FROM syntaxmesh_change_events;",
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("clear incomplete change-event projection: {error}"))
        })?;
    for event in &events {
        insert_change_event(&transaction, event).await?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit change-event backfill: {error}")))
}

async fn read_change_events_for_fact(
    connection: &turso::Connection,
    fact: FactRef,
    after: Option<ChangeEventCursor>,
    limit: usize,
) -> Result<ChangeEventPage, TursoStoreError> {
    if limit == 0 {
        return Ok(ChangeEventPage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let (fact_kind, fact_id) = syntaxmesh_store::fact_storage_key(fact);
    let after_sequence = if let Some(cursor) = after {
        let mut rows = connection
            .query(
                "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
                [cursor.generation.0.0.to_vec()],
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("lookup change-event cursor: {error}"))
            })?;
        match rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read change-event cursor: {error}"))
        })? {
            Some(row) => match row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("decode change-event cursor: {error}"))
            })? {
                turso::Value::Integer(sequence) => Some(sequence),
                other @ (turso::Value::Null
                | turso::Value::Real(_)
                | turso::Value::Text(_)
                | turso::Value::Blob(_)) => {
                    return Err(TursoStoreError::Backend(format!(
                        "change-event cursor sequence was not an integer: {other:?}"
                    )));
                }
            },
            None => {
                return Err(TursoStoreError::Store(StoreError::StaleBase {
                    expected: Some(cursor.generation),
                    actual: read_single::<GenerationManifest>(
                        connection,
                        "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
                    )
                    .await?
                    .map(|manifest| manifest.generation),
                }));
            }
        }
    } else {
        None
    };
    let sql_limit = i64::try_from(limit.saturating_add(1)).map_err(|error| {
        TursoStoreError::Snapshot(format!("change-event limit is too large: {error}"))
    })?;
    let mut rows = connection
        .query("SELECT event.payload FROM syntaxmesh_change_event_facts AS fact JOIN syntaxmesh_change_events AS event ON event.generation = fact.generation WHERE fact.fact_kind = ?1 AND fact.fact_id = ?2 AND (?3 IS NULL OR fact.generation_sequence > ?3) ORDER BY fact.generation_sequence LIMIT ?4", (fact_kind, fact_id, after_sequence, sql_limit))
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query fact-linked change events: {error}")))?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read fact-linked change event: {error}"))
    })? {
        items.push(decode_blob(
            row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("read event payload: {error}"))
            })?,
            "change event",
        )?);
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

async fn read_change_event(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<Option<ChangeEvent>, TursoStoreError> {
    let mut generation_rows = connection
        .query(
            "SELECT 1 FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("lookup change-event generation: {error}"))
        })?;
    if generation_rows
        .next()
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read change-event generation: {error}"))
        })?
        .is_none()
    {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_single::<GenerationManifest>(
                connection,
                "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
            )
            .await?
            .map(|manifest| manifest.generation),
        }));
    }
    drop(generation_rows);

    let mut event_rows = connection
        .query(
            "SELECT payload FROM syntaxmesh_change_events WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("lookup change event: {error}")))?;
    let Some(row) = event_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read change event: {error}")))?
    else {
        return Ok(None);
    };
    decode_blob(
        row.get_value(0)
            .map_err(|error| TursoStoreError::Backend(format!("read event payload: {error}")))?,
        "change event",
    )
    .map(Some)
}

async fn read_generation_sequence(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<u64, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("lookup generation sequence: {error}"))
        })?;
    let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read generation sequence: {error}")))?
    else {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_single::<GenerationManifest>(
                connection,
                "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
            )
            .await?
            .map(|manifest| manifest.generation),
        }));
    };
    let sequence = row.get::<i64>(0).map_err(|error| {
        TursoStoreError::Backend(format!("decode generation sequence: {error}"))
    })?;
    u64::try_from(sequence)
        .map_err(|error| TursoStoreError::Snapshot(format!("invalid generation sequence: {error}")))
}

async fn read_events_for_change_set(
    connection: &turso::Connection,
    change_set: ChangeSetId,
    as_of: GenerationId,
    after: Option<EventsForChangeSetCursor>,
    limit: usize,
) -> Result<EventsForChangeSetPage, TursoStoreError> {
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
        )
        .into());
    }
    let as_of_sequence = read_generation_sequence(connection, as_of).await?;
    let after_sequence = if let Some(cursor) = after {
        Some(read_generation_sequence(connection, cursor.after_event_generation).await?)
    } else {
        None
    };
    if after_sequence.is_some_and(|sequence| sequence > as_of_sequence) {
        return Err(StoreError::StaleBase {
            expected: Some(as_of),
            actual: after.map(|cursor| cursor.after_event_generation),
        }
        .into());
    }
    let sql = if after_sequence.is_some() {
        "SELECT event.payload, membership.provenance_id, membership_generation.generation FROM syntaxmesh_change_set_membership_versions AS membership JOIN syntaxmesh_change_events AS event ON event.event_id = membership.event_id JOIN syntaxmesh_generation_history AS event_generation ON event_generation.generation = event.generation JOIN syntaxmesh_generation_history AS membership_generation ON membership_generation.sequence = membership.valid_from_sequence WHERE membership.change_set_id = ?1 AND membership.valid_from_sequence <= ?2 AND (membership.valid_until_sequence IS NULL OR membership.valid_until_sequence > ?2) AND event_generation.sequence <= ?2 AND event_generation.sequence > ?3 ORDER BY event_generation.sequence, event.event_id LIMIT ?4"
    } else {
        "SELECT event.payload, membership.provenance_id, membership_generation.generation FROM syntaxmesh_change_set_membership_versions AS membership JOIN syntaxmesh_change_events AS event ON event.event_id = membership.event_id JOIN syntaxmesh_generation_history AS event_generation ON event_generation.generation = event.generation JOIN syntaxmesh_generation_history AS membership_generation ON membership_generation.sequence = membership.valid_from_sequence WHERE membership.change_set_id = ?1 AND membership.valid_from_sequence <= ?2 AND (membership.valid_until_sequence IS NULL OR membership.valid_until_sequence > ?2) AND event_generation.sequence <= ?2 ORDER BY event_generation.sequence, event.event_id LIMIT ?3"
    };
    let fetch_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let params: Vec<turso::Value> = after_sequence.map_or_else(
        || {
            vec![
                turso::Value::Blob(change_set.0.0.to_vec()),
                turso::Value::Integer(as_of_sequence as i64),
                turso::Value::Integer(fetch_limit),
            ]
        },
        |sequence| {
            vec![
                turso::Value::Blob(change_set.0.0.to_vec()),
                turso::Value::Integer(as_of_sequence as i64),
                turso::Value::Integer(sequence as i64),
                turso::Value::Integer(fetch_limit),
            ]
        },
    );
    let mut rows = connection
        .query(sql, params)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query ChangeSet events: {error}")))?;
    let mut items = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read ChangeSet event: {error}")))?
    {
        let event: ChangeEvent = decode_blob(
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
            "ChangeSet event",
        )?;
        let provenance = decode_blob(
            row.get_value(1)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
            "membership provenance",
        )?;
        let valid_from = decode_blob(
            row.get_value(2)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
            "membership generation",
        )?;
        items.push(ChangeSetEvent {
            membership: ChangeSetMembership {
                change_set,
                event: event.id,
                provenance,
            },
            event,
            membership_valid_from: valid_from,
        });
    }
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

async fn read_consequence_edges_for_endpoint(
    connection: &turso::Connection,
    endpoint: LineageEndpoint,
    as_of: GenerationId,
    after: Option<ConsequenceEdgeCursor>,
    limit: usize,
) -> Result<ConsequenceEdgePage, TursoStoreError> {
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
        )
        .into());
    }
    let sequence = read_generation_sequence(connection, as_of).await?;
    let sequence = i64::try_from(sequence).map_err(|error| {
        TursoStoreError::Snapshot(format!("invalid generation sequence: {error}"))
    })?;
    let (kind, id, fact_kind, valid_from) = consequence_endpoint_storage(endpoint);
    let after_id = after
        .map(|cursor| turso::Value::Blob(cursor.after_edge.0.0.to_vec()))
        .unwrap_or(turso::Value::Null);
    let fetch_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let values = vec![
        turso::Value::Integer(sequence),
        turso::Value::Text(kind.to_owned()),
        turso::Value::Blob(id.clone()),
        fact_kind.map_or(turso::Value::Null, turso::Value::Integer),
        valid_from
            .clone()
            .map_or(turso::Value::Null, turso::Value::Blob),
        turso::Value::Text(kind.to_owned()),
        turso::Value::Blob(id),
        fact_kind.map_or(turso::Value::Null, turso::Value::Integer),
        valid_from.map_or(turso::Value::Null, turso::Value::Blob),
        after_id,
        turso::Value::Integer(fetch_limit),
    ];
    let mut rows = connection.query(
        "SELECT payload FROM syntaxmesh_consequence_edge_versions WHERE valid_from_sequence <= ?1 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?1) AND ((source_kind = ?2 AND source_id = ?3 AND source_fact_kind IS ?4 AND source_valid_from_generation IS ?5) OR (target_kind = ?6 AND target_id = ?7 AND target_fact_kind IS ?8 AND target_valid_from_generation IS ?9)) AND (?10 IS NULL OR edge_id > ?10) ORDER BY edge_id LIMIT ?11",
        values,
    ).await.map_err(|error| TursoStoreError::Backend(format!("query consequence endpoint: {error}")))?;
    let mut items = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read consequence endpoint: {error}")))?
    {
        items.push(decode_blob(
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
            "consequence edge",
        )?);
    }
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items
            .last()
            .map(|edge: &ConsequenceEdge| ConsequenceEdgeCursor {
                endpoint,
                as_of_generation: as_of,
                after_edge: edge.id,
            })
    } else {
        None
    };
    Ok(ConsequenceEdgePage { items, next_cursor })
}

async fn read_consequence_edges_for_endpoint_range(
    connection: &turso::Connection,
    endpoint: LineageEndpoint,
    from_generation: GenerationId,
    until_generation: GenerationId,
    after: Option<ConsequenceRangeCursor>,
    limit: usize,
) -> Result<ConsequenceRangePage, TursoStoreError> {
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
        )
        .into());
    }
    let (kind, id, fact_kind, endpoint_valid_from) = consequence_endpoint_storage(endpoint);
    let after_id = after
        .map(|cursor| turso::Value::Blob(cursor.after_edge.0.0.to_vec()))
        .unwrap_or(turso::Value::Null);
    let fetch_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let values = vec![
        turso::Value::Blob(from_generation.0.0.to_vec()),
        turso::Value::Blob(until_generation.0.0.to_vec()),
        turso::Value::Text(kind.to_owned()),
        turso::Value::Blob(id.clone()),
        fact_kind.map_or(turso::Value::Null, turso::Value::Integer),
        endpoint_valid_from
            .clone()
            .map_or(turso::Value::Null, turso::Value::Blob),
        turso::Value::Text(kind.to_owned()),
        turso::Value::Blob(id),
        fact_kind.map_or(turso::Value::Null, turso::Value::Integer),
        endpoint_valid_from.map_or(turso::Value::Null, turso::Value::Blob),
        after_id,
        turso::Value::Integer(fetch_limit),
    ];
    let mut rows = connection
        .query(CONSEQUENCE_RANGE_QUERY, values)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("query consequence range endpoint: {error}"))
        })?;
    #[cfg(feature = "benchmark-instrumentation")]
    let sql_read_statements = 1;
    let mut items = Vec::new();
    let mut found_bounds = false;
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read consequence range endpoint: {error}"))
    })? {
        found_bounds = true;
        let from_sequence = row
            .get_value(0)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let until_sequence = row
            .get_value(1)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let (turso::Value::Integer(from_sequence), turso::Value::Integer(until_sequence)) =
            (from_sequence, until_sequence)
        else {
            return Err(TursoStoreError::Snapshot(
                "consequence range bounds are missing or malformed".to_owned(),
            ));
        };
        if from_sequence > until_sequence {
            return Err(StoreError::InvalidDelta(
                "consequence range starts after it ends".to_owned(),
            )
            .into());
        }
        let payload = row
            .get_value(2)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let start = row
            .get_value(3)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let finish = row
            .get_value(4)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let edge = match payload {
            turso::Value::Null => continue,
            payload @ (turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)
            | turso::Value::Blob(_)) => decode_blob(payload, "consequence edge")?,
        };
        let version_valid_from = decode_generation_blob(start, "consequence start generation")?;
        let valid_until = match finish {
            turso::Value::Null => None,
            value @ (turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)
            | turso::Value::Blob(_)) => {
                Some(decode_generation_blob(value, "consequence end generation")?)
            }
        };
        items.push(ConsequenceEdgeVersion {
            edge,
            valid_from: version_valid_from,
            valid_until,
        });
    }
    if !found_bounds {
        // Preserve the established stale-generation diagnostic on this error
        // path; valid pages (including empty ones) use a single SQL read.
        read_generation_sequence(connection, from_generation).await?;
        read_generation_sequence(connection, until_generation).await?;
        return Err(TursoStoreError::Snapshot(
            "consequence range bounds disappeared during lookup".to_owned(),
        ));
    }
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

fn decode_generation_blob(
    value: turso::Value,
    field: &str,
) -> Result<GenerationId, TursoStoreError> {
    let bytes = match value {
        turso::Value::Blob(bytes) => bytes,
        turso::Value::Null
        | turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_) => {
            return Err(TursoStoreError::Snapshot(format!("invalid {field} value")));
        }
    };
    let id = bytes
        .try_into()
        .map_err(|error| TursoStoreError::Snapshot(format!("invalid {field} bytes: {error:?}")))?;
    Ok(GenerationId(syntaxmesh_core::StableId(id)))
}

async fn read_change_set_at(
    connection: &turso::Connection,
    change_set: ChangeSetId,
    as_of: GenerationId,
) -> Result<Option<ChangeSetVersion>, TursoStoreError> {
    let as_of_sequence = read_generation_sequence(connection, as_of).await?;
    let as_of_sequence = i64::try_from(as_of_sequence).map_err(|error| {
        TursoStoreError::Snapshot(format!("invalid generation sequence: {error}"))
    })?;
    let mut rows = connection
        .query(
            "SELECT version.payload, valid_from.generation, valid_until.generation \
             FROM syntaxmesh_change_set_versions AS version \
             JOIN syntaxmesh_generation_history AS valid_from ON valid_from.sequence = version.valid_from_sequence \
             LEFT JOIN syntaxmesh_generation_history AS valid_until ON valid_until.sequence = version.valid_until_sequence \
             WHERE version.change_set_id = ?1 AND version.valid_from_sequence <= ?2 \
               AND (version.valid_until_sequence IS NULL OR version.valid_until_sequence > ?2) \
             ORDER BY version.valid_from_sequence DESC LIMIT 1",
            (change_set.0.0.to_vec(), as_of_sequence),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query ChangeSet declaration: {error}")))?;
    let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read ChangeSet declaration: {error}"))
    })?
    else {
        return Ok(None);
    };
    let declaration: syntaxmesh_core::ChangeSet = decode_blob(
        row.get_value(0)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
        "ChangeSet payload",
    )?;
    let valid_from = decode_blob(
        row.get_value(1)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
        "ChangeSet valid-from generation",
    )?;
    let valid_until = match row
        .get_value(2)
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?
    {
        turso::Value::Null => None,
        value @ (turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_)
        | turso::Value::Blob(_)) => Some(decode_blob(value, "ChangeSet valid-until generation")?),
    };
    Ok(Some(ChangeSetVersion {
        change_set: declaration,
        valid_from,
        valid_until,
    }))
}

async fn read_history_with_sequences(
    connection: &turso::Connection,
) -> Result<Vec<(i64, GenerationHistoryEntry)>, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT sequence, payload FROM syntaxmesh_generation_history ORDER BY sequence",
            (),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!(
                "read generation history for temporal migration: {error}"
            ))
        })?;
    let mut history = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read generation history row: {error}"))
    })? {
        let sequence = match row
            .get_value(0)
            .map_err(|error| TursoStoreError::Backend(format!("read history sequence: {error}")))?
        {
            turso::Value::Integer(value) => value,
            turso::Value::Null
            | turso::Value::Real(_)
            | turso::Value::Text(_)
            | turso::Value::Blob(_) => {
                return Err(TursoStoreError::Snapshot(
                    "history sequence is not an integer".to_owned(),
                ));
            }
        };
        let entry: GenerationHistoryEntry = decode_blob(
            row.get_value(1).map_err(|error| {
                TursoStoreError::Backend(format!("read history entry: {error}"))
            })?,
            "history entry",
        )?;
        history.push((sequence, entry));
    }
    Ok(history)
}

async fn read_canonical_snapshot(
    connection: &turso::Connection,
) -> Result<GraphSnapshot, TursoStoreError> {
    Ok(GraphSnapshot {
        files: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_files ORDER BY id",
        )
        .await?,
        provenance: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_provenance ORDER BY id",
        )
        .await?,
        nodes: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_nodes ORDER BY id",
        )
        .await?,
        edges: read_many(
            connection,
            "SELECT payload FROM syntaxmesh_edges ORDER BY id",
        )
        .await?,
    })
}

async fn next_history_sequence(connection: &turso::Connection) -> Result<i64, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM syntaxmesh_generation_history",
            (),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read next history sequence: {error}"))
        })?;
    let row = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read next history sequence: {error}")))?
        .ok_or_else(|| {
            TursoStoreError::Snapshot("history sequence query returned no value".to_owned())
        })?;
    match row.get_value(0).map_err(|error| {
        TursoStoreError::Backend(format!("decode next history sequence: {error}"))
    })? {
        turso::Value::Integer(value) => Ok(value),
        turso::Value::Null
        | turso::Value::Real(_)
        | turso::Value::Text(_)
        | turso::Value::Blob(_) => Err(TursoStoreError::Snapshot(
            "next history sequence is not an integer".to_owned(),
        )),
    }
}

#[derive(Default)]
struct TemporalFactMetadata {
    observed_at: Option<Vec<u8>>,
    source: Option<Vec<u8>>,
    target: Option<Vec<u8>>,
}

struct TemporalFactWriteOptions {
    close_existing: bool,
    metadata: TemporalFactMetadata,
}

impl TemporalFactWriteOptions {
    const fn new(close_existing: bool, metadata: TemporalFactMetadata) -> Self {
        Self {
            close_existing,
            metadata,
        }
    }
}

fn runtime_observed_at(node: &Node) -> Option<Vec<u8>> {
    let payload = node.extension_payload.as_ref()?;
    let value: serde_json::Value = serde_json::from_slice(&payload.bytes).ok()?;
    let observed = value.get("observed_at_unix_nanos")?.as_u64()?;
    Some(observed.to_be_bytes().to_vec())
}

async fn seed_temporal_snapshot(
    connection: &turso::Connection,
    snapshot: &GraphSnapshot,
    sequence: i64,
) -> Result<(), TursoStoreError> {
    let mut close_fact = connection
        .prepare("UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 WHERE fact_kind = ?2 AND fact_id = ?3 AND valid_until_sequence IS NULL")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("prepare temporal fact closure for snapshot: {error}")))?;
    let mut insert_fact = connection
        .prepare("INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, valid_until_sequence, observed_at_unix_nanos, source_id, target_id, payload) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7)")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("prepare temporal fact insertion for snapshot: {error}")))?;
    for file in &snapshot.files {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            FILE_FACT,
            file.file_id.0.0.to_vec(),
            bincode::serialize(file)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(true, TemporalFactMetadata::default()),
        )
        .await?;
    }
    for item in &snapshot.provenance {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            PROVENANCE_FACT,
            item.id.0.0.to_vec(),
            bincode::serialize(item)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(true, TemporalFactMetadata::default()),
        )
        .await?;
    }
    for node in &snapshot.nodes {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            NODE_FACT,
            node.id.0.0.to_vec(),
            bincode::serialize(node)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(
                true,
                TemporalFactMetadata {
                    observed_at: runtime_observed_at(node),
                    ..TemporalFactMetadata::default()
                },
            ),
        )
        .await?;
    }
    for edge in &snapshot.edges {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            EDGE_FACT,
            edge.id.0.0.to_vec(),
            bincode::serialize(edge)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(
                true,
                TemporalFactMetadata {
                    source: Some(edge.source.0.0.to_vec()),
                    target: Some(edge.target.0.0.to_vec()),
                    ..TemporalFactMetadata::default()
                },
            ),
        )
        .await?;
    }
    Ok(())
}

async fn apply_temporal_delta(
    connection: &turso::Connection,
    delta: &GraphDelta,
    sequence: i64,
    cascaded_edge_ids: Option<&BTreeSet<EdgeId>>,
) -> Result<(), TursoStoreError> {
    let close_existing = delta.expected_base.is_some();
    let mut close_source_edge = if cascaded_edge_ids.is_none() {
        Some(connection
            .prepare("UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 WHERE fact_kind = ?2 AND valid_until_sequence IS NULL AND source_id = ?3")
            .await
            .map_err(|error| TursoStoreError::Backend(format!("prepare outgoing edge closure: {error}")))?)
    } else {
        None
    };
    let mut close_target_edge = if cascaded_edge_ids.is_none() {
        Some(connection
            .prepare("UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 WHERE fact_kind = ?2 AND valid_until_sequence IS NULL AND target_id = ?3")
            .await
            .map_err(|error| TursoStoreError::Backend(format!("prepare incoming edge closure: {error}")))?)
    } else {
        None
    };
    let mut close_fact = connection
        .prepare("UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 WHERE fact_kind = ?2 AND fact_id = ?3 AND valid_until_sequence IS NULL")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("prepare temporal fact closure: {error}")))?;
    let mut insert_fact = connection
        .prepare("INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, valid_until_sequence, observed_at_unix_nanos, source_id, target_id, payload) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7)")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("prepare temporal fact insertion: {error}")))?;
    if let Some(cascaded_edge_ids) = cascaded_edge_ids {
        let mut edges_to_close = cascaded_edge_ids.clone();
        edges_to_close.extend(delta.remove_edges.iter().copied());
        close_temporal_edges_by_id(connection, edges_to_close, sequence).await?;
    } else {
        for node_id in &delta.remove_nodes {
            close_source_edge
                .as_mut()
                .ok_or_else(|| {
                    TursoStoreError::Snapshot("outgoing close statement missing".to_owned())
                })?
                .execute((sequence, EDGE_FACT, node_id.0.0.to_vec()))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("close outgoing edge versions: {error}"))
                })?;
            close_target_edge
                .as_mut()
                .ok_or_else(|| {
                    TursoStoreError::Snapshot("incoming close statement missing".to_owned())
                })?
                .execute((sequence, EDGE_FACT, node_id.0.0.to_vec()))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("close incident edge versions: {error}"))
                })?;
        }
    }
    for file_id in &delta.removed_files {
        close_temporal_fact_prepared(&mut close_fact, FILE_FACT, file_id.0.0.to_vec(), sequence)
            .await?;
    }
    if cascaded_edge_ids.is_none() {
        for edge_id in &delta.remove_edges {
            close_temporal_fact_prepared(
                &mut close_fact,
                EDGE_FACT,
                edge_id.0.0.to_vec(),
                sequence,
            )
            .await?;
        }
    }
    for node_id in &delta.remove_nodes {
        close_temporal_fact_prepared(&mut close_fact, NODE_FACT, node_id.0.0.to_vec(), sequence)
            .await?;
    }
    for file in &delta.changed_files {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            FILE_FACT,
            file.file_id.0.0.to_vec(),
            bincode::serialize(file)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(close_existing, TemporalFactMetadata::default()),
        )
        .await?;
    }
    for item in &delta.upsert_provenance {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            PROVENANCE_FACT,
            item.id.0.0.to_vec(),
            bincode::serialize(item)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(close_existing, TemporalFactMetadata::default()),
        )
        .await?;
    }
    for node in &delta.upsert_nodes {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            NODE_FACT,
            node.id.0.0.to_vec(),
            bincode::serialize(node)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(
                close_existing,
                TemporalFactMetadata {
                    observed_at: runtime_observed_at(node),
                    ..TemporalFactMetadata::default()
                },
            ),
        )
        .await?;
    }
    for edge in &delta.upsert_edges {
        write_temporal_fact_prepared(
            &mut close_fact,
            &mut insert_fact,
            EDGE_FACT,
            edge.id.0.0.to_vec(),
            bincode::serialize(edge)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            sequence,
            TemporalFactWriteOptions::new(
                close_existing,
                TemporalFactMetadata {
                    source: Some(edge.source.0.0.to_vec()),
                    target: Some(edge.target.0.0.to_vec()),
                    ..TemporalFactMetadata::default()
                },
            ),
        )
        .await?;
    }
    Ok(())
}

async fn close_temporal_edges_by_id(
    connection: &turso::Connection,
    edge_ids: BTreeSet<EdgeId>,
    sequence: i64,
) -> Result<(), TursoStoreError> {
    let parameters = (3..EDGE_CLOSE_BATCH_SIZE + 3)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "UPDATE syntaxmesh_fact_versions SET valid_until_sequence = ?1 \
         WHERE fact_kind = ?2 AND valid_until_sequence IS NULL AND fact_id IN ({parameters})"
    );
    let mut statement = connection
        .prepare(&sql)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("prepare edge ID closure: {error}")))?;
    let edge_ids = edge_ids.into_iter().collect::<Vec<_>>();
    for batch in edge_ids.chunks(EDGE_CLOSE_BATCH_SIZE) {
        let mut values = Vec::with_capacity(EDGE_CLOSE_BATCH_SIZE + 2);
        values.push(turso::Value::Integer(sequence));
        values.push(turso::Value::Integer(EDGE_FACT));
        values.extend(
            batch
                .iter()
                .map(|edge_id| turso::Value::Blob(edge_id.0.0.to_vec())),
        );
        values.resize(EDGE_CLOSE_BATCH_SIZE + 2, turso::Value::Null);
        statement
            .execute(turso::params_from_iter(values))
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("close incident edge versions by ID: {error}"))
            })?;
    }
    Ok(())
}

async fn close_temporal_fact_prepared(
    statement: &mut turso::Statement,
    kind: i64,
    id: Vec<u8>,
    sequence: i64,
) -> Result<(), TursoStoreError> {
    statement
        .execute((sequence, kind, id))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("close temporal fact version: {error}"))
        })?;
    Ok(())
}

async fn write_temporal_fact_prepared(
    close_statement: &mut turso::Statement,
    insert_statement: &mut turso::Statement,
    kind: i64,
    id: Vec<u8>,
    payload: Vec<u8>,
    sequence: i64,
    options: TemporalFactWriteOptions,
) -> Result<(), TursoStoreError> {
    if options.close_existing {
        close_temporal_fact_prepared(close_statement, kind, id.clone(), sequence).await?;
    }
    insert_statement
        .execute((
            kind,
            id,
            sequence,
            options.metadata.observed_at,
            options.metadata.source,
            options.metadata.target,
            payload,
        ))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("write temporal fact version: {error}"))
        })?;
    Ok(())
}

fn terminal_name(name: &str) -> String {
    name.rsplit("::").next().unwrap_or(name).to_owned()
}

fn projection_values_clause(
    row_count: usize,
    column_count: usize,
) -> Result<String, TursoStoreError> {
    let mut parameter = 1_usize;
    let mut rows = Vec::with_capacity(row_count);
    for _ in 0..row_count {
        let mut parameters = Vec::with_capacity(column_count);
        for _ in 0..column_count {
            parameters.push(format!("?{parameter}"));
            parameter = parameter.checked_add(1).ok_or_else(|| {
                TursoStoreError::Snapshot("projection bind parameter index overflow".to_owned())
            })?;
        }
        rows.push(format!("({})", parameters.join(", ")));
    }
    Ok(rows.join(", "))
}

async fn apply_database_delta(
    connection: &mut turso::Connection,
    delta: GraphDelta,
    expected_manifest: Option<GenerationManifest>,
    accepted_at: Option<AcceptanceTime>,
    lineage: ChangeSetDelta,
    consequences: ConsequenceDelta,
) -> Result<GenerationManifest, TursoStoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let mut profile = TransactionProfile::new();
    #[cfg(feature = "benchmark-instrumentation")]
    if profile.enabled {
        eprintln!(
            "turso_delta changed_files={} removed_files={} upsert_provenance={} remove_nodes={} upsert_nodes={} remove_edges={} upsert_edges={}",
            delta.changed_files.len(),
            delta.removed_files.len(),
            delta.upsert_provenance.len(),
            delta.remove_nodes.len(),
            delta.upsert_nodes.len(),
            delta.remove_edges.len(),
            delta.upsert_edges.len(),
        );
    }
    delta
        .validate()
        .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("begin graph delta: {error}")))?;
    let current = read_single::<GenerationManifest>(
        &transaction,
        "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
    )
    .await?;
    let actual = current.as_ref().map(|manifest| manifest.generation);
    if actual != delta.expected_base || current != expected_manifest {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: delta.expected_base,
            actual,
        }));
    }
    if actual == Some(delta.next_generation) {
        return current.ok_or_else(|| {
            TursoStoreError::Snapshot("idempotent generation has no manifest".to_owned())
        });
    }
    if current.as_ref().is_some_and(|manifest| {
        manifest.repository != delta.repository || manifest.worktree != delta.worktree
    }) {
        return Err(TursoStoreError::Store(StoreError::Backend(
            "store is already bound to another repository or worktree".to_owned(),
        )));
    }
    let event = change_event_for_delta(&transaction, &delta).await?;
    let history_sequence = next_history_sequence(&transaction).await?;
    validate_change_set_lineage_sql(&transaction, &delta, &lineage, history_sequence, &event)
        .await?;
    let parent_sequence = match delta.expected_base {
        Some(parent) => Some(read_parent_sequence(&transaction, parent).await?),
        None => None,
    };
    let cascaded_edge_ids =
        current_projection_cascade_edge_ids(&transaction, &delta.remove_nodes).await?;
    let incidence_changes =
        delta_incidence_changes(&transaction, &delta, &cascaded_edge_ids).await?;
    let delta_mutations = delta_tree_mutations(
        &transaction,
        &delta,
        parent_sequence,
        2,
        Some(&cascaded_edge_ids),
    )
    .await?;
    let rebuild_root = current
        .as_ref()
        .is_some_and(|manifest| manifest.schema_version < 2);
    let (root_parent, tree_mutations) = if rebuild_root {
        let parent = delta.expected_base.ok_or_else(|| {
            TursoStoreError::Snapshot("legacy root transition has no parent generation".to_owned())
        })?;
        let snapshot = read_persistent_snapshot(&transaction, parent).await?;
        (
            None,
            merge_persistent_mutations(snapshot_tree_mutations(&snapshot, 2)?, delta_mutations),
        )
    } else {
        (delta.expected_base, delta_mutations)
    };
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("validate_history_and_build_delta_mutations");
    apply_temporal_delta(
        &transaction,
        &delta,
        history_sequence,
        Some(&cascaded_edge_ids),
    )
    .await?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("temporal_fact_versions");
    let fact_root = if rebuild_root {
        publish_rebuilt_persistent_root(
            &transaction,
            history_sequence,
            delta.next_generation,
            &tree_mutations,
        )
        .await?
    } else {
        publish_persistent_root(
            &transaction,
            history_sequence,
            delta.next_generation,
            root_parent,
            &tree_mutations,
        )
        .await?
    };
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("persistent_fact_root");
    publish_incidence_root(
        &transaction,
        history_sequence,
        delta.next_generation,
        delta.expected_base,
        &incidence_changes,
    )
    .await?;
    let graph_root = generation_root_v2(delta.next_generation.0, fact_root);
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("persistent_incidence_root");

    if !delta.removed_files.is_empty() {
        let mut statement = transaction
            .prepare("DELETE FROM syntaxmesh_files WHERE id = ?1")
            .await
            .map_err(|error| TursoStoreError::Backend(format!("prepare file removal: {error}")))?;
        for file_id in &delta.removed_files {
            statement
                .execute([file_id.0.0.to_vec()])
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("remove file version: {error}"))
                })?;
        }
    }
    if !delta.changed_files.is_empty() {
        let mut statement = transaction
            .prepare("INSERT INTO syntaxmesh_files (id, payload) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload")
            .await
            .map_err(|error| TursoStoreError::Backend(format!("prepare file upsert: {error}")))?;
        for file in &delta.changed_files {
            statement
                .execute((
                    file.file_id.0.0.to_vec(),
                    bincode::serialize(file)
                        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                ))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("upsert file version: {error}"))
                })?;
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("current_projection_file_rows");
    if !delta.upsert_provenance.is_empty() {
        let mut statement = transaction
            .prepare("INSERT INTO syntaxmesh_provenance (id, payload) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload")
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("prepare provenance upsert: {error}"))
            })?;
        for item in &delta.upsert_provenance {
            statement
                .execute((
                    item.id.0.0.to_vec(),
                    bincode::serialize(item)
                        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                ))
                .await
                .map_err(|error| TursoStoreError::Backend(format!("upsert provenance: {error}")))?;
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("current_projection_provenance_rows");
    let mut edges_to_remove = cascaded_edge_ids.clone();
    edges_to_remove.extend(delta.remove_edges.iter().copied());
    if !edges_to_remove.is_empty() {
        let mut statement = transaction
            .prepare("DELETE FROM syntaxmesh_edges WHERE id = ?1")
            .await
            .map_err(|error| TursoStoreError::Backend(format!("prepare edge removal: {error}")))?;
        for edge_id in &edges_to_remove {
            statement
                .execute([edge_id.0.0.to_vec()])
                .await
                .map_err(|error| TursoStoreError::Backend(format!("remove edge: {error}")))?;
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("current_projection_edge_removals");
    if !delta.remove_nodes.is_empty() {
        let mut delete_nodes = transaction
            .prepare("DELETE FROM syntaxmesh_nodes WHERE id = ?1")
            .await
            .map_err(|error| TursoStoreError::Backend(format!("prepare node removal: {error}")))?;
        for node_id in &delta.remove_nodes {
            delete_nodes
                .execute([node_id.0.0.to_vec()])
                .await
                .map_err(|error| TursoStoreError::Backend(format!("remove node: {error}")))?;
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("current_projection_node_removals");
    if !delta.upsert_nodes.is_empty() {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut encoding_elapsed = Duration::ZERO;
        #[cfg(feature = "benchmark-instrumentation")]
        let mut execution_elapsed = Duration::ZERO;
        if delta.expected_base.is_none() {
            let mut full_statement = if delta.upsert_nodes.len() >= CURRENT_PROJECTION_BATCH_SIZE {
                let values = projection_values_clause(CURRENT_PROJECTION_BATCH_SIZE, 7)?;
                let sql = format!(
                    "INSERT INTO syntaxmesh_nodes (id, payload, name, owner_file, terminal_name, provenance, node_kind) VALUES {values} ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, name = excluded.name, owner_file = excluded.owner_file, terminal_name = excluded.terminal_name, provenance = excluded.provenance, node_kind = excluded.node_kind"
                );
                Some(transaction.prepare(&sql).await.map_err(|error| {
                    TursoStoreError::Backend(format!("prepare batched node upsert: {error}"))
                })?)
            } else {
                None
            };
            for nodes in delta.upsert_nodes.chunks(CURRENT_PROJECTION_BATCH_SIZE) {
                #[cfg(feature = "benchmark-instrumentation")]
                let encoding_started = Instant::now();
                let mut values = Vec::with_capacity(nodes.len().saturating_mul(7));
                for node in nodes {
                    values.extend([
                        turso::Value::Blob(node.id.0.0.to_vec()),
                        turso::Value::Blob(
                            bincode::serialize(node)
                                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                        ),
                        turso::Value::Text(node.name.clone()),
                        node.owner_file.map_or(turso::Value::Null, |file| {
                            turso::Value::Blob(file.0.0.to_vec())
                        }),
                        turso::Value::Text(terminal_name(&node.name)),
                        turso::Value::Blob(node.provenance.0.0.to_vec()),
                        turso::Value::Integer(node_kind_storage_code(&node.kind)),
                    ]);
                }
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    encoding_elapsed = encoding_elapsed.saturating_add(encoding_started.elapsed());
                }
                #[cfg(feature = "benchmark-instrumentation")]
                let execution_started = Instant::now();
                if nodes.len() == CURRENT_PROJECTION_BATCH_SIZE {
                    full_statement
                        .as_mut()
                        .ok_or_else(|| {
                            TursoStoreError::Snapshot(
                                "full node projection statement missing".to_owned(),
                            )
                        })?
                        .execute(turso::params_from_iter(values))
                        .await
                        .map_err(|error| {
                            TursoStoreError::Backend(format!("upsert node batch: {error}"))
                        })?;
                } else {
                    let values_sql = projection_values_clause(nodes.len(), 7)?;
                    let sql = format!(
                        "INSERT INTO syntaxmesh_nodes (id, payload, name, owner_file, terminal_name, provenance, node_kind) VALUES {values_sql} ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, name = excluded.name, owner_file = excluded.owner_file, terminal_name = excluded.terminal_name, provenance = excluded.provenance, node_kind = excluded.node_kind"
                    );
                    transaction
                        .execute(&sql, turso::params_from_iter(values))
                        .await
                        .map_err(|error| {
                            TursoStoreError::Backend(format!("upsert node tail batch: {error}"))
                        })?;
                }
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    execution_elapsed =
                        execution_elapsed.saturating_add(execution_started.elapsed());
                }
            }
        } else {
            let mut statement = transaction
                .prepare("INSERT INTO syntaxmesh_nodes (id, payload, name, owner_file, terminal_name, provenance, node_kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, name = excluded.name, owner_file = excluded.owner_file, terminal_name = excluded.terminal_name, provenance = excluded.provenance, node_kind = excluded.node_kind")
                .await
                .map_err(|error| TursoStoreError::Backend(format!("prepare node upsert: {error}")))?;
            for node in &delta.upsert_nodes {
                #[cfg(feature = "benchmark-instrumentation")]
                let encoding_started = Instant::now();
                let id = node.id.0.0.to_vec();
                let payload = bincode::serialize(node)
                    .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
                let owner_file = node.owner_file.map(|file| file.0.0.to_vec());
                let terminal_name = terminal_name(&node.name);
                let provenance = node.provenance.0.0.to_vec();
                let node_kind = node_kind_storage_code(&node.kind);
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    encoding_elapsed = encoding_elapsed.saturating_add(encoding_started.elapsed());
                }
                #[cfg(feature = "benchmark-instrumentation")]
                let execution_started = Instant::now();
                statement
                    .execute((
                        id,
                        payload,
                        node.name.as_str(),
                        owner_file,
                        terminal_name,
                        provenance,
                        node_kind,
                    ))
                    .await
                    .map_err(|error| TursoStoreError::Backend(format!("upsert node: {error}")))?;
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    execution_elapsed =
                        execution_elapsed.saturating_add(execution_started.elapsed());
                }
            }
        }
        #[cfg(feature = "benchmark-instrumentation")]
        {
            profile.report_elapsed("current_projection_node_encoding", encoding_elapsed);
            profile.report_elapsed(
                "current_projection_node_statement_execution",
                execution_elapsed,
            );
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("current_projection_node_upserts");
    if !delta.upsert_edges.is_empty() {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut encoding_elapsed = Duration::ZERO;
        #[cfg(feature = "benchmark-instrumentation")]
        let mut execution_elapsed = Duration::ZERO;
        if delta.expected_base.is_none() {
            let mut full_statement = if delta.upsert_edges.len() >= CURRENT_PROJECTION_BATCH_SIZE {
                let values = projection_values_clause(CURRENT_PROJECTION_BATCH_SIZE, 5)?;
                let sql = format!(
                    "INSERT INTO syntaxmesh_edges (id, payload, source, target, provenance) VALUES {values} ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, source = excluded.source, target = excluded.target, provenance = excluded.provenance"
                );
                Some(transaction.prepare(&sql).await.map_err(|error| {
                    TursoStoreError::Backend(format!("prepare batched edge upsert: {error}"))
                })?)
            } else {
                None
            };
            for edges in delta.upsert_edges.chunks(CURRENT_PROJECTION_BATCH_SIZE) {
                #[cfg(feature = "benchmark-instrumentation")]
                let encoding_started = Instant::now();
                let mut values = Vec::with_capacity(edges.len().saturating_mul(5));
                for edge in edges {
                    values.extend([
                        turso::Value::Blob(edge.id.0.0.to_vec()),
                        turso::Value::Blob(
                            bincode::serialize(edge)
                                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                        ),
                        turso::Value::Blob(edge.source.0.0.to_vec()),
                        turso::Value::Blob(edge.target.0.0.to_vec()),
                        turso::Value::Blob(edge.provenance.0.0.to_vec()),
                    ]);
                }
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    encoding_elapsed = encoding_elapsed.saturating_add(encoding_started.elapsed());
                }
                #[cfg(feature = "benchmark-instrumentation")]
                let execution_started = Instant::now();
                if edges.len() == CURRENT_PROJECTION_BATCH_SIZE {
                    full_statement
                        .as_mut()
                        .ok_or_else(|| {
                            TursoStoreError::Snapshot(
                                "full edge projection statement missing".to_owned(),
                            )
                        })?
                        .execute(turso::params_from_iter(values))
                        .await
                        .map_err(|error| {
                            TursoStoreError::Backend(format!("upsert edge batch: {error}"))
                        })?;
                } else {
                    let values_sql = projection_values_clause(edges.len(), 5)?;
                    let sql = format!(
                        "INSERT INTO syntaxmesh_edges (id, payload, source, target, provenance) VALUES {values_sql} ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, source = excluded.source, target = excluded.target, provenance = excluded.provenance"
                    );
                    transaction
                        .execute(&sql, turso::params_from_iter(values))
                        .await
                        .map_err(|error| {
                            TursoStoreError::Backend(format!("upsert edge tail batch: {error}"))
                        })?;
                }
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    execution_elapsed =
                        execution_elapsed.saturating_add(execution_started.elapsed());
                }
            }
        } else {
            let mut statement = transaction
                .prepare("INSERT INTO syntaxmesh_edges (id, payload, source, target, provenance) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, source = excluded.source, target = excluded.target, provenance = excluded.provenance")
                .await
                .map_err(|error| TursoStoreError::Backend(format!("prepare edge upsert: {error}")))?;
            for edge in &delta.upsert_edges {
                #[cfg(feature = "benchmark-instrumentation")]
                let encoding_started = Instant::now();
                let id = edge.id.0.0.to_vec();
                let payload = bincode::serialize(edge)
                    .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
                let source = edge.source.0.0.to_vec();
                let target = edge.target.0.0.to_vec();
                let provenance = edge.provenance.0.0.to_vec();
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    encoding_elapsed = encoding_elapsed.saturating_add(encoding_started.elapsed());
                }
                #[cfg(feature = "benchmark-instrumentation")]
                let execution_started = Instant::now();
                statement
                    .execute((id, payload, source, target, provenance))
                    .await
                    .map_err(|error| TursoStoreError::Backend(format!("upsert edge: {error}")))?;
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    execution_elapsed =
                        execution_elapsed.saturating_add(execution_started.elapsed());
                }
            }
        }
        #[cfg(feature = "benchmark-instrumentation")]
        {
            profile.report_elapsed("current_projection_edge_encoding", encoding_elapsed);
            profile.report_elapsed(
                "current_projection_edge_statement_execution",
                execution_elapsed,
            );
        }
    }

    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("current_projection_edge_upserts");
    integrity::validate_delta_references_sql(&transaction, &delta).await?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("delta_reference_validation");
    let manifest = GenerationManifest {
        repository: delta.repository,
        worktree: delta.worktree,
        generation: delta.next_generation,
        parent: delta.expected_base,
        graph_root,
        configuration_hash: [0; 32],
        extractor_set_hash: [0; 32],
        schema_version: 2,
        status: GenerationStatus::Durable,
    };
    transaction.execute("INSERT INTO syntaxmesh_manifest (id, payload) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload", [bincode::serialize(&manifest).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?]).await
        .map_err(|error| TursoStoreError::Backend(format!("publish graph manifest: {error}")))?;
    let history = GenerationHistoryEntry {
        manifest: manifest.clone(),
        delta: Some(delta.clone()),
        anchor: None,
    };
    transaction
        .execute(
            "INSERT INTO syntaxmesh_generation_history (sequence, generation, payload) VALUES (?1, ?2, ?3)",
            (
                history_sequence,
                manifest.generation.0.0.to_vec(),
                bincode::serialize(&history)
                    .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            ),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("append graph generation history: {error}"))
        })?;
    insert_change_event(&transaction, &event).await?;
    let lineage_entry = GenerationLineageEntry {
        generation: manifest.generation,
        delta: lineage.clone(),
    };
    transaction
        .execute(
            "INSERT INTO syntaxmesh_generation_lineage (sequence, generation, payload) VALUES (?1, ?2, ?3)",
            (
                history_sequence,
                manifest.generation.0.0.to_vec(),
                bincode::serialize(&lineage_entry)
                    .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            ),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("append generation lineage: {error}")))?;
    apply_change_set_lineage_projection(&transaction, &lineage, history_sequence).await?;
    validate_consequence_delta_sql(&transaction, &consequences, history_sequence).await?;
    let consequence_entry = GenerationConsequenceEntry {
        generation: manifest.generation,
        delta: consequences.clone(),
    };
    transaction
        .execute(
            "INSERT INTO syntaxmesh_generation_consequences (sequence, generation, payload) VALUES (?1, ?2, ?3)",
            (
                history_sequence,
                manifest.generation.0.0.to_vec(),
                bincode::serialize(&consequence_entry)
                    .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            ),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("append generation consequences: {error}"))
        })?;
    apply_consequence_projection(&transaction, &consequences, history_sequence).await?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("history_lineage_and_consequence_projection");
    if let Some(accepted_at) = accepted_at {
        let accepted_through = if let Some(parent) = delta.expected_base {
            let mut rows = transaction.query(
                "SELECT accepted_through_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
                [parent.0.0.to_vec()],
            ).await.map_err(|error| TursoStoreError::Backend(format!("read parent acceptance prefix: {error}")))?;
            match rows.next().await.map_err(|error| {
                TursoStoreError::Backend(format!("read parent acceptance prefix row: {error}"))
            })? {
                Some(row) => match row.get_value(0).map_err(|error| {
                    TursoStoreError::Backend(format!("decode parent acceptance prefix: {error}"))
                })? {
                    turso::Value::Blob(bytes) => {
                        let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                            TursoStoreError::Snapshot(format!(
                                "accepted-through watermark must be eight bytes, got {}",
                                bytes.len()
                            ))
                        })?;
                        Some(u64::from_be_bytes(bytes).max(accepted_at.0))
                    }
                    turso::Value::Null => None,
                    other @ (turso::Value::Integer(_)
                    | turso::Value::Real(_)
                    | turso::Value::Text(_)) => {
                        return Err(TursoStoreError::Snapshot(format!(
                            "accepted-through watermark has invalid storage type: {other:?}"
                        )));
                    }
                },
                None => None,
            }
        } else {
            Some(accepted_at.0)
        };
        transaction
            .execute(
                "INSERT INTO syntaxmesh_generation_acceptance (generation, accepted_at_unix_nanos, accepted_through_unix_nanos) VALUES (?1, ?2, ?3)",
                (manifest.generation.0.0.to_vec(), accepted_at.0.to_be_bytes().to_vec(), accepted_through.map(|value| value.to_be_bytes().to_vec())),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("record generation acceptance time: {error}"))
            })?;
    }
    if history_sequence == 1 || history_sequence % CHECKPOINT_INTERVAL == 0 {
        let snapshot = read_canonical_snapshot(&transaction).await?;
        transaction.execute(
            "INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (?1, ?2, ?3, ?4)",
            (history_sequence, manifest.generation.0.0.to_vec(), bincode::serialize(&manifest).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?, bincode::serialize(&snapshot).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
        ).await.map_err(|error| TursoStoreError::Backend(format!("write graph checkpoint: {error}")))?;
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("acceptance_and_checkpoint");
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit graph delta: {error}")))?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.mark("transaction_commit");
    Ok(manifest)
}

async fn set_database_status(
    connection: &mut turso::Connection,
    generation: GenerationId,
    status: GenerationStatus,
    expected_manifest: Option<GenerationManifest>,
) -> Result<GenerationManifest, TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin generation status update: {error}"))
        })?;
    let mut manifest = read_single::<GenerationManifest>(
        &transaction,
        "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
    )
    .await?
    .ok_or(StoreError::StaleBase {
        expected: Some(generation),
        actual: None,
    })?;
    if Some(manifest.clone()) != expected_manifest {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: expected_manifest.map(|item| item.generation),
            actual: Some(manifest.generation),
        }));
    }
    if manifest.generation != generation {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: Some(manifest.generation),
        }));
    }
    manifest.status = status;
    transaction
        .execute(
            "UPDATE syntaxmesh_manifest SET payload = ?1 WHERE id = 1",
            [bincode::serialize(&manifest)
                .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("write generation status: {error}")))?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit generation status: {error}")))?;
    Ok(manifest)
}

async fn read_single<T>(
    connection: &turso::Connection,
    sql: &str,
) -> Result<Option<T>, TursoStoreError>
where
    T: serde::de::DeserializeOwned,
{
    let mut rows = connection
        .query(sql, ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read Turso row: {error}")))?;
    let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read Turso row: {error}")))?
    else {
        return Ok(None);
    };
    match row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("read Turso payload: {error}")))?
    {
        turso::Value::Blob(payload) => bincode::deserialize(&payload)
            .map(Some)
            .map_err(|error| TursoStoreError::Snapshot(format!("decode Turso payload: {error}"))),
        turso::Value::Null
        | turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_) => Err(TursoStoreError::Snapshot(
            "Turso payload is not a blob".to_owned(),
        )),
    }
}

fn decode_blob<T>(value: turso::Value, label: &str) -> Result<T, TursoStoreError>
where
    T: serde::de::DeserializeOwned,
{
    match value {
        turso::Value::Blob(payload) => bincode::deserialize(&payload)
            .map_err(|error| TursoStoreError::Snapshot(format!("decode {label}: {error}"))),
        turso::Value::Null
        | turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_) => Err(TursoStoreError::Snapshot(format!("{label} is not a blob"))),
    }
}

async fn read_many<T>(connection: &turso::Connection, sql: &str) -> Result<Vec<T>, TursoStoreError>
where
    T: serde::de::DeserializeOwned,
{
    read_many_with_params(connection, sql, ()).await
}

async fn read_many_with_params<T>(
    connection: &turso::Connection,
    sql: &str,
    params: impl turso::IntoParams,
) -> Result<Vec<T>, TursoStoreError>
where
    T: serde::de::DeserializeOwned,
{
    let mut rows = connection
        .query(sql, params)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query Turso table: {error}")))?;
    decode_rows(&mut rows).await
}

async fn read_many_prepared<T>(
    statement: &mut turso::Statement,
    params: impl turso::IntoParams,
) -> Result<Vec<T>, TursoStoreError>
where
    T: serde::de::DeserializeOwned,
{
    let mut rows = statement
        .query(params)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query Turso table: {error}")))?;
    decode_rows(&mut rows).await
}

async fn decode_rows<T: serde::de::DeserializeOwned>(
    rows: &mut turso::Rows,
) -> Result<Vec<T>, TursoStoreError> {
    let mut items = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read Turso row: {error}")))?
    {
        match row
            .get_value(0)
            .map_err(|error| TursoStoreError::Backend(format!("read Turso payload: {error}")))?
        {
            turso::Value::Blob(payload) => {
                items.push(bincode::deserialize(&payload).map_err(|error| {
                    TursoStoreError::Snapshot(format!("decode Turso row: {error}"))
                })?)
            }
            turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "Turso row payload is not a blob".to_owned(),
                ));
            }
        }
    }
    Ok(items)
}

impl TursoGraphStore {
    /// Returns the manifest for the latest persisted generation, if present.
    #[must_use]
    pub fn latest_generation(&self) -> Option<GenerationManifest> {
        self.database_manifest().ok().flatten()
    }

    /// Returns all persisted edge IDs for diagnostics and conformance checks.
    ///
    /// # Errors
    /// Returns a stale-generation error when the generation is unavailable.
    pub fn edge_ids(&self, generation: GenerationId) -> Result<Vec<EdgeId>, StoreError> {
        Ok(self
            .edges(generation)?
            .into_iter()
            .map(|edge| edge.id)
            .collect())
    }
}
