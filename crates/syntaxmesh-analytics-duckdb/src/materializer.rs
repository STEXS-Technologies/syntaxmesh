//! Restartable DuckDB materialization of SyntaxMesh temporal fact history.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use crate::analytics::GenerationFactCount;
use duckdb::{AccessMode, Config, Connection, Error as DuckDbError, params};
use syntaxmesh_analytics::{MAX_FACT_HISTORY_PAGE_SIZE, fact_version_changes_page_to_batch};
use syntaxmesh_analytics_parquet::{ParquetExportError, open_verified_fact_history_export};
use syntaxmesh_core::{
    FactRef, FileId, GenerationId, NodeId, ProvenanceId, RepositoryId, StableId, WorktreeId,
};
use syntaxmesh_store::{FactVersionChangeCursor, GraphStore, StoreError};

const MATERIALIZER_SCHEMA_VERSION: i32 = 1;
const SCHEMA_TABLE: &str = "syntaxmesh_analytics_schema";
const HISTORY_TABLE: &str = "fact_history_attributes";
const WATERMARK_TABLE: &str = "fact_history_watermarks";
const STAGING_TABLE: &str = "fact_history_attributes_staging";
const INCOMING_TABLE: &str = "fact_history_attributes_incoming";
const MAX_QUERY_ROWS: usize = 4_096;

/// Errors raised by persistent fact-history materialization.
#[derive(Debug)]
pub enum FactHistoryMaterializerError {
    DuckDb(DuckDbError),
    Export(ParquetExportError),
    Store(StoreError),
    Arrow(String),
    InvalidPageSize(usize),
    InvalidResultLimit(usize),
    UnsupportedSchema(i32),
    UnrecognizedDatabase,
    MissingSchema,
    MissingBaseline,
    IncompleteMaterialization,
    ScopeMismatch,
    GenerationSequenceMismatch { expected: u64, actual: u64 },
    PendingGenerationMismatch,
    InvalidStoredIdentity(String),
    InvalidStoredState(String),
    ExportRowCountMismatch { expected: u64, actual: u64 },
    ExportIdentityMismatch,
    ExportSequenceMismatch { expected: u64, actual: u64 },
    CounterOverflow,
}

impl Display for FactHistoryMaterializerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for FactHistoryMaterializerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DuckDb(error) => Some(error),
            Self::Export(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Arrow(_)
            | Self::InvalidPageSize(_)
            | Self::InvalidResultLimit(_)
            | Self::UnsupportedSchema(_)
            | Self::UnrecognizedDatabase
            | Self::MissingSchema
            | Self::MissingBaseline
            | Self::IncompleteMaterialization
            | Self::ScopeMismatch
            | Self::GenerationSequenceMismatch { .. }
            | Self::PendingGenerationMismatch
            | Self::InvalidStoredIdentity(_)
            | Self::InvalidStoredState(_)
            | Self::ExportRowCountMismatch { .. }
            | Self::ExportIdentityMismatch
            | Self::ExportSequenceMismatch { .. }
            | Self::CounterOverflow => None,
        }
    }
}

impl From<DuckDbError> for FactHistoryMaterializerError {
    fn from(error: DuckDbError) -> Self {
        Self::DuckDb(error)
    }
}

impl From<ParquetExportError> for FactHistoryMaterializerError {
    fn from(error: ParquetExportError) -> Self {
        Self::Export(error)
    }
}

impl From<StoreError> for FactHistoryMaterializerError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// The last completely materialized graph generation for one store scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactHistoryMaterializationWatermark {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation: GenerationId,
    pub generation_sequence: u64,
}

