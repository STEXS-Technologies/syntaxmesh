use serde::Deserialize;
use syntaxmesh_core::{GenerationId, Node, NodeId, NodeKind, RepositoryId};
use syntaxmesh_ownership_host::OwnerEndpoint;

use super::{CliError, Client};
use crate::cli::identifiers::{parse_node_id, parse_repository_id};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    repository: String,
    items: Vec<Node>,
    scanned_nodes: usize,
    has_more: bool,
    next_after: Option<String>,
}

pub(in crate::cli) fn resolution_diagnostics(
    owner: &OwnerEndpoint,
) -> Result<(RepositoryId, GenerationId, Vec<Node>), CliError> {
    let client = Client::new(owner)?;
    let mut selection: Option<(RepositoryId, GenerationId)> = None;
    let mut after: Option<NodeId> = None;
    let mut result = Vec::new();
    loop {
        let generation = selection.map(|(_repository, generation)| generation.0.to_hex());
        let encoded_after = after.map(|id| id.0.to_hex());
        let mut parameters = vec![("scan_limit", "100")];
        if let Some(generation) = generation.as_deref() {
            parameters.push(("generation", generation));
        }
        if let Some(seek) = encoded_after.as_deref() {
            parameters.push(("after_node", seek));
        }
        let (actual, page): (_, Page) =
            client.get_data("/api/v1/resolution-diagnostics", &parameters)?;
        let repository = parse_repository_id(&page.repository)?;
        let next = page.next_after.as_deref().map(parse_node_id).transpose()?;
        if selection.is_some_and(|expected| expected != (repository, actual))
            || page.scanned_nodes > 100
            || page.items.len() > page.scanned_nodes
            || page.has_more != next.is_some()
            || (page.has_more && page.scanned_nodes != 100)
            || next.is_some_and(|seek| after.is_some_and(|previous| seek <= previous))
        {
            return Err(invalid_page());
        }
        selection = Some((repository, actual));
        let mut last = after;
        for item in page.items {
            if !matches!(item.kind, NodeKind::ModuleResolutionDiagnostic { .. })
                || last.is_some_and(|previous| item.id <= previous)
                || next.is_some_and(|seek| item.id > seek)
            {
                return Err(invalid_page());
            }
            last = Some(item.id);
            result.push(item);
        }
        let Some(seek) = next else {
            return Ok((repository, actual, result));
        };
        after = Some(seek);
    }
}

fn invalid_page() -> CliError {
    CliError::Usage("daemon attachment failed: inconsistent diagnostic page".to_owned())
}
