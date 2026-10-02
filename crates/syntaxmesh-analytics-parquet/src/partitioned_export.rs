//! Restartable, page-partitioned Parquet publication.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::{collections::BTreeSet, ffi::OsString};

use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::errors::ParquetError;
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use syntaxmesh_analytics::{
    AnalyticsError, FACT_HISTORY_ARROW_SCHEMA_VERSION, FactHistoryArrowBatch,
    FactHistoryBatchReader, MAX_FACT_HISTORY_PAGE_SIZE, fact_history_arrow_schema,
};
use syntaxmesh_core::{FactRef, GenerationId, RepositoryId, WorktreeId};
use syntaxmesh_store::{FactHistoryCursor, GraphStore, StoreError};
use tempfile::NamedTempFile;

const MANIFEST_FILE: &str = "manifest.json";
const LOCK_FILE: &str = ".export.lock";
const MANIFEST_FORMAT: &str = "syntaxmesh.fact_history.parquet";
const MANIFEST_VERSION: u16 = 1;
const MAX_MANIFEST_BYTES: u64 = 1_048_576;
const ROW_GROUP_SIZE: usize = 8_192;

/// Result of exporting or resuming one snapshot-pinned fact-history dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactHistoryParquetExport {
    /// Number of committed page partitions.
    pub partition_count: u64,
    /// Number of typed attribute rows across committed partitions.
    pub row_count: u64,
    /// Whether the pinned history scan reached its end.
    pub complete: bool,
}

/// Verified identity and committed size of one Parquet fact-history export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactHistoryParquetVerification {
    /// Repository whose graph history was exported.
    pub repository: RepositoryId,
    /// Worktree whose graph history was exported.
    pub worktree: WorktreeId,
    /// Snapshot generation represented by the partitions.
    pub as_of_generation: GenerationId,
    /// Number of committed page partitions verified.
    pub partition_count: u64,
    /// Number of Arrow attribute rows verified across all partitions.
    pub row_count: u64,
    /// Whether the manifest says the snapshot scan reached its end.
    pub complete: bool,
}

/// Fully verified export whose cooperative filesystem lock remains held while
/// a downstream reader consumes the committed Parquet partitions.
pub struct VerifiedFactHistoryExport {
    destination: PathBuf,
    verification: FactHistoryParquetVerification,
    _lock: File,
}

impl VerifiedFactHistoryExport {
    /// Verified immutable identity and artifact counts.
    #[must_use]
    pub const fn verification(&self) -> FactHistoryParquetVerification {
        self.verification
    }

    /// Committed partition paths in deterministic export order.
    pub fn partition_paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        (0..self.verification.partition_count)
            .map(|sequence| partition_path(&self.destination, sequence))
    }
}

/// Errors raised while publishing or resuming Parquet artifacts.
#[derive(Debug)]
pub enum ParquetExportError {
    Analytics(AnalyticsError),
    Store(StoreError),
    Parquet(ParquetError),
    Io(io::Error),
    Manifest(String),
    DestinationNotEmpty,
    ExportIdentityMismatch,
    PartitionIntegrity(String),
    CounterOverflow,
}

impl Display for ParquetExportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ParquetExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Analytics(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Parquet(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Manifest(_)
            | Self::DestinationNotEmpty
            | Self::ExportIdentityMismatch
            | Self::PartitionIntegrity(_)
            | Self::CounterOverflow => None,
        }
    }
}

impl From<AnalyticsError> for ParquetExportError {
    fn from(error: AnalyticsError) -> Self {
        Self::Analytics(error)
    }
}

impl From<StoreError> for ParquetExportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ParquetError> for ParquetExportError {
    fn from(error: ParquetError) -> Self {
        Self::Parquet(error)
    }
}

