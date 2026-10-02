use super::{CliError, Client};
use serde::Deserialize;
use syntaxmesh_ownership_host::OwnerEndpoint;
use syntaxmesh_store::{BackendIntegrityKind, BackendIntegrityReport};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    kind: String,
    passed: bool,
    findings: Vec<String>,
}

pub(in crate::cli) fn backend_integrity(
    owner: &OwnerEndpoint,
) -> Result<BackendIntegrityReport, CliError> {
    let (_, report): (_, Report) =
        Client::new(owner)?.get_data("/api/v1/backend-integrity", &[])?;
    let checked = BackendIntegrityReport::from_database_findings(
        BackendIntegrityKind::TursoDatabase,
        report.findings,
    );
    if report.kind != checked.kind.as_str() || report.passed != checked.passed {
        return Err(CliError::Usage(
            "daemon attachment failed: inconsistent backend integrity report".to_owned(),
        ));
    }
    Ok(checked)
}