/// Bounded result summary for an incremental generation synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactHistoryMaterializationReceipt {
    pub watermark: FactHistoryMaterializationWatermark,
    pub changed_version_count: u64,
    pub source_page_count: u64,
    pub already_current: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingGeneration {
    generation: GenerationId,
    sequence: u64,
    cursor: Option<FactVersionChangeCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StoredWatermark {
    watermark: FactHistoryMaterializationWatermark,
    pending: Option<PendingGeneration>,
}

struct PageTransaction {
    previous: StoredWatermark,
    generation: GenerationId,
    generation_sequence: u64,
    next_cursor: Option<FactVersionChangeCursor>,
    complete: bool,
    batch: duckdb::arrow::record_batch::RecordBatch,
}

/// Durable, generation-watermarked DuckDB projection of fact-version history.
///
/// Opening validates an explicitly migrated database and never creates or
/// repairs schema objects. Use [`Self::migrate`] as the separate bootstrap
/// operation, matching SyntaxMesh's Shardline-derived migration boundary.
pub struct FactHistoryMaterializer {
    connection: Connection,
}

impl FactHistoryMaterializer {
    /// Create or validate the versioned materializer schema.
    ///
    /// Existing unknown databases are rejected. This method never upgrades an
    /// unrecognized schema implicitly.
    ///
    /// # Errors
    /// Returns an error if the database is not empty or already has an
    /// unsupported schema.
    pub fn migrate(path: impl AsRef<Path>) -> Result<(), FactHistoryMaterializerError> {
        let path = path.as_ref();
        let connection = Connection::open_with_flags(
            path,
            Config::default().access_mode(AccessMode::ReadWrite)?,
        )?;
        if table_exists(&connection, SCHEMA_TABLE)? {
            validate_schema(&connection)?;
            return Ok(());
        }
        let user_table_count: i64 = connection.query_row(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'main' \
             AND table_type = 'BASE TABLE'",
            [],
            |row| row.get(0),
        )?;
        if user_table_count != 0 {
            return Err(FactHistoryMaterializerError::UnrecognizedDatabase);
        }

        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(&format!(
            "CREATE TABLE {SCHEMA_TABLE} (schema_version INTEGER NOT NULL);\
             INSERT INTO {SCHEMA_TABLE} VALUES ({MATERIALIZER_SCHEMA_VERSION});\
             CREATE TABLE {WATERMARK_TABLE} (\
                 repository_id BLOB NOT NULL,\
                 worktree_id BLOB NOT NULL,\
                 generation_id BLOB NOT NULL,\
                 generation_sequence UBIGINT NOT NULL,\
                 pending_generation_id BLOB,\
                 pending_generation_sequence UBIGINT,\
                 cursor_fact_kind VARCHAR,\
                 cursor_fact_id BLOB,\
                 cursor_valid_from_sequence UBIGINT,\
                 PRIMARY KEY (repository_id, worktree_id)\
             );\
             {};\
             {};",
            create_history_table_sql(HISTORY_TABLE),
            materialized_view_sql()
        ))?;
        transaction.commit()?;
        validate_schema(&connection)
    }

    /// Open a migrated materializer database without applying migrations or
    /// repairing missing tables.
    ///
    /// # Errors
    /// Returns an error for a missing database, unknown schema version, or
    /// invalid schema structure.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, FactHistoryMaterializerError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(FactHistoryMaterializerError::MissingSchema);
        }
        let connection = Connection::open_with_flags(
            path,
            Config::default().access_mode(AccessMode::ReadWrite)?,
        )?;
        validate_schema(&connection)?;
        connection.execute_batch("SET threads = 1;")?;
        Ok(Self { connection })
    }

    /// Replace this materialization from one completely verified Parquet
    /// export. The export lock remains held through the transactional import.
    ///
    /// `generation_sequence` is supplied from the canonical store because an
    /// empty graph export has no row from which to derive the sequence. For a
    /// non-empty export it must match every row's pinned snapshot sequence.
    ///
    /// # Errors
    /// Returns an error for an incomplete/corrupt export, identity or sequence
    /// mismatch, failed staging, or failed transactional replacement. The
    /// prior valid materialization remains intact on failure.
    pub fn rebuild_from_verified_export(
        &mut self,
        export_path: impl AsRef<Path>,
        generation_sequence: u64,
    ) -> Result<FactHistoryMaterializationWatermark, FactHistoryMaterializerError> {
        self.rebuild_verified_export(export_path.as_ref(), generation_sequence, None)
    }

    /// Rebuild from a complete verified export only when it represents the
    /// requested canonical graph generation and store scope.
    ///
    /// The exact generation and scope are checked before staging begins. This
    /// is the safe rebuild entry point for workflow integrations that journal
    /// a graph-generation request.
    ///
    /// # Errors
    /// Returns an error when the generation is unavailable, the export does
    /// not match its manifest/scope, or the verified staging replacement fails.
    pub fn rebuild_from_graph_generation<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        generation: GenerationId,
        export_path: impl AsRef<Path>,
    ) -> Result<FactHistoryMaterializationWatermark, FactHistoryMaterializerError> {
        let manifest = store.manifest(generation)?;
        let generation_sequence = store.generation_sequence(generation)?;
        self.rebuild_verified_export(
            export_path.as_ref(),
            generation_sequence,
            Some((manifest.repository, manifest.worktree, generation)),
        )
    }

    fn rebuild_verified_export(
        &mut self,
        export_path: &Path,
        generation_sequence: u64,
        expected_identity: Option<(RepositoryId, WorktreeId, GenerationId)>,
    ) -> Result<FactHistoryMaterializationWatermark, FactHistoryMaterializerError> {
        let verified = open_verified_fact_history_export(export_path)?;
        let identity = verified.verification();
        if !identity.complete {
            return Err(FactHistoryMaterializerError::Export(
                ParquetExportError::Manifest("materializer requires a complete export".to_owned()),
            ));
        }
        let repository = identity.repository;
        let worktree = identity.worktree;
        let generation = identity.as_of_generation;
        if expected_identity.is_some_and(|expected| expected != (repository, worktree, generation))
        {
            return Err(FactHistoryMaterializerError::ExportIdentityMismatch);
        }

        let transaction = self.connection.transaction()?;
        transaction.execute_batch(&format!(
            "DROP TABLE IF EXISTS {STAGING_TABLE};\
             {};",
            create_history_table_sql(STAGING_TABLE)
        ))?;
        transaction.execute(
            &format!(
                "INSERT INTO {STAGING_TABLE} ({HISTORY_COLUMNS}) \
                 SELECT {HISTORY_COLUMNS} FROM {HISTORY_TABLE} \
                 WHERE repository_id != ? OR worktree_id != ?"
            ),
            params![repository.0.0.as_slice(), worktree.0.0.as_slice()],
        )?;
        if identity.partition_count > 0 {
            let mut paths_sql = String::from("[");
            for (index, partition) in verified.partition_paths().enumerate() {
                if index > 0 {
                    paths_sql.push(',');
                }
                let path = partition.to_str().ok_or_else(|| {
                    FactHistoryMaterializerError::Export(ParquetExportError::Manifest(
                        "partition path is not valid UTF-8".to_owned(),
                    ))
                })?;
                paths_sql.push(' ');
                paths_sql.push('\'');
                for character in path.chars() {
                    if character == '\'' {
                        paths_sql.push('\'');
                    }
                    paths_sql.push(character);
                }
                paths_sql.push('\'');
            }
            paths_sql.push(']');
            let sql = format!(
                "INSERT INTO {STAGING_TABLE} ({HISTORY_COLUMNS}) \
                 SELECT {HISTORY_COLUMNS} FROM read_parquet({paths_sql})"
            );
            transaction.execute(&sql, [])?;
        }
        let (row_count, source_sequence) =
            staging_identity(&transaction, repository, worktree, generation)?;
        if row_count != identity.row_count {
            return Err(FactHistoryMaterializerError::ExportRowCountMismatch {
                expected: identity.row_count,
                actual: row_count,
            });
        }
        if let Some(actual) = source_sequence
            && actual != generation_sequence
        {
            return Err(FactHistoryMaterializerError::ExportSequenceMismatch {
                expected: generation_sequence,
                actual,
            });
        }

        transaction.execute_batch(&format!(
            "DROP VIEW fact_history_materialized;\
             DROP TABLE {HISTORY_TABLE};\
             ALTER TABLE {STAGING_TABLE} RENAME TO {HISTORY_TABLE};\
             {}",
            materialized_view_sql()
        ))?;
        replace_watermark(
            &transaction,
            FactHistoryMaterializationWatermark {
                repository,
                worktree,
                generation,
                generation_sequence,
            },
        )?;
        transaction.commit()?;
        Ok(FactHistoryMaterializationWatermark {
            repository,
            worktree,
            generation,
            generation_sequence,
        })
    }

    /// Apply one accepted generation, resuming from the atomic page cursor if
    /// a prior attempt stopped between pages.
    ///
    /// The generation must immediately follow the complete materialization
    /// watermark. Queries for that scope fail closed until its final page is
    /// committed.
    ///
    /// # Errors
    /// Returns an error for an invalid page size, wrong store scope, generation
    /// gap, conflicting pending generation, corrupt stored cursor, or failed
    /// source/DuckDB operation.
    pub fn synchronize_generation<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        generation: GenerationId,
        page_size: usize,
    ) -> Result<FactHistoryMaterializationReceipt, FactHistoryMaterializerError> {
        if page_size == 0 || page_size > MAX_FACT_HISTORY_PAGE_SIZE {
            return Err(FactHistoryMaterializerError::InvalidPageSize(page_size));
        }
        let manifest = store.manifest(generation)?;
        let repository = manifest.repository;
        let worktree = manifest.worktree;
        let generation_sequence = store.generation_sequence(generation)?;
        let mut current = load_watermark(&self.connection, repository, worktree)?
            .ok_or(FactHistoryMaterializerError::MissingBaseline)?;

        if current.watermark.generation == generation && current.pending.is_none() {
            return Ok(FactHistoryMaterializationReceipt {
                watermark: current.watermark,
                changed_version_count: 0,
                source_page_count: 0,
                already_current: true,
            });
        }
        if current.watermark.repository != repository || current.watermark.worktree != worktree {
            return Err(FactHistoryMaterializerError::ScopeMismatch);
        }
        if let Some(pending) = current.pending {
            if pending.generation != generation || pending.sequence != generation_sequence {
                return Err(FactHistoryMaterializerError::PendingGenerationMismatch);
            }
        } else {
            let expected = current
                .watermark
                .generation_sequence
                .checked_add(1)
                .ok_or(FactHistoryMaterializerError::CounterOverflow)?;
            if generation_sequence != expected {
                return Err(FactHistoryMaterializerError::GenerationSequenceMismatch {
                    expected,
                    actual: generation_sequence,
                });
            }
        }

        let mut cursor = current.pending.and_then(|pending| pending.cursor);
        let mut page_count = 0_u64;
        let mut changed_version_count = 0_u64;
        loop {
            let page = store.fact_version_changes_at_page(generation, cursor, page_size)?;
            let version_count = u64::try_from(page.items.len())
                .map_err(|_error| FactHistoryMaterializerError::CounterOverflow)?;
            let batch = fact_version_changes_page_to_batch(
                &page,
                generation,
                generation_sequence,
                repository,
                worktree,
            )
            .map_err(|error| FactHistoryMaterializerError::Arrow(error.to_string()))?;
            let next_cursor = page.next_cursor;
            if next_cursor == cursor || (next_cursor.is_some() && page.items.is_empty()) {
                return Err(FactHistoryMaterializerError::InvalidStoredState(
                    "store change-page cursor failed to make progress".to_owned(),
                ));
            }
            let complete = next_cursor.is_none();
            current = apply_page_transaction(
                &mut self.connection,
                PageTransaction {
                    previous: current,
                    generation,
                    generation_sequence,
                    next_cursor,
                    complete,
                    batch,
                },
            )?;
            changed_version_count = changed_version_count
                .checked_add(version_count)
                .ok_or(FactHistoryMaterializerError::CounterOverflow)?;
            page_count = page_count
                .checked_add(1)
                .ok_or(FactHistoryMaterializerError::CounterOverflow)?;
            if complete {
                break;
            }
            cursor = next_cursor;
        }
        Ok(FactHistoryMaterializationReceipt {
            watermark: current.watermark,
            changed_version_count,
            source_page_count: page_count,
            already_current: false,
        })
    }

    /// Count distinct fact versions by their valid-from generation for a
    /// completely materialized scope.
    ///
    /// # Errors
    /// Returns an error while a generation is only partially synchronized or
    /// when `limit` is outside `1..=4096`.
    pub fn fact_counts_by_valid_from(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
        limit: usize,
    ) -> Result<Vec<GenerationFactCount>, FactHistoryMaterializerError> {
        if limit == 0 || limit > MAX_QUERY_ROWS {
            return Err(FactHistoryMaterializerError::InvalidResultLimit(limit));
        }
        let watermark = load_watermark(&self.connection, repository, worktree)?
            .ok_or(FactHistoryMaterializerError::MissingBaseline)?;
        if watermark.pending.is_some() {
            return Err(FactHistoryMaterializerError::IncompleteMaterialization);
        }
        query_generation_fact_counts(&self.connection, repository, worktree, limit)
    }

    /// Read the committed watermark for one repository/worktree scope.
    /// A pending generation remains represented separately and does not
    /// advance this value.
    ///
    /// # Errors
    /// Returns an error if persisted watermark data is malformed.
    pub fn watermark(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<FactHistoryMaterializationWatermark>, FactHistoryMaterializerError> {
        Ok(load_watermark(&self.connection, repository, worktree)?.map(|value| value.watermark))
    }
}