impl From<io::Error> for ParquetExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Export a retained generation to resumable Parquet page partitions.
///
/// The destination must be a dedicated directory. Use a new directory for each
/// repository/worktree/generation tuple. Repeating this call resumes an
/// interrupted export or returns the already completed matching export.
///
/// # Errors
/// Returns an error if the snapshot is unavailable, the destination contains
/// unrelated files, the manifest does not match this export, a committed
/// checkpoint is corrupt, or file/Parquet publication fails.
pub fn export_fact_history<S: GraphStore + ?Sized>(
    store: &S,
    as_of_generation: GenerationId,
    destination: impl AsRef<Path>,
    page_size: usize,
) -> Result<FactHistoryParquetExport, ParquetExportError> {
    let destination = destination.as_ref();
    if page_size == 0 || page_size > MAX_FACT_HISTORY_PAGE_SIZE {
        return Err(ParquetExportError::Analytics(
            AnalyticsError::InvalidPageSize(page_size),
        ));
    }
    fs::create_dir_all(destination)?;
    let _lock = acquire_export_lock(destination)?;

    let snapshot = store.manifest(as_of_generation)?;
    let manifest_path = destination.join(MANIFEST_FILE);
    let mut manifest = if manifest_path.exists() {
        read_manifest(&manifest_path)?
    } else {
        ensure_new_destination(destination)?;
        ExportManifest {
            format: MANIFEST_FORMAT.to_owned(),
            manifest_version: MANIFEST_VERSION,
            schema_version: FACT_HISTORY_ARROW_SCHEMA_VERSION,
            repository: snapshot.repository,
            worktree: snapshot.worktree,
            as_of_generation,
            partition_count: 0,
            row_count: 0,
            cursor: None,
            complete: false,
        }
    };

    validate_manifest(
        &manifest,
        &snapshot.repository,
        &snapshot.worktree,
        as_of_generation,
    )?;
    validate_latest_partition_presence(destination, manifest.partition_count)?;
    if manifest.partition_count > 0 {
        validate_last_partition(destination, &manifest)?;
    }
    if manifest.complete {
        return Ok(manifest.summary());
    }
    if !manifest_path.exists() {
        persist_manifest(destination, &manifest)?;
    }

    let resume_after = manifest.cursor.map(CursorRecord::into_cursor);
    let reader = FactHistoryBatchReader::new(store, as_of_generation, page_size, resume_after)?;
    for item in reader {
        let batch = item?;
        commit_page(destination, &mut manifest, &batch)?;
        persist_manifest(destination, &manifest)?;
    }
    manifest.complete = true;
    persist_manifest(destination, &manifest)?;
    Ok(manifest.summary())
}

/// Verify every committed artifact in a fact-history Parquet export.
///
/// Unlike resume validation, this scans every partition and is proportional to
/// the total exported bytes. It does not open a graph store or mutate export
/// contents. A valid incomplete checkpoint is verifiable and is reported with
/// `complete == false`.
///
/// # Errors
/// Returns an error for an unsupported or inconsistent manifest, missing or
/// extra files, checksum mismatch, unreadable or wrongly-versioned Parquet,
/// or a row-count mismatch.
pub fn verify_fact_history_export(
    destination: impl AsRef<Path>,
) -> Result<FactHistoryParquetVerification, ParquetExportError> {
    Ok(open_verified_fact_history_export(destination)?.verification())
}

