use std::path::Path;

use syntaxmesh_core::IndexRunId;
use syntaxmesh_engine::{SyntaxMeshEngine, WorkflowRejectionPage, WorkflowRejectionReason};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_ownership_host::{OwnerEndpoint, WriterLease};
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use super::{CliError, file_engine, turso_engine, write_stdout};

pub(super) fn workflow_rejections(
    snapshot: &Path,
    limit: usize,
    after: Option<IndexRunId>,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    print_rejections(&engine, limit, after)
}

pub(super) fn workflow_rejections_turso(
    database: &Path,
    limit: usize,
    after: Option<IndexRunId>,
) -> Result<(), CliError> {
    if let Some(owner) = OwnerEndpoint::discover(database)? {
        return print_page(super::attachment::workflow_rejections(
            &owner, limit, after,
        )?);
    }
    let lease = WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&lease)?;
    print_rejections(&engine, limit, after)
}

fn print_rejections<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    limit: usize,
    after: Option<IndexRunId>,
) -> Result<(), CliError> {
    let page = engine.workflow_rejections(after, limit)?;
    print_page(page)
}

fn print_page(page: WorkflowRejectionPage) -> Result<(), CliError> {
    for rejection in page.items {
        let run_id = rejection.run_id.0.to_hex();
        match rejection.reason {
            WorkflowRejectionReason::StaleBase {
                repository,
                worktree,
                expected,
                actual,
            } => write_stdout(&format!(
                "run_id={run_id} reason=stale_base repository={} worktree={} expected={} actual={}",
                repository.0.to_hex(),
                worktree.0.to_hex(),
                expected.map_or_else(|| "none".to_owned(), |id| id.0.to_hex()),
                actual.map_or_else(|| "none".to_owned(), |id| id.0.to_hex()),
            ))?,
            WorkflowRejectionReason::UnknownLegacy => {
                write_stdout(&format!("run_id={run_id} reason=unknown_legacy"))?;
            }
        }
    }
    write_stdout(&format!(
        "next_cursor={}",
        page.next_cursor
            .map_or_else(|| "none".to_owned(), |id| id.0.to_hex())
    ))
}
