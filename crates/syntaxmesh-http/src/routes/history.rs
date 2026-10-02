use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::json;
use syntaxmesh_api_model::{TEMPORAL_EXPORT_SCHEMA_VERSION, TemporalQueryMode, TemporalRecord};
use syntaxmesh_core::{EdgeId, FactRef, FileId, GenerationId, NodeId, ProvenanceId};
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_store::FactHistoryCursor;

use super::{failure, parse_id};
use crate::SyntaxMeshHttp;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    generation: Option<String>,
    limit: Option<usize>,
    after_kind: Option<String>,
    after_id: Option<String>,
    after_valid_from: Option<String>,
}

impl Input {
    fn cursor(
        &self,
        selected: Option<GenerationId>,
    ) -> Result<Option<FactHistoryCursor>, StatusCode> {
        match (&self.after_kind, &self.after_id, &self.after_valid_from) {
            (None, None, None) => Ok(None),
            (Some(kind), Some(id), Some(valid_from)) => {
                let id = parse_id(id)?;
                let fact = match kind.as_str() {
                    "file" => FactRef::File(FileId(id)),
                    "provenance" => FactRef::Provenance(ProvenanceId(id)),
                    "node" => FactRef::Node(NodeId(id)),
                    "edge" => FactRef::Edge(EdgeId(id)),
                    _ => return Err(StatusCode::BAD_REQUEST),
                };
                Ok(Some(FactHistoryCursor {
                    as_of_generation: selected.ok_or(StatusCode::BAD_REQUEST)?,
                    after_fact: fact,
                    after_valid_from: GenerationId(parse_id(valid_from)?),
                }))
            }
            _ => Err(StatusCode::BAD_REQUEST),
        }
    }
}

pub(super) async fn history<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    input: Result<Query<Input>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(input)) = input else {
        return failure(StatusCode::BAD_REQUEST);
    };
    let selected = match input.generation.as_deref().map(parse_id).transpose() {
        Ok(value) => value.map(GenerationId),
        Err(status) => return failure(status),
    };
    let after = match input.cursor(selected) {
        Ok(value) => value,
        Err(status) => return failure(status),
    };
    let limit = input.limit.unwrap_or(100);
    if limit == 0 || limit > 100 {
        return failure(StatusCode::BAD_REQUEST);
    }
    match host.labeled_query(selected, move |query| {
        let manifest = query.manifest().map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
        let page = query.fact_history_page(after, limit).map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
        let items = page.items.into_iter().map(|entry| {
            let version = entry.version;
            let record = TemporalRecord::FactVersion {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                valid_from: version.valid_from, valid_until: version.valid_until,
                observed_at: version.observed_at, accepted_at: version.accepted_at,
                payload: version.payload,
            };
            json!({"fact":entry.fact,"valid_from_sequence":entry.valid_from_sequence,
                "valid_until_sequence":entry.valid_until_sequence,"record":record})
        }).collect::<Vec<_>>();
        let next = page.next_cursor.map(|cursor| {
            let (kind, id) = match cursor.after_fact {
                FactRef::File(id) => ("file", id.0), FactRef::Provenance(id) => ("provenance", id.0),
                FactRef::Node(id) => ("node", id.0), FactRef::Edge(id) => ("edge", id.0),
            };
            json!({"generation":cursor.as_of_generation.0.to_hex(),"after_kind":kind,
                "after_id":id.to_hex(),"after_valid_from":cursor.after_valid_from.0.to_hex()})
        });
        super::neighborhood::bounded_json_bytes(&json!({"schema_version":1,
            "generation":query.generation().0.to_hex(),"data":{"repository":manifest.repository.0.to_hex(),
            "worktree":manifest.worktree.0.to_hex(),"items":items,"next_cursor":next}}),4 * 1024 * 1024)
    }).await {
        Ok((_, bytes)) => ([(header::CONTENT_TYPE,"application/json")],bytes).into_response(),
        Err(status) => failure(status),
    }
}
