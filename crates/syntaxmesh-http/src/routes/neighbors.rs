use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use serde_json::json;
use syntaxmesh_api_model::HistoricalNeighborCursor;
use syntaxmesh_core::{EdgeDirection, GenerationId, NodeId};
use syntaxmesh_query::QueryError;

use super::{envelope, failure, parse_id};
use crate::SyntaxMeshHttp;
use syntaxmesh_language_sdk::LanguageExtractor;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    generation: Option<String>,
    direction: Option<EdgeDirection>,
    limit: Option<usize>,
    cursor: Option<String>,
}

fn encode_cursor(cursor: HistoricalNeighborCursor) -> Result<String, StatusCode> {
    syntaxmesh_api_model::encode_neighbor_cursor(cursor)
        .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)
}

fn decode_cursor(token: &str) -> Result<HistoricalNeighborCursor, StatusCode> {
    syntaxmesh_api_model::decode_neighbor_cursor(token).map_err(|_error| StatusCode::BAD_REQUEST)
}

pub(super) async fn neighbors<E: LanguageExtractor + Send + 'static>(
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
    let selected = match input.generation.as_deref().map(parse_id).transpose() {
        Ok(value) => value.map(GenerationId),
        Err(status) => return failure(status),
    };
    let after = match input.cursor.as_deref().map(decode_cursor).transpose() {
        Ok(value) => value,
        Err(status) => return failure(status),
    };
    let limit = input.limit.unwrap_or(20);
    if limit == 0 || limit > 100 {
        return failure(StatusCode::BAD_REQUEST);
    }
    let direction = input.direction.unwrap_or(EdgeDirection::Outgoing);
    let endpoint = NodeId(id);
    match host.labeled_query(selected, move |query| {
        if after.is_some_and(|cursor| {
            cursor.generation != query.generation() || cursor.endpoint != endpoint || cursor.direction != direction
        }) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if query.historical_node(endpoint).map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?.is_none() {
            return Err(StatusCode::NOT_FOUND);
        }
        let page = query.historical_neighbors(endpoint, direction, limit, after).map_err(|error| {
            if matches!(error, QueryError::InvalidLimit | QueryError::InvalidHistoricalNeighborCursor) {
                StatusCode::BAD_REQUEST
            } else { StatusCode::INTERNAL_SERVER_ERROR }
        })?;
        let next_cursor = page.next_cursor.map(encode_cursor).transpose()?;
        Ok(json!({
            "endpoint": endpoint.0.to_hex(),
            "direction": direction,
            "items": page.items.into_iter().map(|item| json!({"edge": item.edge, "neighbor": item.neighbor})).collect::<Vec<_>>(),
            "has_more": page.has_more,
            "next_cursor": next_cursor,
        }))
    }).await {
        Ok((generation, data)) => envelope(generation, &data),
        Err(status) => failure(status),
    }
}
