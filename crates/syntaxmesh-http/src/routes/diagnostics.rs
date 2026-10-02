use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::json;
use syntaxmesh_core::{GenerationId, NodeId};
use syntaxmesh_language_sdk::LanguageExtractor;

use super::{failure, parse_id};
use crate::SyntaxMeshHttp;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    generation: Option<String>,
    after_node: Option<String>,
    scan_limit: Option<usize>,
}

pub(super) async fn diagnostics<E: LanguageExtractor + Send + 'static>(
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
    let after = match input.after_node.as_deref().map(parse_id).transpose() {
        Ok(value) => value.map(NodeId),
        Err(status) => return failure(status),
    };
    let scan_limit = input.scan_limit.unwrap_or(100);
    if scan_limit == 0 || scan_limit > 100 || (after.is_some() && selected.is_none()) {
        return failure(StatusCode::BAD_REQUEST);
    }
    match host
        .labeled_query(selected, move |query| {
            let manifest = query
                .manifest()
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            let page = query
                .module_resolution_diagnostic_page(after, scan_limit)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            let value = json!({"schema_version":1,"generation":page.generation.0.to_hex(),"data":{
                "repository":manifest.repository.0.to_hex(),"items":page.items,
                "scanned_nodes":page.scanned_nodes,"has_more":page.next_after.is_some(),
                "next_after":page.next_after.map(|id| id.0.to_hex()),
            }});
            super::neighborhood::bounded_json_bytes(&value, 4 * 1024 * 1024)
        })
        .await
    {
        Ok((_generation, bytes)) => {
            ([(header::CONTENT_TYPE, "application/json")], bytes).into_response()
        }
        Err(status) => failure(status),
    }
}
