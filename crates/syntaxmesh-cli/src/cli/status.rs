use std::path::Path;

use syntaxmesh_core::{FileVersion, RepositoryId, WorktreeId};
use syntaxmesh_engine::{
    EngineIndexFreshness, EngineStatus, EngineWorkflowDiagnostics, SyntaxMeshEngine,
};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_ownership_host::{OwnerEndpoint, WriterLease};
use syntaxmesh_scanner::{SUPPORTED_SOURCE_EXTENSIONS, scan};
use syntaxmesh_store::{
    BackendIntegrityCheck, BackendIntegrityReport, DurableRecordStore, FileGraphStore, GraphStore,
};
use syntaxmesh_store_turso::TursoGraphStore;

use super::{CliError, file_engine, indexing, turso_engine, write_stdout};

pub(super) struct SourceInventory {
    repository: RepositoryId,
    worktree: WorktreeId,
    files: Vec<FileVersion>,
}

fn source_inventory(root: &Path) -> Result<SourceInventory, CliError> {
    let canonical_root = std::fs::canonicalize(root)?;
    let (repository, worktree) = indexing::repository_scope(&canonical_root);
    let scan_report = scan(&canonical_root, SUPPORTED_SOURCE_EXTENSIONS)?;
    Ok(SourceInventory {
        repository,
        worktree,
        files: scan_report
            .files
            .into_iter()
            .map(|source| source.file)
            .collect(),
    })
}

pub(super) fn status(snapshot: &Path, source_root: Option<&Path>) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    let inventory = source_root.map(source_inventory).transpose()?;
    status_engine(&engine, "snapshot", inventory.as_ref())
}

pub(super) fn status_turso(database: &Path, source_root: Option<&Path>) -> Result<(), CliError> {
    if let Some(owner) = OwnerEndpoint::discover(database)? {
        let inventory = source_root.map(source_inventory).transpose()?;
        let (status, workflow, indexed_files) =
            super::attachment::status(&owner, inventory.is_some())?;
        let freshness = inventory
            .as_ref()
            .map(|source| {
                check_scope(&status, source)?;
                let files = indexed_files.as_deref().ok_or_else(|| {
                    CliError::Usage(
                        "daemon attachment failed: indexed inventory missing".to_owned(),
                    )
                })?;
                Ok::<_, CliError>(EngineIndexFreshness::compare(files, &source.files))
            })
            .transpose()?;
        return print_status(&status, workflow, freshness);
    }
    let lease = WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&lease)?;
    let inventory = source_root.map(source_inventory).transpose()?;
    status_engine(&engine, "Turso database", inventory.as_ref())
}

pub(super) fn integrity_snapshot(snapshot: &Path) -> Result<(), CliError> {
    let store = FileGraphStore::open(snapshot)?;
    backend_integrity(&store)
}

pub(super) fn integrity_turso(database: &Path) -> Result<(), CliError> {
    if let Some(owner) = OwnerEndpoint::discover(database)? {
        return print_backend_integrity(&super::attachment::backend_integrity(&owner)?);
    }
    let _lease = WriterLease::acquire(database)?;
    let store = TursoGraphStore::open(database)?;
    backend_integrity(&store)
}

fn backend_integrity<S: BackendIntegrityCheck>(store: &S) -> Result<(), CliError> {
    let report = store.backend_integrity_check()?;
    print_backend_integrity(&report)
}

fn print_backend_integrity(report: &BackendIntegrityReport) -> Result<(), CliError> {
    let findings = report
        .findings
        .iter()
        .map(|finding| finding.replace(['\n', '\r'], " "))
        .collect::<Vec<_>>()
        .join(" | ");
    write_stdout(&format!(
        "backend_integrity_kind={}\nbackend_integrity={}\nbackend_integrity_findings={findings}",
        report.kind.as_str(),
        if report.passed { "ok" } else { "failed" },
    ))?;
    if !report.passed {
        return Err(CliError::Integrity(format!(
            "{}: {findings}",
            report.kind.as_str()
        )));
    }
    Ok(())
}

fn status_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    scope_label: &str,
    inventory: Option<&SourceInventory>,
) -> Result<(), CliError> {
    let status = engine
        .status()?
        .ok_or_else(|| CliError::Usage(format!("{scope_label} has no published generation")))?;
    let workflow = engine.workflow_diagnostics()?;
    let freshness =
        if let Some(inventory) = inventory {
            check_scope(&status, inventory)?;
            Some(engine.index_freshness(&inventory.files)?.ok_or_else(|| {
                CliError::Usage(format!("{scope_label} has no current generation"))
            })?)
        } else {
            None
        };
    print_status(&status, workflow, freshness)
}

fn check_scope(status: &EngineStatus, inventory: &SourceInventory) -> Result<(), CliError> {
    if status.manifest.repository != inventory.repository
        || status.manifest.worktree != inventory.worktree
    {
        return Err(CliError::Usage(
            "source root does not match the repository/worktree indexed in this store".to_owned(),
        ));
    }
    Ok(())
}

fn print_status(
    status: &EngineStatus,
    workflow: EngineWorkflowDiagnostics,
    freshness: Option<EngineIndexFreshness>,
) -> Result<(), CliError> {
    let freshness_label = match freshness {
        Some(value) if value.is_current() => "current",
        Some(_) => "stale",
        None => "not_checked",
    };
    let unindexed_files = freshness.map_or(0, |value| value.unindexed_files);
    let changed_files = freshness.map_or(0, |value| value.changed_files);
    let removed_files = freshness.map_or(0, |value| value.removed_files);
    write_stdout(&format!(
        "generation={}\nstatus={:?}\nlogical_integrity={}\ngraph_root_matches={}\nreferences_valid={}\nindex_freshness={freshness_label}\nindex_unindexed_files={unindexed_files}\nindex_changed_files={changed_files}\nindex_removed_files={removed_files}\nfiles={files}\nnodes={nodes}\nedges={edges}\nprovenance={provenance}\nworkflow_prepared={prepared}\nworkflow_completed={completed}\nworkflow_rejected={rejected}",
        status.manifest.generation.0.to_hex(),
        status.manifest.status,
        if status.integrity.is_valid() {
            "ok"
        } else {
            "failed"
        },
        status.integrity.graph_root_matches,
        status.integrity.references_valid,
        files = status.files,
        nodes = status.nodes,
        edges = status.edges,
        provenance = status.provenance,
        prepared = workflow.prepared_operations,
        completed = workflow.completed_operations,
        rejected = workflow.rejected_operations,
    ))?;
    if !status.integrity.is_valid() {
        return Err(CliError::Integrity(format!(
            "graph_root_matches={}, references_valid={}",
            status.integrity.graph_root_matches, status.integrity.references_valid
        )));
    }
    if let Some(freshness) = freshness.filter(|value| !value.is_current()) {
        return Err(CliError::IndexLag(format!(
            "unindexed={}, changed={}, removed={}",
            freshness.unindexed_files, freshness.changed_files, freshness.removed_files
        )));
    }
    Ok(())
}