/// Fully verify an export and retain its cooperative OS lock until the
/// returned guard is dropped. Use this when a downstream adapter must read
/// the verified files after auditing them, avoiding a verify/read race with
/// another SyntaxMesh exporter.
///
/// # Errors
/// Returns an error for an unsupported or inconsistent manifest, missing or
/// extra files, checksum mismatch, unreadable or wrongly-versioned Parquet,
/// or a row-count mismatch.
pub fn open_verified_fact_history_export(
    destination: impl AsRef<Path>,
) -> Result<VerifiedFactHistoryExport, ParquetExportError> {
    let destination = destination.as_ref();
    let lock = acquire_existing_export_lock(destination)?;
    let destination = destination.canonicalize()?;
    let manifest = read_manifest(&destination.join(MANIFEST_FILE))?;
    validate_manifest_structure(&manifest)?;
    validate_export_file_set(&destination, manifest.partition_count)?;

    let mut verified_rows = 0_u64;
    for sequence in 0..manifest.partition_count {
        let checksum = fs::read_to_string(checksum_path(&destination, sequence))?;
        let actual_hash = hash_file(&partition_path(&destination, sequence))?;
        if checksum.trim() != actual_hash {
            return Err(ParquetExportError::PartitionIntegrity(format!(
                "committed partition {sequence} checksum does not match"
            )));
        }

        let file = File::open(partition_path(&destination, sequence))?;
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)?;
        if reader.schema().as_ref() != fact_history_arrow_schema().as_ref() {
            return Err(ParquetExportError::PartitionIntegrity(format!(
                "committed partition {sequence} has an unexpected Arrow schema"
            )));
        }
        let mut partition_rows = 0_u64;
        for batch in reader.build()? {
            let batch = batch.map_err(|error| {
                ParquetExportError::PartitionIntegrity(format!(
                    "read Parquet record batch: {error}"
                ))
            })?;
            partition_rows = partition_rows
                .checked_add(u64::try_from(batch.num_rows()).map_err(|error| {
                    ParquetExportError::Manifest(format!("convert partition row count: {error}"))
                })?)
                .ok_or(ParquetExportError::CounterOverflow)?;
        }
        if partition_rows == 0 {
            return Err(ParquetExportError::PartitionIntegrity(format!(
                "committed partition {sequence} is empty"
            )));
        }
        verified_rows = verified_rows
            .checked_add(partition_rows)
            .ok_or(ParquetExportError::CounterOverflow)?;
    }
    if verified_rows != manifest.row_count {
        return Err(ParquetExportError::PartitionIntegrity(format!(
            "manifest declares {} rows but partitions contain {verified_rows}",
            manifest.row_count
        )));
    }

    let verification = FactHistoryParquetVerification {
        repository: manifest.repository,
        worktree: manifest.worktree,
        as_of_generation: manifest.as_of_generation,
        partition_count: manifest.partition_count,
        row_count: verified_rows,
        complete: manifest.complete,
    };
    Ok(VerifiedFactHistoryExport {
        destination,
        verification,
        _lock: lock,
    })
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportManifest {
    format: String,
    manifest_version: u16,
    schema_version: u16,
    repository: RepositoryId,
    worktree: WorktreeId,
    as_of_generation: GenerationId,
    partition_count: u64,
    row_count: u64,
    cursor: Option<CursorRecord>,
    complete: bool,
}

