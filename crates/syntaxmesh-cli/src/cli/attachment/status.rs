use super::{CliError, Client};
use serde::Deserialize;
use syntaxmesh_core::{FileVersion, GenerationManifest};
use syntaxmesh_engine::{EngineIntegrity, EngineStatus, EngineWorkflowDiagnostics};
use syntaxmesh_ownership_host::OwnerEndpoint;

mod files;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    manifest: GenerationManifest,
    files: usize,
    nodes: usize,
    edges: usize,
    provenance: usize,
    graph_root_matches: bool,
    references_valid: bool,
    workflow_prepared: usize,
    workflow_completed: usize,
    workflow_rejected: usize,
    indexed_files: Option<Vec<FileVersion>>,
}

pub(in crate::cli) fn status(
    owner: &OwnerEndpoint,
    include_files: bool,
) -> Result<
    (
        EngineStatus,
        EngineWorkflowDiagnostics,
        Option<Vec<FileVersion>>,
    ),
    CliError,
> {
    let client = Client::new(owner)?;
    let (generation, report): (_, Report) =
        client.get_data("/api/v1/status", &[("include_files", "false")])?;
    if generation != report.manifest.generation || report.indexed_files.is_some() {
        return Err(CliError::Usage(
            "daemon attachment failed: inconsistent status report".to_owned(),
        ));
    }
    let indexed_files = if include_files {
        Some(files::inventory(&client, &report.manifest, report.files)?)
    } else {
        None
    };
    Ok((
        EngineStatus {
            manifest: report.manifest,
            files: report.files,
            nodes: report.nodes,
            edges: report.edges,
            provenance: report.provenance,
            integrity: EngineIntegrity {
                graph_root_matches: report.graph_root_matches,
                references_valid: report.references_valid,
            },
        },
        EngineWorkflowDiagnostics {
            prepared_operations: report.workflow_prepared,
            completed_operations: report.workflow_completed,
            rejected_operations: report.workflow_rejected,
        },
        indexed_files,
    ))
}
