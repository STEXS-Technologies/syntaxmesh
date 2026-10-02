//! Read-only DuckDB operations over a complete, verified Parquet snapshot.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use duckdb::{Connection, Error as DuckDbError};
use syntaxmesh_analytics_parquet::{
    FactHistoryParquetVerification, ParquetExportError, verify_fact_history_export,
};
use syntaxmesh_core::{GenerationId, StableId};

/// Maximum rows returned by a single generation-count query.
pub const MAX_GENERATION_COUNT_ROWS: usize = 4_096;

/// Number of unique fact identities first observed at one generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationFactCount {
    /// Opaque generation ID where this version of each fact identity became valid.
    pub valid_from_generation: GenerationId,
    /// One-based canonical chronological sequence of that generation.
    pub valid_from_sequence: u64,
    /// Distinct `(fact_kind, fact_id)` identities at this generation.
    pub fact_count: u64,
}

/// Errors raised while validating or querying a verified analytics export.
#[derive(Debug)]
pub enum AnalyticsDuckDbError {
    Export(ParquetExportError),
    DuckDb(DuckDbError),
    IncompleteExport,
    NoPartitions,
    InvalidResultLimit(usize),
    InvalidGenerationId(String),
    InvalidFactCount { value: i64, reason: String },
    PathNotUtf8(PathBuf),
    SqlTextTooLarge,
}

impl Display for AnalyticsDuckDbError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for AnalyticsDuckDbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Export(error) => Some(error),
            Self::DuckDb(error) => Some(error),
            Self::IncompleteExport
            | Self::NoPartitions
            | Self::InvalidResultLimit(_)
            | Self::InvalidGenerationId(_)
            | Self::InvalidFactCount { .. }
            | Self::PathNotUtf8(_)
            | Self::SqlTextTooLarge => None,
        }
    }
}

impl From<ParquetExportError> for AnalyticsDuckDbError {
    fn from(error: ParquetExportError) -> Self {
        Self::Export(error)
    }
}

impl From<DuckDbError> for AnalyticsDuckDbError {
    fn from(error: DuckDbError) -> Self {
        Self::DuckDb(error)
    }
}

/// Read-only DuckDB session bound to one completely verified export snapshot.
pub struct FactHistoryDuckDb {
    connection: Connection,
    verification: FactHistoryParquetVerification,
}

impl FactHistoryDuckDb {
    /// Verify a complete export and open its committed partitions in memory.
    ///
    /// This performs a full integrity audit. The adapter does not copy data to
    /// a second durable database, and its DuckDB connection remains
    /// generation-pinned to the verified partition count.
    ///
    /// # Errors
    /// Returns an error for a corrupt, incomplete, empty, or unsupported export
    /// or when DuckDB cannot create the read-only analytical view.
    pub fn open_verified_export(
        destination: impl AsRef<Path>,
    ) -> Result<Self, AnalyticsDuckDbError> {
        let destination = destination.as_ref();
        let verification = verify_fact_history_export(destination)?;
        if !verification.complete {
            return Err(AnalyticsDuckDbError::IncompleteExport);
        }
        if verification.partition_count == 0 {
            return Err(AnalyticsDuckDbError::NoPartitions);
        }

        let destination = destination.canonicalize().map_err(ParquetExportError::Io)?;
        let connection = Connection::open_in_memory()?;
        connection.execute_batch("SET threads = 1;")?;
        create_fact_history_view(&connection, &destination, verification.partition_count)?;

        Ok(Self {
            connection,
            verification,
        })
    }

    /// Identity and size of the verified snapshot queried by this session.
    #[must_use]
    pub const fn verification(&self) -> FactHistoryParquetVerification {
        self.verification
    }

