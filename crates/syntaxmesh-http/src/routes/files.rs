use super::{failure, parse_id};
use crate::SyntaxMeshHttp;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::json;
use syntaxmesh_core::{FileId, GenerationId};
use syntaxmesh_language_sdk::LanguageExtractor;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    generation: Option<String>,
    after_file: Option<String>,
    limit: Option<usize>,
}

pub(super) async fn files<E: LanguageExtractor + Send + 'static>(
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
    let after = match input.after_file.as_deref().map(parse_id).transpose() {
        Ok(value) => value.map(FileId),
        Err(status) => return failure(status),
    };
    let limit = input.limit.unwrap_or(100);
    if limit == 0 || limit > 100 || (after.is_some() && selected.is_none()) {
        return failure(StatusCode::BAD_REQUEST);
    }
    match host.labeled_query(selected, move |query| {
        let manifest = query.manifest().map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
        let page = query.historical_files_page(after, limit).map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
        let next = if page.has_more { Some(page.items.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?.file_id.0.to_hex()) } else { None };
        let value = json!({"schema_version":1,"generation":query.generation().0.to_hex(),"data":{
            "repository":manifest.repository.0.to_hex(),"worktree":manifest.worktree.0.to_hex(),
            "items":page.items,"has_more":page.has_more,"next_after":next,
        }});
        super::neighborhood::bounded_json_bytes(&value, 4 * 1024 * 1024)
    }).await {
        Ok((_, bytes)) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
        Err(status) => failure(status),
    }
}