impl ExportManifest {
    const fn summary(&self) -> FactHistoryParquetExport {
        FactHistoryParquetExport {
            partition_count: self.partition_count,
            row_count: self.row_count,
            complete: self.complete,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct CursorRecord {
    as_of_generation: GenerationId,
    after_fact: FactRef,
    after_valid_from: GenerationId,
}

impl CursorRecord {
    const fn from_cursor(cursor: FactHistoryCursor) -> Self {
        Self {
            as_of_generation: cursor.as_of_generation,
            after_fact: cursor.after_fact,
            after_valid_from: cursor.after_valid_from,
        }
    }

    const fn into_cursor(self) -> FactHistoryCursor {
        FactHistoryCursor {
            as_of_generation: self.as_of_generation,
            after_fact: self.after_fact,
            after_valid_from: self.after_valid_from,
        }
    }
}

fn acquire_export_lock(destination: &Path) -> Result<File, ParquetExportError> {
    let lock_path = destination.join(LOCK_FILE);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock()?;
    Ok(lock)
}

fn acquire_existing_export_lock(destination: &Path) -> Result<File, ParquetExportError> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(destination.join(LOCK_FILE))?;
    lock.lock()?;
    Ok(lock)
}

fn ensure_new_destination(destination: &Path) -> Result<(), ParquetExportError> {
    for entry in fs::read_dir(destination)? {
        let entry = entry?;
        if entry.file_name() != LOCK_FILE {
            return Err(ParquetExportError::DestinationNotEmpty);
        }
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<ExportManifest, ParquetExportError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(ParquetExportError::Manifest(
            "manifest exceeds the 1 MiB size limit".to_owned(),
        ));
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|error| ParquetExportError::Manifest(error.to_string()))
}

fn validate_manifest(
    manifest: &ExportManifest,
    repository: &RepositoryId,
    worktree: &WorktreeId,
    generation: GenerationId,
) -> Result<(), ParquetExportError> {
    validate_manifest_structure(manifest)?;
    if &manifest.repository != repository
        || &manifest.worktree != worktree
        || manifest.as_of_generation != generation
    {
        return Err(ParquetExportError::ExportIdentityMismatch);
    }
    Ok(())
}

fn validate_manifest_structure(manifest: &ExportManifest) -> Result<(), ParquetExportError> {
    if manifest.format != MANIFEST_FORMAT
        || manifest.manifest_version != MANIFEST_VERSION
        || manifest.schema_version != FACT_HISTORY_ARROW_SCHEMA_VERSION
    {
        return Err(ParquetExportError::Manifest(
            "unsupported dataset or manifest version".to_owned(),
        ));
    }
    let has_partitions = manifest.partition_count > 0;
    let has_cursor = manifest.cursor.is_some();
    if has_partitions != has_cursor
        || has_partitions != (manifest.row_count > 0)
        || manifest.row_count < manifest.partition_count
        || manifest
            .cursor
            .is_some_and(|cursor| cursor.as_of_generation != manifest.as_of_generation)
    {
        return Err(ParquetExportError::Manifest(
            "checkpoint counts and cursor are inconsistent".to_owned(),
        ));
    }
    Ok(())
}

fn validate_export_file_set(
    destination: &Path,
    partition_count: u64,
) -> Result<(), ParquetExportError> {
    let expected_file_count = partition_count
        .checked_mul(2)
        .and_then(|count| count.checked_add(2))
        .ok_or(ParquetExportError::CounterOverflow)?;
    let mut actual = BTreeSet::<OsString>::new();
    for entry in fs::read_dir(destination)? {
        let entry = entry?;
        actual.insert(entry.file_name());
    }
    if u64::try_from(actual.len()).ok() != Some(expected_file_count) {
        return Err(ParquetExportError::PartitionIntegrity(
            "export directory contains missing or extra artifacts".to_owned(),
        ));
    }
    for fixed_name in [LOCK_FILE, MANIFEST_FILE] {
        if !actual.contains(&OsString::from(fixed_name)) {
            return Err(ParquetExportError::PartitionIntegrity(format!(
                "required export artifact {fixed_name} is missing"
            )));
        }
    }
    for sequence in 0..partition_count {
        for path in [
            partition_path(destination, sequence),
            checksum_path(destination, sequence),
        ] {
            let Some(name) = path.file_name() else {
                return Err(ParquetExportError::PartitionIntegrity(
                    "partition path has no file name".to_owned(),
                ));
            };
            if !actual.contains(name) {
                return Err(ParquetExportError::PartitionIntegrity(format!(
                    "committed artifact {} is missing",
                    name.to_string_lossy()
                )));
            }
        }
    }
    Ok(())
}

fn validate_latest_partition_presence(
    destination: &Path,
    committed_count: u64,
) -> Result<(), ParquetExportError> {
    if committed_count > 0 {
        let last_sequence = committed_count
            .checked_sub(1)
            .ok_or(ParquetExportError::CounterOverflow)?;
        if !partition_path(destination, last_sequence).is_file()
            || !checksum_path(destination, last_sequence).is_file()
        {
            return Err(ParquetExportError::PartitionIntegrity(format!(
                "latest committed partition {last_sequence} or its checksum is missing"
            )));
        }
    }
    Ok(())
}

fn validate_last_partition(
    destination: &Path,
    manifest: &ExportManifest,
) -> Result<(), ParquetExportError> {
    let sequence = manifest
        .partition_count
        .checked_sub(1)
        .ok_or(ParquetExportError::CounterOverflow)?;
    let expected = fs::read_to_string(checksum_path(destination, sequence))?;
    let actual = hash_file(&partition_path(destination, sequence))?;
    if expected.trim() != actual {
        return Err(ParquetExportError::PartitionIntegrity(format!(
            "committed partition {sequence} checksum does not match"
        )));
    }
    Ok(())
}

fn commit_page(
    destination: &Path,
    manifest: &mut ExportManifest,
    batch: &FactHistoryArrowBatch,
) -> Result<(), ParquetExportError> {
    let sequence = manifest.partition_count;
    let partition = partition_path(destination, sequence);
    let checksum = checksum_path(destination, sequence);
    let (temporary, hash) = write_partition(destination, batch)?;
    if partition.exists() {
        let orphan_hash = hash_file(&partition)?;
        if orphan_hash != hash {
            return Err(ParquetExportError::PartitionIntegrity(format!(
                "uncommitted partition {sequence} differs from deterministic retry"
            )));
        }
        drop(temporary);
    } else {
        temporary
            .persist_noclobber(&partition)
            .map_err(|error| ParquetExportError::Io(error.error))?;
        sync_directory(destination)?;
    }
    persist_checksum(destination, &checksum, &hash)?;

    manifest.partition_count = manifest
        .partition_count
        .checked_add(1)
        .ok_or(ParquetExportError::CounterOverflow)?;
    manifest.row_count = manifest
        .row_count
        .checked_add(u64::try_from(batch.batch.num_rows()).map_err(|error| {
            ParquetExportError::Manifest(format!("Arrow batch row count overflow: {error}"))
        })?)
        .ok_or(ParquetExportError::CounterOverflow)?;
    manifest.cursor = batch.resume_after.map(CursorRecord::from_cursor);
    manifest.complete = false;
    Ok(())
}

fn write_partition(
    destination: &Path,
    batch: &FactHistoryArrowBatch,
) -> Result<(NamedTempFile, String), ParquetExportError> {
    let mut temporary = NamedTempFile::new_in(destination)?;
    let properties = WriterProperties::builder()
        .set_created_by("SyntaxMesh analytics".to_owned())
        .set_max_row_group_row_count(Some(ROW_GROUP_SIZE))
        .build();
    let mut writer = ArrowWriter::try_new(
        temporary.as_file_mut(),
        batch.batch.schema(),
        Some(properties),
    )?;
    writer.write(&batch.batch)?;
    writer.close()?;
    temporary.as_file().sync_all()?;
    let hash = hash_file(temporary.path())?;
    Ok((temporary, hash))
}

fn persist_checksum(destination: &Path, path: &Path, hash: &str) -> Result<(), ParquetExportError> {
    if path.exists() {
        let existing = fs::read_to_string(path)?;
        if existing.trim() != hash {
            return Err(ParquetExportError::PartitionIntegrity(
                "orphan checksum differs from deterministic retry".to_owned(),
            ));
        }
        return Ok(());
    }
    let mut temporary = NamedTempFile::new_in(destination)?;
    temporary.write_all(hash.as_bytes())?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| ParquetExportError::Io(error.error))?;
    sync_directory(destination)?;
    Ok(())
}

fn persist_manifest(
    destination: &Path,
    manifest: &ExportManifest,
) -> Result<(), ParquetExportError> {
    let path = destination.join(MANIFEST_FILE);
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| ParquetExportError::Manifest(error.to_string()))?;
    let mut temporary = NamedTempFile::new_in(destination)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(&path)
        .map_err(|error| ParquetExportError::Io(error.error))?;
    sync_directory(destination)?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, ParquetExportError> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        let chunk = buffer.get(..bytes_read).ok_or_else(|| {
            ParquetExportError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "file read exceeded the hash buffer",
            ))
        })?;
        hasher.update(chunk);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn partition_path(destination: &Path, sequence: u64) -> PathBuf {
    destination.join(format!("part-{sequence:08}.parquet"))
}