const HISTORY_COLUMNS: &str = "schema_version, repository_id, worktree_id, as_of_generation, \
    fact_kind, fact_id, valid_from_generation, valid_until_generation, attribute, ordinal, \
    value_kind, value_text, value_binary, observed_at_unix_nanos, accepted_at_unix_nanos, \
    value_uint64, value_boolean, as_of_generation_sequence, valid_from_generation_sequence, \
    valid_until_generation_sequence";

fn create_history_table_sql(table: &str) -> String {
    format!(
        "CREATE TABLE {table} (\
            schema_version USMALLINT NOT NULL,\
            repository_id BLOB NOT NULL,\
            worktree_id BLOB NOT NULL,\
            as_of_generation BLOB NOT NULL,\
            fact_kind VARCHAR NOT NULL,\
            fact_id BLOB NOT NULL,\
            valid_from_generation BLOB NOT NULL,\
            valid_until_generation BLOB,\
            attribute VARCHAR NOT NULL,\
            ordinal UINTEGER NOT NULL,\
            value_kind UTINYINT NOT NULL,\
            value_text VARCHAR,\
            value_binary BLOB,\
            observed_at_unix_nanos UBIGINT,\
            accepted_at_unix_nanos UBIGINT,\
            value_uint64 UBIGINT,\
            value_boolean BOOLEAN,\
            as_of_generation_sequence UBIGINT NOT NULL,\
            valid_from_generation_sequence UBIGINT NOT NULL,\
            valid_until_generation_sequence UBIGINT,\
            PRIMARY KEY (repository_id, worktree_id, fact_kind, fact_id, \
                         valid_from_generation, attribute, ordinal)\
        )"
    )
}