    /// Count distinct facts by the generation where each fact version became
    /// valid, ordered chronologically by canonical generation sequence.
    ///
    /// # Errors
    /// Returns an error if `limit` is zero or exceeds
    /// [`MAX_GENERATION_COUNT_ROWS`], or if DuckDB returns an invalid row.
    pub fn fact_counts_by_valid_from(
        &self,
        limit: usize,
    ) -> Result<Vec<GenerationFactCount>, AnalyticsDuckDbError> {
        if limit == 0 || limit > MAX_GENERATION_COUNT_ROWS {
            return Err(AnalyticsDuckDbError::InvalidResultLimit(limit));
        }

        let sql = "SELECT hex(valid_from_generation), valid_from_generation_sequence, count(*) FROM (\
            SELECT DISTINCT fact_kind, fact_id, valid_from_generation, valid_from_generation_sequence \
            FROM fact_history_attributes\
        ) GROUP BY valid_from_generation, valid_from_generation_sequence \
        ORDER BY valid_from_generation_sequence LIMIT ?";
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
            let encoded_id: String = row.get(0)?;
            let raw_sequence: i64 = row.get(1)?;
            let raw_count: i64 = row.get(2)?;
            Ok((encoded_id, raw_sequence, raw_count))
        })?;

        let mut counts = Vec::new();
        for row in rows {
            let (encoded_id, raw_sequence, raw_count) = row?;
            let decoded_id = hex::decode(&encoded_id).map_err(|error| {
                AnalyticsDuckDbError::InvalidGenerationId(format!("{encoded_id}: {error}"))
            })?;
            let id_bytes: [u8; 32] = decoded_id.try_into().map_err(|error: Vec<u8>| {
                AnalyticsDuckDbError::InvalidGenerationId(format!(
                    "{} decodes to {} bytes, expected 32",
                    encoded_id,
                    error.len()
                ))
            })?;
            let fact_count = u64::try_from(raw_count).map_err(|error| {
                AnalyticsDuckDbError::InvalidFactCount {
                    value: raw_count,
                    reason: error.to_string(),
                }
            })?;
            let valid_from_sequence = u64::try_from(raw_sequence).map_err(|error| {
                AnalyticsDuckDbError::InvalidFactCount {
                    value: raw_sequence,
                    reason: format!("invalid generation sequence: {error}"),
                }
            })?;
            counts.push(GenerationFactCount {
                valid_from_generation: GenerationId(StableId(id_bytes)),
                valid_from_sequence,
                fact_count,
            });
        }
        Ok(counts)
    }
}

