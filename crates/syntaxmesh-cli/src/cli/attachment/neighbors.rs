use std::collections::BTreeSet;

use serde::Deserialize;
use syntaxmesh_core::{Edge, EdgeDirection, EdgeId, GenerationId, Node, NodeId};
use syntaxmesh_ownership_host::OwnerEndpoint;

use super::{CliError, Client};

pub(super) mod historical;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    endpoint: String,
    direction: EdgeDirection,
    items: Vec<Item>,
    has_more: bool,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    edge: Edge,
    neighbor: Node,
}

pub(in crate::cli) fn neighbors(
    owner: &OwnerEndpoint,
    seed: NodeId,
) -> Result<Vec<(Edge, Node)>, CliError> {
    let client = Client::new(owner)?;
    let mut pinned: Option<GenerationId> = None;
    let mut cursor: Option<String> = None;
    let mut seen_cursors = BTreeSet::new();
    let mut last_edge: Option<EdgeId> = None;
    let mut result = Vec::new();
    loop {
        let generation = pinned.map(|id| id.0.to_hex());
        let mut parameters = vec![("limit", "100"), ("direction", "outgoing")];
        if let Some(label) = generation.as_deref() {
            parameters.push(("generation", label));
        }
        if let Some(token) = cursor.as_deref() {
            parameters.push(("cursor", token));
        }
        let response: Option<(_, Page)> = client.get_optional_data(
            &format!("/api/v1/nodes/{}/neighbors", seed.0.to_hex()),
            &parameters,
        )?;
        let Some((actual, page)) = response else {
            if pinned.is_none() {
                return Ok(Vec::new());
            }
            return Err(invalid_page());
        };
        if pinned.is_some_and(|expected| expected != actual)
            || page.endpoint != seed.0.to_hex()
            || page.direction != EdgeDirection::Outgoing
            || page.items.len() > 100
            || page.has_more != page.next_cursor.is_some()
            || (page.has_more && page.items.is_empty())
        {
            return Err(invalid_page());
        }
        pinned = Some(actual);
        for item in page.items {
            if item.edge.source != seed
                || item.edge.target != item.neighbor.id
                || last_edge.is_some_and(|previous| previous >= item.edge.id)
            {
                return Err(invalid_page());
            }
            last_edge = Some(item.edge.id);
            result.push((item.edge, item.neighbor));
        }
        let Some(next) = page.next_cursor else {
            return Ok(result);
        };
        if next.is_empty() || next.len() > 2048 || !seen_cursors.insert(next.clone()) {
            return Err(invalid_page());
        }
        cursor = Some(next);
    }
}

fn invalid_page() -> CliError {
    CliError::Usage("daemon attachment failed: inconsistent neighbor page".to_owned())
}
