use axum::Json;
use axum::extract::{State, rejection::JsonRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use syntaxmesh_api_model::ContextRequest;
use syntaxmesh_core::{GenerationId, NodeId};
use syntaxmesh_query::QueryError;

use super::{failure, parse_id};
use crate::SyntaxMeshHttp;
use syntaxmesh_language_sdk::LanguageExtractor;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    query: String,
    token_budget: u64,
    generation: Option<String>,
    seed_nodes: Option<Vec<String>>,
    max_hops: Option<u8>,
    max_candidates: Option<u16>,
}

impl Input {
    fn request(self) -> Result<(Option<GenerationId>, ContextRequest), StatusCode> {
        let seeds = self.seed_nodes.unwrap_or_default();
        let hops = self.max_hops.unwrap_or(1);
        let candidates = self.max_candidates.unwrap_or(64);
        if self.query.len() > 4096
            || seeds.len() > 64
            || !(1..=32768).contains(&self.token_budget)
            || hops > 8
            || !(1..=256).contains(&candidates)
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        let generation = self
            .generation
            .as_deref()
            .map(parse_id)
            .transpose()?
            .map(GenerationId);
        let seed_nodes = seeds
            .iter()
            .map(|id| parse_id(id).map(NodeId))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((
            generation,
            ContextRequest {
                query: self.query,
                seed_nodes,
                token_budget: self.token_budget,
                max_hops: hops,
                max_candidates: candidates,
            },
        ))
    }
}

pub(super) async fn context<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    input: Result<Json<Input>, JsonRejection>,
) -> Response {
    let input = match input {
        Ok(Json(input)) => input,
        Err(error) => {
            return failure(match error.status() {
                StatusCode::PAYLOAD_TOO_LARGE => StatusCode::PAYLOAD_TOO_LARGE,
                StatusCode::UNSUPPORTED_MEDIA_TYPE => StatusCode::UNSUPPORTED_MEDIA_TYPE,
                _ => StatusCode::BAD_REQUEST,
            });
        }
    };
    let (generation, request) = match input.request() {
        Ok(request) => request,
        Err(status) => return failure(status),
    };
    let Some((reader, counter)) = host.context.clone() else {
        return failure(StatusCode::SERVICE_UNAVAILABLE);
    };
    let counter = counter.for_request();
    match host
        .query(generation, move |query| {
            let result = if generation.is_some() {
                query.historical_context(&request, &reader, &counter)
            } else {
                query.context(&request, &reader, &counter)
            };
            result.map_err(|error| match error {
                QueryError::UnknownSeed(_) => StatusCode::NOT_FOUND,
                QueryError::InvalidContextRequest => StatusCode::BAD_REQUEST,
                QueryError::ContextBudgetTooSmall { .. } => StatusCode::PAYLOAD_TOO_LARGE,
                QueryError::Store(_)
                | QueryError::InvalidLimit
                | QueryError::Context(_)
                | QueryError::OutputBudgetTooSmall { .. }
                | QueryError::Serialization(_)
                | QueryError::InvalidCorrelationAnchor
                | QueryError::InvalidHistoricalNeighborCursor
                | QueryError::UnknownAcceptanceHistory(_)
                | QueryError::GenerationNotKnownBy { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            })
        })
        .await
    {
        Ok(pack) => Json(pack).into_response(),
        Err(status) => failure(status),
    }
}
