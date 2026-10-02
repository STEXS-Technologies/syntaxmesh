use syntaxmesh_api_model::{
    HistoricalNeighborCursor, decode_neighbor_cursor, encode_neighbor_cursor,
};
use syntaxmesh_core::{EdgeDirection, EdgeId, GenerationId, NodeId};
use syntaxmesh_ownership_host::OwnerEndpoint;
use syntaxmesh_query::{HistoricalNeighbor, HistoricalNeighborPage};
use syntaxmesh_store::MAX_HISTORICAL_EDGE_PAGE_SIZE;

use super::{CliError, Page, invalid_page};
use crate::cli::attachment::Client;

pub(in crate::cli) fn historical_neighbors(
    owner: &OwnerEndpoint,
    generation: GenerationId,
    endpoint: NodeId,
    direction: EdgeDirection,
    limit: usize,
    after_edge: Option<EdgeId>,
) -> Result<HistoricalNeighborPage, CliError> {
    if limit == 0 || limit > MAX_HISTORICAL_EDGE_PAGE_SIZE {
        return Err(CliError::Usage(
            "invalid historical neighbor limit".to_owned(),
        ));
    }
    let client = Client::new(owner)?;
    let mut next = after_edge.map(|edge| HistoricalNeighborCursor {
        generation,
        endpoint,
        direction,
        after_edge: edge,
    });
    let mut last_edge = after_edge;
    let mut items = Vec::new();
    let generation_label = generation.0.to_hex();
    let direction_label = match direction {
        EdgeDirection::Outgoing => "outgoing",
        EdgeDirection::Incoming => "incoming",
    };
    loop {
        let requested = limit.saturating_sub(items.len()).min(100);
        let limit_label = requested.to_string();
        let token = next
            .map(encode_neighbor_cursor)
            .transpose()
            .map_err(|_error| invalid_page())?;
        let mut parameters = vec![
            ("generation", generation_label.as_str()),
            ("direction", direction_label),
            ("limit", limit_label.as_str()),
        ];
        if let Some(token) = token.as_deref() {
            parameters.push(("cursor", token));
        }
        let response: Option<(_, Page)> = client.get_optional_data(
            &format!("/api/v1/nodes/{}/neighbors", endpoint.0.to_hex()),
            &parameters,
        )?;
        let Some((actual, page)) = response else {
            if !items.is_empty() {
                return Err(invalid_page());
            }
            // Neighbor 404 conflates absent seed and absent generation. The
            // existing search route checks the selected manifest independently.
            let (actual, _): (_, Vec<syntaxmesh_core::Node>) = client.get_data(
                "/api/v1/search",
                &[
                    ("generation", &generation_label),
                    ("text", ""),
                    ("limit", "1"),
                ],
            )?;
            if actual != generation {
                return Err(invalid_page());
            }
            next = None;
            break;
        };
        if actual != generation
            || page.endpoint != endpoint.0.to_hex()
            || page.direction != direction
            || page.items.len() > requested
            || page.has_more != page.next_cursor.is_some()
            || (page.has_more && page.items.len() != requested)
        {
            return Err(invalid_page());
        }
        for item in page.items {
            let (seed, neighbor) = match direction {
                EdgeDirection::Outgoing => (item.edge.source, item.edge.target),
                EdgeDirection::Incoming => (item.edge.target, item.edge.source),
            };
            if seed != endpoint
                || neighbor != item.neighbor.id
                || last_edge.is_some_and(|previous| previous >= item.edge.id)
            {
                return Err(invalid_page());
            }
            last_edge = Some(item.edge.id);
            items.push(HistoricalNeighbor {
                edge: item.edge,
                neighbor: item.neighbor,
            });
        }
        next = page
            .next_cursor
            .as_deref()
            .map(decode_neighbor_cursor)
            .transpose()
            .map_err(|_error| invalid_page())?;
        if next.is_some_and(|cursor| {
            cursor.generation != generation
                || cursor.endpoint != endpoint
                || cursor.direction != direction
                || Some(cursor.after_edge) != last_edge
        }) {
            return Err(invalid_page());
        }
        if next.is_none() || items.len() == limit {
            break;
        }
    }
    Ok(HistoricalNeighborPage {
        generation,
        endpoint,
        direction,
        items,
        has_more: next.is_some(),
        next_cursor: next,
    })
}