fn create_fact_history_view(
    connection: &Connection,
    destination: &Path,
    partition_count: u64,
) -> Result<(), AnalyticsDuckDbError> {
    let mut sql =
        String::from("CREATE VIEW fact_history_attributes AS SELECT * FROM read_parquet([");
    for sequence in 0..partition_count {
        if sequence > 0 {
            sql.push(',');
        }
        let partition = destination.join(format!("part-{sequence:08}.parquet"));
        let path = partition
            .to_str()
            .ok_or_else(|| AnalyticsDuckDbError::PathNotUtf8(partition.clone()))?;
        sql.push(' ');
        sql.push('\'');
        for character in path.chars() {
            if character == '\'' {
                sql.push('\'');
            }
            sql.push(character);
        }
        sql.push('\'');
        if sql.len() > 1_048_576 {
            return Err(AnalyticsDuckDbError::SqlTextTooLarge);
        }
    }
    sql.push_str("]);");
    connection.execute_batch(&sql)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use syntaxmesh_analytics_parquet::{export_fact_history, verify_fact_history_export};
    use syntaxmesh_core::{
        EvidenceClass, FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, Provenance,
        ProvenanceId, RepositoryId, WorktreeId,
    };
    use syntaxmesh_store::{GraphStore, InMemoryGraphStore};
    use tempfile::tempdir;

    use super::{AnalyticsDuckDbError, FactHistoryDuckDb, MAX_GENERATION_COUNT_ROWS};

    #[test]
    fn queries_are_generation_pinned_and_count_fact_versions() -> Result<(), Box<dyn Error>> {
        let mut store = InMemoryGraphStore::new();
        let repository = RepositoryId::derive(&[b"duckdb-repository"]);
        let worktree = WorktreeId::derive(&[b"duckdb-worktree"]);
        let first_generation = GenerationId::derive(&[b"duckdb-generation-1"]);
        let second_generation = GenerationId::derive(&[b"duckdb-generation-2"]);
        let file_id = FileId::derive(&[b"duckdb-file"]);
        let provenance = Provenance {
            id: ProvenanceId::derive(&[b"duckdb-provenance"]),
            producer_namespace: "syntaxmesh.analytics.duckdb.test".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: None,
        };

        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"duckdb-run-1"]),
            expected_base: None,
            next_generation: first_generation,
            changed_files: vec![file_version(file_id, b"version one")],
            removed_files: Vec::new(),
            upsert_provenance: vec![provenance],
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"duckdb-run-2"]),
            expected_base: Some(first_generation),
            next_generation: second_generation,
            changed_files: vec![file_version(file_id, b"version two")],
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;

        let temporary_directory = tempdir()?;
        let destination = temporary_directory.path().join("export'quote");
        export_fact_history(&store, second_generation, &destination, 16)?;

        let database = FactHistoryDuckDb::open_verified_export(&destination)?;
        require(
            database.verification().as_of_generation == second_generation,
            "the DuckDB session must remain pinned to the exported generation",
        )?;
        let counts = database.fact_counts_by_valid_from(8)?;
        require(counts.len() == 2, "two valid-from generations are expected")?;
        require(
            counts
                .iter()
                .map(|count| count.valid_from_sequence)
                .collect::<Vec<_>>()
                == [1, 2],
            "generation counts must be ordered by canonical chronology",
        )?;
        let found_generations = counts
            .iter()
            .map(|count| count.valid_from_generation)
            .collect::<std::collections::BTreeSet<_>>();
        require(
            found_generations == [first_generation, second_generation].into_iter().collect(),
            "the export query must report the two expected opaque generation IDs",
        )?;
        let counts_by_generation = counts
            .iter()
            .map(|count| (count.valid_from_generation, count.fact_count))
            .collect::<std::collections::BTreeMap<_, _>>();
        require(
            counts_by_generation
                == [(first_generation, 2), (second_generation, 1)]
                    .into_iter()
                    .collect(),
            "fact-version counts must match the direct fixture calculation",
        )?;
        require(
            counts == database.fact_counts_by_valid_from(8)?,
            "repeating a query over the same immutable export must be deterministic",
        )?;
        let zero_limit_error = database
            .fact_counts_by_valid_from(0)
            .err()
            .ok_or("zero result limit should be rejected")?;
        require(
            matches!(
                zero_limit_error,
                AnalyticsDuckDbError::InvalidResultLimit(0)
            ),
            "zero result limit should produce the typed limit error",
        )?;
        let oversized_limit_error = database
            .fact_counts_by_valid_from(MAX_GENERATION_COUNT_ROWS + 1)
            .err()
            .ok_or("oversized result limit should be rejected")?;
        require(
            matches!(
                oversized_limit_error,
                AnalyticsDuckDbError::InvalidResultLimit(_)
            ),
            "oversized result limit should produce the typed limit error",
        )?;
        Ok(())
    }

    #[test]
    fn rejects_incomplete_exports_and_unbounded_result_requests() -> Result<(), Box<dyn Error>> {
        let mut store = InMemoryGraphStore::new();
        let repository = RepositoryId::derive(&[b"duckdb-incomplete-repository"]);
        let worktree = WorktreeId::derive(&[b"duckdb-incomplete-worktree"]);
        let generation = GenerationId::derive(&[b"duckdb-incomplete-generation"]);
        let provenance = Provenance {
            id: ProvenanceId::derive(&[b"duckdb-incomplete-provenance"]),
            producer_namespace: "syntaxmesh.analytics.duckdb.test".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: None,
        };
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"duckdb-incomplete-run"]),
            expected_base: None,
            next_generation: generation,
            changed_files: vec![file_version(
                FileId::derive(&[b"duckdb-incomplete-file"]),
                b"content",
            )],
            removed_files: Vec::new(),
            upsert_provenance: vec![provenance],
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;

        let temporary_directory = tempdir()?;
        let destination = temporary_directory.path().join("incomplete");
        export_fact_history(&store, generation, &destination, 16)?;
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("manifest.json"))?)?;
        let complete = manifest
            .as_object_mut()
            .and_then(|fields| fields.get_mut("complete"))
            .ok_or("export manifest must contain the complete field")?;
        *complete = serde_json::Value::Bool(false);
        fs::write(
            destination.join("manifest.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        let verification = verify_fact_history_export(&destination)?;
        require(
            !verification.complete,
            "the modified export must remain incomplete",
        )?;
        let incomplete_error = FactHistoryDuckDb::open_verified_export(&destination)
            .err()
            .ok_or("incomplete export should not be opened")?;
        require(
            matches!(incomplete_error, AnalyticsDuckDbError::IncompleteExport),
            "incomplete exports must fail with the typed incomplete error",
        )?;
        require(
            MAX_GENERATION_COUNT_ROWS == 4_096,
            "the current result limit must remain explicit",
        )?;
        Ok(())
    }

    fn file_version(file_id: FileId, contents: &[u8]) -> FileVersion {
        FileVersion {
            file_id,
            normalized_path: "src/lib.rs".to_owned(),
            content_hash: *blake3::hash(contents).as_bytes(),
            size_bytes: u64::try_from(contents.len()).unwrap_or(u64::MAX),
        }
    }

    fn require(condition: bool, message: &str) -> Result<(), std::io::Error> {
        if condition {
            Ok(())
        } else {
            Err(std::io::Error::other(message.to_owned()))
        }
    }

    use std::error::Error;
}
