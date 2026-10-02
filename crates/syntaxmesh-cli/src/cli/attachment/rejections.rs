use serde::Deserialize;
use syntaxmesh_core::IndexRunId;
use syntaxmesh_engine::{WorkflowRejection, WorkflowRejectionPage, WorkflowRejectionReason};
use syntaxmesh_ownership_host::OwnerEndpoint;

use super::{CliError, Client};
use crate::cli::identifiers::{
    parse_generation_id, parse_index_run_id, parse_repository_id, parse_worktree_id,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    items: Vec<Item>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    run_id: String,
    reason: Reason,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Reason {
    UnknownLegacy {},
    StaleBase {
        repository: String,
        worktree: String,
        expected: Option<String>,
        actual: Option<String>,
    },
}

pub(in crate::cli) fn workflow_rejections(
    owner: &OwnerEndpoint,
    limit: usize,
    after: Option<IndexRunId>,
) -> Result<WorkflowRejectionPage, CliError> {
    let limit = limit.min(100);
    if limit == 0 {
        return Ok(WorkflowRejectionPage {
            items: vec![],
            next_cursor: None,
        });
    }
    let count = limit.to_string();
    let encoded_after = after.map(|run| run.0.to_hex());
    let mut parameters = vec![("limit", count.as_str())];
    if let Some(cursor) = encoded_after.as_deref() {
        parameters.push(("after_run", cursor));
    }
    let (_, page): (_, Page) =
        Client::new(owner)?.get_data("/api/v1/workflow-rejections", &parameters)?;
    let next_cursor = page
        .next_cursor
        .as_deref()
        .map(parse_index_run_id)
        .transpose()?;
    let mut items = Vec::with_capacity(page.items.len());
    let mut previous = after;
    if page.items.len() > limit {
        return Err(invalid_page());
    }
    for item in page.items {
        let run_id = parse_index_run_id(&item.run_id)?;
        if previous.is_some_and(|run| run_id <= run) {
            return Err(invalid_page());
        }
        let reason = match item.reason {
            Reason::UnknownLegacy {} => WorkflowRejectionReason::UnknownLegacy,
            Reason::StaleBase {
                repository,
                worktree,
                expected,
                actual,
            } => WorkflowRejectionReason::StaleBase {
                repository: parse_repository_id(&repository)?,
                worktree: parse_worktree_id(&worktree)?,
                expected: expected.as_deref().map(parse_generation_id).transpose()?,
                actual: actual.as_deref().map(parse_generation_id).transpose()?,
            },
        };
        items.push(WorkflowRejection { run_id, reason });
        previous = Some(run_id);
    }
    if next_cursor.is_some() && (items.len() != limit || next_cursor != previous) {
        return Err(invalid_page());
    }
    Ok(WorkflowRejectionPage { items, next_cursor })
}

fn invalid_page() -> CliError {
    CliError::Usage("daemon attachment failed: inconsistent workflow rejection page".to_owned())
}