fn checksum_path(destination: &Path, sequence: u64) -> PathBuf {
    destination.join(format!("part-{sequence:08}.parquet.blake3"))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ParquetExportError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ParquetExportError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use syntaxmesh_analytics::{FACT_HISTORY_ARROW_SCHEMA_VERSION, FactHistoryBatchReader};
    use syntaxmesh_core::{
        EvidenceClass, FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, Provenance,
        ProvenanceId, RepositoryId, WorktreeId,
    };
    use syntaxmesh_store::{GraphStore, InMemoryGraphStore};
    use tempfile::tempdir;

    use super::{
        ExportManifest, FactHistoryParquetExport, FactHistoryParquetVerification, MANIFEST_FILE,
        MANIFEST_FORMAT, MANIFEST_VERSION, acquire_export_lock, commit_page, export_fact_history,
        persist_manifest, verify_fact_history_export,
    };

    #[test]
    fn export_writes_versioned_partitions_and_rerun_is_idempotent()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = InMemoryGraphStore::new();
        let repository = RepositoryId::derive(&[b"parquet-repository"]);
        let worktree = WorktreeId::derive(&[b"parquet-worktree"]);
        let generation = GenerationId::derive(&[b"parquet-generation"]);
        let file_id = FileId::derive(&[b"parquet-file"]);
        let provenance = Provenance {
            id: ProvenanceId::derive(&[b"parquet-provenance"]),
            producer_namespace: "syntaxmesh.analytics.test".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: None,
        };
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"parquet-run"]),
            expected_base: None,
            next_generation: generation,
            changed_files: vec![FileVersion {
                file_id,
                normalized_path: "src/lib.rs".to_owned(),
                content_hash: *blake3::hash(b"pub fn sample() {}").as_bytes(),
                size_bytes: 19,
            }],
            removed_files: Vec::new(),
            upsert_provenance: vec![provenance],
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;

        let temporary_directory = tempdir()?;
        let destination = temporary_directory.path().join("fact-history");

        // Simulate a stop after the page and checksum are durable but before
        // the checkpoint manifest advances.
        fs::create_dir_all(&destination)?;
        let checkpoint_lock = acquire_export_lock(&destination)?;
        let snapshot = store.manifest(generation)?;
        let mut incomplete = ExportManifest {
            format: MANIFEST_FORMAT.to_owned(),
            manifest_version: MANIFEST_VERSION,
            schema_version: FACT_HISTORY_ARROW_SCHEMA_VERSION,
            repository: snapshot.repository,
            worktree: snapshot.worktree,
            as_of_generation: generation,
            partition_count: 0,
            row_count: 0,
            cursor: None,
            complete: false,
        };
        persist_manifest(&destination, &incomplete)?;
        drop(checkpoint_lock);
        let partial_audit = verify_fact_history_export(&destination)?;
        if partial_audit
            != (FactHistoryParquetVerification {
                repository,
                worktree,
                as_of_generation: generation,
                partition_count: 0,
                row_count: 0,
                complete: false,
            })
        {
            return Err(format!("unexpected partial audit summary: {partial_audit:?}").into());
        }
        let retry_lock = acquire_export_lock(&destination)?;
        let mut reader = FactHistoryBatchReader::new(&store, generation, 1, None)?;
        let first_batch = reader
            .next()
            .ok_or("fact-history reader returned no first page")??;
        commit_page(&destination, &mut incomplete, &first_batch)?;
        drop(retry_lock);

        let first = export_fact_history(&store, generation, &destination, 1)?;
        if first
            != (FactHistoryParquetExport {
                partition_count: 2,
                row_count: 6,
                complete: true,
            })
        {
            return Err(format!("unexpected partition summary: {first:?}").into());
        }
        let verified = verify_fact_history_export(&destination)?;
        if verified
            != (FactHistoryParquetVerification {
                repository,
                worktree,
                as_of_generation: generation,
                partition_count: first.partition_count,
                row_count: first.row_count,
                complete: true,
            })
        {
            return Err(format!("unexpected verification summary: {verified:?}").into());
        }
        let manifest_path = destination.join(MANIFEST_FILE);
        let valid_manifest_bytes = fs::read(&manifest_path)?;
        let mut inconsistent_manifest: ExportManifest =
            serde_json::from_slice(&valid_manifest_bytes)?;
        inconsistent_manifest.row_count = inconsistent_manifest
            .row_count
            .checked_add(1)
            .ok_or("test manifest row count overflow")?;
        fs::write(&manifest_path, serde_json::to_vec(&inconsistent_manifest)?)?;
        match verify_fact_history_export(&destination) {
            Err(super::ParquetExportError::PartitionIntegrity(_)) => {}
            Err(error) => return Err(format!("unexpected row-count audit error: {error}").into()),
            Ok(_) => return Err("full audit accepted an inconsistent manifest row count".into()),
        }
        fs::write(&manifest_path, &valid_manifest_bytes)?;
        fs::write(destination.join("unexpected-artifact"), b"extra")?;
        match verify_fact_history_export(&destination) {
            Err(super::ParquetExportError::PartitionIntegrity(_)) => {}
            Err(error) => return Err(format!("unexpected extra-file audit error: {error}").into()),
            Ok(_) => return Err("full audit accepted an unexpected artifact".into()),
        }
        fs::remove_file(destination.join("unexpected-artifact"))?;
        let canonical_before = store.manifest(generation)?;

        let manifest_bytes = std::fs::read(destination.join(MANIFEST_FILE))?;
        let manifest: super::ExportManifest = serde_json::from_slice(&manifest_bytes)?;
        if manifest.schema_version != FACT_HISTORY_ARROW_SCHEMA_VERSION
            || manifest.partition_count != 2
            || !manifest.complete
        {
            return Err("manifest metadata is incorrect".into());
        }

        let partition = File::open(destination.join("part-00000000.parquet"))?;
        let reader_builder = ParquetRecordBatchReaderBuilder::try_new(partition)?;
        if reader_builder
            .schema()
            .metadata()
            .get("syntaxmesh.schema_version")
            != Some(&FACT_HISTORY_ARROW_SCHEMA_VERSION.to_string())
        {
            return Err("Parquet schema version metadata is missing".into());
        }
        let rows = reader_builder
            .build()?
            .map(|batch| batch.map(|batch| batch.num_rows()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum::<usize>();
        if rows == 0 {
            return Err("Parquet partition contains no typed rows".into());
        }

        let resumed = export_fact_history(&store, generation, &destination, 1)?;
        if resumed != first {
            return Err("completed export changed when resumed".into());
        }
        let first_partition = destination.join("part-00000000.parquet");
        let first_partition_bytes = fs::read(&first_partition)?;
        fs::write(&first_partition, b"corrupt older partition")?;
        match verify_fact_history_export(&destination) {
            Err(super::ParquetExportError::PartitionIntegrity(_)) => {}
            Err(error) => return Err(format!("unexpected audit error: {error}").into()),
            Ok(_) => return Err("full audit accepted a corrupt older partition".into()),
        }
        fs::write(&first_partition, first_partition_bytes)?;
        fs::write(destination.join("part-00000001.parquet"), b"corrupt")?;
        match export_fact_history(&store, generation, &destination, 1) {
            Err(super::ParquetExportError::PartitionIntegrity(_)) => {}
            Err(error) => return Err(format!("unexpected corruption error: {error}").into()),
            Ok(_) => return Err("corrupt checkpoint partition was accepted".into()),
        }
        if store.manifest(generation)? != canonical_before {
            return Err("analytics export mutated canonical graph history".into());
        }
        Ok(())
    }
}