fn materialized_view_sql() -> String {
    format!(
        "CREATE VIEW fact_history_materialized AS \
         SELECT a.schema_version, a.repository_id, a.worktree_id,\
                w.generation_id AS as_of_generation, a.fact_kind, a.fact_id,\
                a.valid_from_generation, a.valid_until_generation, a.attribute,\
                a.ordinal, a.value_kind, a.value_text, a.value_binary,\
                a.observed_at_unix_nanos, a.accepted_at_unix_nanos,\
                a.value_uint64, a.value_boolean, w.generation_sequence AS as_of_generation_sequence,\
                a.valid_from_generation_sequence, a.valid_until_generation_sequence \
         FROM {HISTORY_TABLE} a JOIN {WATERMARK_TABLE} w \
           USING (repository_id, worktree_id) \
         WHERE w.pending_generation_id IS NULL;"
    )
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, DuckDbError> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM information_schema.tables \
         WHERE table_schema = 'main' AND table_type = 'BASE TABLE' AND table_name = ?",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn validate_schema(connection: &Connection) -> Result<(), FactHistoryMaterializerError> {
    if !table_exists(connection, SCHEMA_TABLE)?
        || !table_exists(connection, HISTORY_TABLE)?
        || !table_exists(connection, WATERMARK_TABLE)?
    {
        return Err(FactHistoryMaterializerError::MissingSchema);
    }
    let (version_count, version): (i64, Option<i32>) = connection.query_row(
        &format!("SELECT count(*), min(schema_version) FROM {SCHEMA_TABLE}"),
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if version_count != 1 {
        return Err(FactHistoryMaterializerError::MissingSchema);
    }
    let version = version.ok_or(FactHistoryMaterializerError::MissingSchema)?;
    if version != MATERIALIZER_SCHEMA_VERSION {
        return Err(FactHistoryMaterializerError::UnsupportedSchema(version));
    }
    require_columns(connection, SCHEMA_TABLE, &["schema_version"])?;
    require_columns(
        connection,
        WATERMARK_TABLE,
        &[
            "repository_id",
            "worktree_id",
            "generation_id",
            "generation_sequence",
            "pending_generation_id",
            "pending_generation_sequence",
            "cursor_fact_kind",
            "cursor_fact_id",
            "cursor_valid_from_sequence",
        ],
    )?;
    let fact_columns = [
        "schema_version",
        "repository_id",
        "worktree_id",
        "as_of_generation",
        "fact_kind",
        "fact_id",
        "valid_from_generation",
        "valid_until_generation",
        "attribute",
        "ordinal",
        "value_kind",
        "value_text",
        "value_binary",
        "observed_at_unix_nanos",
        "accepted_at_unix_nanos",
        "value_uint64",
        "value_boolean",
        "as_of_generation_sequence",
        "valid_from_generation_sequence",
        "valid_until_generation_sequence",
    ];
    require_columns(connection, HISTORY_TABLE, &fact_columns)?;
    require_columns(connection, "fact_history_materialized", &fact_columns)?;
    let view_count: i64 = connection.query_row(
        "SELECT count(*) FROM information_schema.views \
         WHERE table_schema = 'main' AND table_name = 'fact_history_materialized'",
        [],
        |row| row.get(0),
    )?;
    if view_count != 1 {
        return Err(FactHistoryMaterializerError::MissingSchema);
    }
    Ok(())
}

fn require_columns(
    connection: &Connection,
    table: &str,
    expected: &[&str],
) -> Result<(), FactHistoryMaterializerError> {
    let mut statement = connection.prepare(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'main' AND table_name = ? ORDER BY ordinal_position",
    )?;
    let rows = statement.query_map([table], |row| row.get::<_, String>(0))?;
    let actual = rows.collect::<Result<Vec<_>, _>>()?;
    if actual
        .iter()
        .map(String::as_str)
        .ne(expected.iter().copied())
    {
        return Err(FactHistoryMaterializerError::MissingSchema);
    }
    Ok(())
}

fn load_watermark(
    connection: &Connection,
    repository: RepositoryId,
    worktree: WorktreeId,
) -> Result<Option<StoredWatermark>, FactHistoryMaterializerError> {
    let mut statement = connection.prepare(&format!(
        "SELECT generation_id, generation_sequence, pending_generation_id, \
         pending_generation_sequence, cursor_fact_kind, cursor_fact_id, \
         cursor_valid_from_sequence FROM {WATERMARK_TABLE} \
         WHERE repository_id = ? AND worktree_id = ?"
    ))?;
    let mut rows = statement.query(params![repository.0.0.as_slice(), worktree.0.0.as_slice()])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let generation = decode_generation(row.get(0)?)?;
    let generation_sequence = row.get(1)?;
    let pending_generation: Option<Vec<u8>> = row.get(2)?;
    let pending_sequence: Option<u64> = row.get(3)?;
    let cursor_kind: Option<String> = row.get(4)?;
    let cursor_id: Option<Vec<u8>> = row.get(5)?;
    let cursor_start_sequence: Option<u64> = row.get(6)?;
    let pending = match (pending_generation, pending_sequence) {
        (None, None) => {
            if cursor_kind.is_some() || cursor_id.is_some() || cursor_start_sequence.is_some() {
                return Err(FactHistoryMaterializerError::InvalidStoredState(
                    "cursor exists without a pending generation".to_owned(),
                ));
            }
            None
        }
        (Some(id), Some(sequence)) => {
            let cursor = match (cursor_kind, cursor_id, cursor_start_sequence) {
                (None, None, None) => None,
                (Some(kind), Some(cursor_id), Some(start_sequence)) => {
                    let cursor_generation = decode_generation(id.clone())?;
                    let cursor_id = cursor_id_bytes(&kind, &cursor_id)?;
                    Some(FactVersionChangeCursor {
                        generation: cursor_generation,
                        after_fact: decode_fact_ref(&kind, cursor_id)?,
                        after_valid_from_sequence: start_sequence,
                    })
                }
                _ => {
                    return Err(FactHistoryMaterializerError::InvalidStoredState(
                        "partial generation-change cursor".to_owned(),
                    ));
                }
            };
            Some(PendingGeneration {
                generation: decode_generation(id)?,
                sequence,
                cursor,
            })
        }
        _ => {
            return Err(FactHistoryMaterializerError::InvalidStoredState(
                "partial pending-generation watermark".to_owned(),
            ));
        }
    };
    if let Some(pending) = pending
        && pending
            .cursor
            .is_some_and(|cursor| cursor.generation != pending.generation)
    {
        return Err(FactHistoryMaterializerError::InvalidStoredState(
            "pending cursor generation differs from watermark".to_owned(),
        ));
    }
    Ok(Some(StoredWatermark {
        watermark: FactHistoryMaterializationWatermark {
            repository,
            worktree,
            generation,
            generation_sequence,
        },
        pending,
    }))
}

fn cursor_id_bytes(kind: &str, id: &[u8]) -> Result<[u8; 32], FactHistoryMaterializerError> {
    id.try_into().map_err(|_error| {
        FactHistoryMaterializerError::InvalidStoredIdentity(format!(
            "{kind} cursor ID must contain 32 bytes"
        ))
    })
}

fn decode_generation(bytes: Vec<u8>) -> Result<GenerationId, FactHistoryMaterializerError> {
    let raw: [u8; 32] = bytes.try_into().map_err(|value: Vec<u8>| {
        FactHistoryMaterializerError::InvalidStoredIdentity(format!(
            "generation ID has {} bytes, expected 32",
            value.len()
        ))
    })?;
    Ok(GenerationId(StableId(raw)))
}

fn decode_fact_ref(kind: &str, bytes: [u8; 32]) -> Result<FactRef, FactHistoryMaterializerError> {
    let id = StableId(bytes);
    match kind {
        "file" => Ok(FactRef::File(FileId(id))),
        "provenance" => Ok(FactRef::Provenance(ProvenanceId(id))),
        "node" => Ok(FactRef::Node(NodeId(id))),
        "edge" => Ok(FactRef::Edge(syntaxmesh_core::EdgeId(id))),
        _ => Err(FactHistoryMaterializerError::InvalidStoredIdentity(
            format!("unknown fact kind {kind:?}"),
        )),
    }
}

fn replace_watermark(
    transaction: &duckdb::Transaction<'_>,
    watermark: FactHistoryMaterializationWatermark,
) -> Result<(), DuckDbError> {
    transaction.execute(
        &format!("DELETE FROM {WATERMARK_TABLE} WHERE repository_id = ? AND worktree_id = ?"),
        params![
            watermark.repository.0.0.as_slice(),
            watermark.worktree.0.0.as_slice()
        ],
    )?;
    transaction.execute(
        &format!(
            "INSERT INTO {WATERMARK_TABLE} \
             (repository_id, worktree_id, generation_id, generation_sequence) VALUES (?, ?, ?, ?)"
        ),
        params![
            watermark.repository.0.0.as_slice(),
            watermark.worktree.0.0.as_slice(),
            watermark.generation.0.0.as_slice(),
            watermark.generation_sequence
        ],
    )?;
    Ok(())
}

struct StagingIdentityStats {
    row_count: i64,
    repository_count: i64,
    worktree_count: i64,
    generation_count: i64,
    repo_matches: Option<bool>,
    worktree_matches: Option<bool>,
    generation_matches: Option<bool>,
    sequence_count: i64,
    min_sequence: Option<u64>,
    max_sequence: Option<u64>,
}

fn staging_identity(
    transaction: &duckdb::Transaction<'_>,
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
) -> Result<(u64, Option<u64>), FactHistoryMaterializerError> {
    let stats = transaction.query_row(
        &format!(
            "SELECT count(*), count(DISTINCT repository_id), count(DISTINCT worktree_id), \
                        count(DISTINCT as_of_generation), bool_and(repository_id = ?), \
                        bool_and(worktree_id = ?), bool_and(as_of_generation = ?), \
                        count(DISTINCT as_of_generation_sequence), \
                        min(as_of_generation_sequence), max(as_of_generation_sequence) \
                 FROM {STAGING_TABLE} WHERE repository_id = ? AND worktree_id = ?"
        ),
        params![
            repository.0.0.as_slice(),
            worktree.0.0.as_slice(),
            generation.0.0.as_slice(),
            repository.0.0.as_slice(),
            worktree.0.0.as_slice()
        ],
        |row| {
            Ok(StagingIdentityStats {
                row_count: row.get(0)?,
                repository_count: row.get(1)?,
                worktree_count: row.get(2)?,
                generation_count: row.get(3)?,
                repo_matches: row.get(4)?,
                worktree_matches: row.get(5)?,
                generation_matches: row.get(6)?,
                sequence_count: row.get(7)?,
                min_sequence: row.get(8)?,
                max_sequence: row.get(9)?,
            })
        },
    )?;
    let row_count = u64::try_from(stats.row_count)
        .map_err(|_error| FactHistoryMaterializerError::CounterOverflow)?;
    if row_count == 0 {
        return Ok((0, None));
    }
    if stats.repository_count != 1
        || stats.worktree_count != 1
        || stats.generation_count != 1
        || stats.repo_matches != Some(true)
        || stats.worktree_matches != Some(true)
        || stats.generation_matches != Some(true)
        || stats.sequence_count != 1
    {
        return Err(FactHistoryMaterializerError::ExportIdentityMismatch);
    }
    let (Some(min_sequence), Some(max_sequence)) = (stats.min_sequence, stats.max_sequence) else {
        return Err(FactHistoryMaterializerError::ExportIdentityMismatch);
    };
    if min_sequence != max_sequence {
        return Err(FactHistoryMaterializerError::ExportIdentityMismatch);
    }
    Ok((row_count, Some(min_sequence)))
}

fn apply_page_transaction(
    connection: &mut Connection,
    page: PageTransaction,
) -> Result<StoredWatermark, FactHistoryMaterializerError> {
    apply_page_transaction_inner(connection, page, false)
}

#[cfg(test)]
fn apply_page_transaction_with_failure(
    connection: &mut Connection,
    page: PageTransaction,
) -> Result<StoredWatermark, FactHistoryMaterializerError> {
    apply_page_transaction_inner(connection, page, true)
}

fn apply_page_transaction_inner(
    connection: &mut Connection,
    page: PageTransaction,
    inject_failure_after_history: bool,
) -> Result<StoredWatermark, FactHistoryMaterializerError> {
    let PageTransaction {
        previous,
        generation,
        generation_sequence,
        next_cursor,
        complete,
        batch,
    } = page;
    let transaction = connection.transaction()?;
    let repository = previous.watermark.repository;
    let worktree = previous.watermark.worktree;
    if batch.num_rows() > 0 {
        let incoming_ddl = create_history_table_sql(INCOMING_TABLE).replacen(
            "CREATE TABLE",
            "CREATE TEMP TABLE",
            1,
        );
        transaction.execute_batch(&incoming_ddl)?;
        {
            let mut appender = transaction.appender(INCOMING_TABLE)?;
            appender.append_record_batch(batch)?;
            appender.flush()?;
        }
        transaction.execute_batch(&format!(
            "DELETE FROM {HISTORY_TABLE} AS stored USING {INCOMING_TABLE} AS incoming \
             WHERE stored.repository_id = incoming.repository_id \
               AND stored.worktree_id = incoming.worktree_id \
               AND stored.fact_kind = incoming.fact_kind \
               AND stored.fact_id = incoming.fact_id \
               AND stored.valid_from_generation = incoming.valid_from_generation \
               AND stored.attribute = incoming.attribute \
               AND stored.ordinal = incoming.ordinal;\
             INSERT INTO {HISTORY_TABLE} ({HISTORY_COLUMNS}) \
             SELECT {HISTORY_COLUMNS} FROM {INCOMING_TABLE};\
             DROP TABLE {INCOMING_TABLE};"
        ))?;
    }
    if inject_failure_after_history {
        transaction.execute(
            "INSERT INTO syntaxmesh_missing_failure_injection VALUES (1)",
            [],
        )?;
    }

    let (committed, pending) = if complete {
        (
            FactHistoryMaterializationWatermark {
                repository,
                worktree,
                generation,
                generation_sequence,
            },
            None,
        )
    } else {
        (
            previous.watermark,
            Some(PendingGeneration {
                generation,
                sequence: generation_sequence,
                cursor: next_cursor,
            }),
        )
    };
    transaction.execute(
        &format!("DELETE FROM {WATERMARK_TABLE} WHERE repository_id = ? AND worktree_id = ?"),
        params![repository.0.0.as_slice(), worktree.0.0.as_slice()],
    )?;
    transaction.execute(
        &format!(
            "INSERT INTO {WATERMARK_TABLE} \
             (repository_id, worktree_id, generation_id, generation_sequence, \
              pending_generation_id, pending_generation_sequence, cursor_fact_kind, \
              cursor_fact_id, cursor_valid_from_sequence) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        params![
            repository.0.0.as_slice(),
            worktree.0.0.as_slice(),
            committed.generation.0.0.as_slice(),
            committed.generation_sequence,
            pending.map(|value| value.generation.0.0.to_vec()),
            pending.map(|value| value.sequence),
            pending.and_then(|value| value
                .cursor
                .map(|cursor| fact_kind(cursor.after_fact).to_owned())),
            pending.and_then(|value| value
                .cursor
                .map(|cursor| fact_id(cursor.after_fact).to_vec())),
            pending.and_then(|value| value.cursor.map(|cursor| cursor.after_valid_from_sequence)),
        ],
    )?;
    transaction.commit()?;
    Ok(StoredWatermark {
        watermark: committed,
        pending,
    })
}

const fn fact_kind(fact: FactRef) -> &'static str {
    match fact {
        FactRef::File(_) => "file",
        FactRef::Provenance(_) => "provenance",
        FactRef::Node(_) => "node",
        FactRef::Edge(_) => "edge",
    }
}

