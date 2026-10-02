use std::io::{self, Write};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use syntaxmesh_core::{Edge, GenerationId, Node, NodeId};
use syntaxmesh_query::{
    HistoricalNeighborhoodLimits, HistoricalNeighborhoodOutputSizer, QueryError,
};

use super::{failure, parse_id};
use crate::SyntaxMeshHttp;
use syntaxmesh_language_sdk::LanguageExtractor;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    generation: Option<String>,
    max_hops: Option<usize>,
    max_nodes: Option<usize>,
    max_edges: Option<usize>,
    max_scanned_edges: Option<usize>,
    max_result_bytes: Option<usize>,
}

impl Input {
    fn limits(&self) -> Result<HistoricalNeighborhoodLimits, StatusCode> {
        let limits = HistoricalNeighborhoodLimits {
            max_hops: self.max_hops.unwrap_or(2),
            max_nodes: self.max_nodes.unwrap_or(64),
            max_edges: self.max_edges.unwrap_or(128),
            max_scanned_edges: self.max_scanned_edges.unwrap_or(1024),
            max_result_bytes: self.max_result_bytes.unwrap_or(262_144),
        };
        for (value, maximum) in [
            (limits.max_hops, 8),
            (limits.max_nodes, 256),
            (limits.max_edges, 256),
            (limits.max_scanned_edges, 4096),
            (limits.max_result_bytes, 1_048_576),
        ] {
            if value == 0 || value > maximum {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        Ok(limits)
    }
}

#[derive(Serialize)]
struct NodeItem {
    depth: usize,
    node: Node,
}
#[derive(Serialize)]
struct EdgeItem {
    depth: usize,
    edge: Edge,
}
#[derive(Serialize)]
struct Data {
    nodes: Vec<NodeItem>,
    edges: Vec<EdgeItem>,
    scanned_incidence_entries: usize,
    accounted_item_bytes: usize,
    truncated: bool,
}
#[derive(Serialize)]
struct Envelope {
    schema_version: u32,
    generation: String,
    data: Data,
}

struct BoundedWriter {
    written: usize,
    limit: usize,
    exceeded: bool,
    bytes: Option<Vec<u8>>,
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.len() > self.limit.saturating_sub(self.written) {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "JSON output budget exceeded",
            ));
        }
        self.written = self.written.saturating_add(buffer.len());
        if let Some(bytes) = &mut self.bytes {
            bytes.extend_from_slice(buffer);
        }
        Ok(buffer.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn bounded_json_bytes<T: Serialize>(
    value: &T,
    limit: usize,
) -> Result<Vec<u8>, StatusCode> {
    let mut writer = BoundedWriter {
        written: 0,
        limit,
        exceeded: false,
        bytes: Some(Vec::new()),
    };
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(if writer.exceeded {
            StatusCode::PAYLOAD_TOO_LARGE
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        });
    }
    writer.bytes.ok_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn json_size<T: Serialize>(value: &T, limit: usize) -> Result<Option<usize>, QueryError> {
    let mut writer = BoundedWriter {
        written: 0,
        limit,
        exceeded: false,
        bytes: None,
    };
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(Some(writer.written)),
        Err(_error) if writer.exceeded => Ok(None),
        Err(error) => Err(QueryError::Serialization(error.to_string())),
    }
}

fn item_size<T: Serialize>(value: &T, remaining: usize) -> Result<Option<usize>, QueryError> {
    let Some(limit) = remaining.checked_sub(1) else {
        return Ok(None);
    };
    Ok(json_size(value, limit)?.map(|size| size.saturating_add(1)))
}

pub(crate) struct JsonOutputSizer;

impl HistoricalNeighborhoodOutputSizer for JsonOutputSizer {
    fn metadata(
        &self,
        generation: GenerationId,
        limit: usize,
    ) -> Result<Option<usize>, QueryError> {
        json_size(
            &Envelope {
                schema_version: 1,
                generation: generation.0.to_hex(),
                data: Data {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    scanned_incidence_entries: usize::MAX,
                    accounted_item_bytes: usize::MAX,
                    truncated: false,
                },
            },
            limit,
        )
    }
    fn node(
        &self,
        _generation: GenerationId,
        depth: usize,
        node: Node,
        remaining: usize,
    ) -> Result<Option<usize>, QueryError> {
        item_size(&NodeItem { depth, node }, remaining)
    }
    fn edge(
        &self,
        _generation: GenerationId,
        depth: usize,
        edge: Edge,
        remaining: usize,
    ) -> Result<Option<usize>, QueryError> {
        item_size(&EdgeItem { depth, edge }, remaining)
    }
}

pub(super) async fn neighborhood<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    Path(id): Path<String>,
    input: Result<Query<Input>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(input)) = input else {
        return failure(StatusCode::BAD_REQUEST);
    };
    let Ok(id) = parse_id(&id) else {
        return failure(StatusCode::BAD_REQUEST);
    };
    let limits = match input.limits() {
        Ok(value) => value,
        Err(status) => return failure(status),
    };
    let selected = match input.generation.as_deref().map(parse_id).transpose() {
        Ok(value) => value.map(GenerationId),
        Err(status) => return failure(status),
    };
    match host
        .query(selected, move |query| {
            let result = query
                .historical_neighborhood_with_output(&[NodeId(id)], limits, &JsonOutputSizer)
                .map_err(|error| {
                    if matches!(error, QueryError::UnknownSeed(_)) {
                        StatusCode::NOT_FOUND
                    } else if matches!(error, QueryError::InvalidLimit) {
                        StatusCode::BAD_REQUEST
                    } else if matches!(error, QueryError::OutputBudgetTooSmall { .. }) {
                        StatusCode::PAYLOAD_TOO_LARGE
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                })?;
            let mut nodes = Vec::with_capacity(result.nodes.len());
            for traversal_node in result.nodes {
                let node = query
                    .historical_node(traversal_node.id)
                    .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
                    .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
                nodes.push(NodeItem {
                    depth: traversal_node.depth,
                    node,
                });
            }
            let data = Data {
                nodes,
                edges: result
                    .edges
                    .into_iter()
                    .map(|traversal_edge| EdgeItem {
                        depth: traversal_edge.depth,
                        edge: traversal_edge.edge,
                    })
                    .collect(),
                scanned_incidence_entries: result.scanned_incidence_entries,
                accounted_item_bytes: result.serialized_item_bytes,
                truncated: result.truncated,
            };
            let response = Envelope {
                schema_version: 1,
                generation: query.generation().0.to_hex(),
                data,
            };
            let mut writer = BoundedWriter {
                written: 0,
                limit: limits.max_result_bytes,
                exceeded: false,
                bytes: Some(Vec::new()),
            };
            serde_json::to_writer(&mut writer, &response)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            writer.bytes.ok_or(StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await
    {
        Ok(bytes) => ([("content-type", "application/json")], bytes).into_response(),
        Err(status) => failure(status),
    }
}