const fn fact_id(fact: FactRef) -> [u8; 32] {
    match fact {
        FactRef::File(id) => id.0.0,
        FactRef::Provenance(id) => id.0.0,
        FactRef::Node(id) => id.0.0,
        FactRef::Edge(id) => id.0.0,
    }
}

fn query_generation_fact_counts(
    connection: &Connection,
    repository: RepositoryId,
    worktree: WorktreeId,
    limit: usize,
) -> Result<Vec<GenerationFactCount>, FactHistoryMaterializerError> {
    let sql = format!(
        "SELECT hex(valid_from_generation), valid_from_generation_sequence, count(*) FROM (\
            SELECT DISTINCT fact_kind, fact_id, valid_from_generation, valid_from_generation_sequence \
            FROM {HISTORY_TABLE} WHERE repository_id = ? AND worktree_id = ?\
        ) GROUP BY valid_from_generation, valid_from_generation_sequence \
        ORDER BY valid_from_generation_sequence LIMIT ?"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            repository.0.0.as_slice(),
            worktree.0.0.as_slice(),
            limit as u64
        ],
        |row| {
            let encoded_id: String = row.get(0)?;
            let raw_sequence: u64 = row.get(1)?;
            let raw_count: u64 = row.get(2)?;
            Ok((encoded_id, raw_sequence, raw_count))
        },
    )?;
    let mut counts = Vec::new();
    for row in rows {
        let (encoded_id, valid_from_sequence, fact_count) = row?;
        let bytes = hex::decode(&encoded_id).map_err(|error| {
            FactHistoryMaterializerError::InvalidStoredIdentity(format!(
                "generation {encoded_id:?}: {error}"
            ))
        })?;
        let generation = decode_generation(bytes)?;
        counts.push(GenerationFactCount {
            valid_from_generation: generation,
            valid_from_sequence,
            fact_count,
        });
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use syntaxmesh_analytics::fact_version_changes_page_to_batch;
    use syntaxmesh_analytics_parquet::export_fact_history;
    use syntaxmesh_core::{
        FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, RepositoryId, WorktreeId,
    };
    use syntaxmesh_store::{GraphStore, InMemoryGraphStore};
    use tempfile::tempdir;

    use super::{
        FactHistoryMaterializer, FactHistoryMaterializerError, PageTransaction,
        apply_page_transaction, apply_page_transaction_with_failure, load_watermark,
    };
    use crate::analytics::FactHistoryDuckDb;

    #[test]
    fn verified_rebuild_then_interrupted_page_sync_resumes_to_full_export()
    -> Result<(), Box<dyn Error>> {
        let mut store = InMemoryGraphStore::new();
        let repository = RepositoryId::derive(&[b"materializer-repository"]);
        let worktree = WorktreeId::derive(&[b"materializer-worktree"]);
        let first_generation = GenerationId::derive(&[b"materializer-generation-1"]);
        let second_generation = GenerationId::derive(&[b"materializer-generation-2"]);
        let first_file = FileId::derive(&[b"materializer-file-1"]);
        let removed_file = FileId::derive(&[b"materializer-file-2"]);
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"materializer-run-1"]),
            expected_base: None,
            next_generation: first_generation,
            changed_files: vec![
                file_version(first_file, "src/one.rs", b"one-v1"),
                file_version(removed_file, "src/two.rs", b"two-v1"),
            ],
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"materializer-run-2"]),
            expected_base: Some(first_generation),
            next_generation: second_generation,
            changed_files: vec![file_version(first_file, "src/one.rs", b"one-v2")],
            removed_files: vec![removed_file],
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;

        let directory = tempdir()?;
        let baseline_export = directory.path().join("baseline");
        let current_export = directory.path().join("current");
        let database_path = directory.path().join("analytics.duckdb");
        export_fact_history(&store, first_generation, &baseline_export, 8)?;
        export_fact_history(&store, second_generation, &current_export, 8)?;
        FactHistoryMaterializer::migrate(&database_path)?;
        let mut materializer = FactHistoryMaterializer::open(&database_path)?;
        let baseline_sequence = store.generation_sequence(first_generation)?;
        materializer.rebuild_from_verified_export(&baseline_export, baseline_sequence)?;
        let baseline_counts = materializer.fact_counts_by_valid_from(repository, worktree, 8)?;
        require(
            baseline_counts.len() == 1,
            "baseline has one start generation",
        )?;
        require(
            baseline_counts.first().map(|count| count.fact_count) == Some(2),
            "baseline contains both file facts",
        )?;

        let first_page = store.fact_version_changes_at_page(second_generation, None, 1)?;
        let next_cursor = first_page
            .next_cursor
            .ok_or("fixture must span multiple generation-change pages")?;
        let second_sequence = store.generation_sequence(second_generation)?;
        let batch = fact_version_changes_page_to_batch(
            &first_page,
            second_generation,
            second_sequence,
            repository,
            worktree,
        )?;
        let stored = load_watermark(&materializer.connection, repository, worktree)?
            .ok_or("rebuild must establish a watermark")?;
        let failed = apply_page_transaction_with_failure(
            &mut materializer.connection,
            PageTransaction {
                previous: stored,
                generation: second_generation,
                generation_sequence: second_sequence,
                next_cursor: Some(next_cursor),
                complete: false,
                batch: batch.clone(),
            },
        );
        require(
            failed.is_err(),
            "injected DuckDB statement failure is returned",
        )?;
        require(
            load_watermark(&materializer.connection, repository, worktree)? == Some(stored),
            "failed page transaction leaves the previous watermark and cursor intact",
        )?;
        require(
            materializer.fact_counts_by_valid_from(repository, worktree, 8)? == baseline_counts,
            "failed page transaction rolls back fact-row replacement",
        )?;

        let partial = apply_page_transaction(
            &mut materializer.connection,
            PageTransaction {
                previous: stored,
                generation: second_generation,
                generation_sequence: second_sequence,
                next_cursor: Some(next_cursor),
                complete: false,
                batch,
            },
        )?;
        require(
            partial.watermark.generation == first_generation,
            "partial page must not advance the committed watermark",
        )?;
        drop(materializer);

        let mut resuming = FactHistoryMaterializer::open(&database_path)?;
        let incomplete_query = resuming.fact_counts_by_valid_from(repository, worktree, 8);
        require(
            matches!(
                incomplete_query,
                Err(FactHistoryMaterializerError::IncompleteMaterialization)
            ),
            "queries fail closed while a generation is partially applied",
        )?;
        let receipt = resuming.synchronize_generation(&store, second_generation, 1)?;
        require(
            receipt.watermark.generation == second_generation
                && receipt.source_page_count == 2
                && receipt.changed_version_count == 2
                && !receipt.already_current,
            "restart resumes exactly the remaining pages and commits the generation",
        )?;

        let reference = FactHistoryDuckDb::open_verified_export(&current_export)?;
        let materialized_counts = resuming.fact_counts_by_valid_from(repository, worktree, 8)?;
        let reference_counts = reference
            .fact_counts_by_valid_from(8)?
            .into_iter()
            .map(|count| super::GenerationFactCount {
                valid_from_generation: count.valid_from_generation,
                valid_from_sequence: count.valid_from_sequence,
                fact_count: count.fact_count,
            })
            .collect::<Vec<_>>();
        require(
            materialized_counts == reference_counts,
            "incremental materialization equals the full verified export",
        )?;
        let closed_versions: u64 = resuming.connection.query_row(
            "SELECT count(DISTINCT fact_id) FROM fact_history_materialized \
             WHERE repository_id = ? AND worktree_id = ? \
               AND valid_until_generation_sequence = 2",
            duckdb::params![repository.0.0.as_slice(), worktree.0.0.as_slice()],
            |row| row.get(0),
        )?;
        require(
            closed_versions == 2,
            "update and deletion both close one version",
        )?;
        require(
            resuming
                .synchronize_generation(&store, second_generation, 1)?
                .already_current,
            "repeating a completed generation is idempotent",
        )?;
        drop(resuming);

        let reopened = FactHistoryMaterializer::open(&database_path)?;
        require(
            reopened
                .watermark(repository, worktree)?
                .ok_or("watermark must survive restart")?
                .generation
                == second_generation,
            "completed watermark survives restart",
        )?;
        drop(reopened);

        std::fs::write(current_export.join("part-00000000.parquet"), b"corrupt")?;
        let mut rebuild_retry = FactHistoryMaterializer::open(&database_path)?;
        require(
            rebuild_retry
                .rebuild_from_verified_export(&current_export, second_sequence)
                .is_err(),
            "a failed verifier must not replace the last good materialization",
        )?;
        require(
            rebuild_retry
                .watermark(repository, worktree)?
                .ok_or("failed rebuild must preserve the last good watermark")?
                .generation
                == second_generation,
            "failed rebuild leaves the prior watermark intact",
        )?;
        Ok(())
    }

    fn require(condition: bool, message: &str) -> Result<(), std::io::Error> {
        if condition {
            Ok(())
        } else {
            Err(std::io::Error::other(message.to_owned()))
        }
    }

    fn file_version(file_id: FileId, normalized_path: &str, bytes: &[u8]) -> FileVersion {
        FileVersion {
            file_id,
            normalized_path: normalized_path.to_owned(),
            content_hash: *blake3::hash(bytes).as_bytes(),
            size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        }
    }
}
