use super::failure;
use crate::SyntaxMeshHttp;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::json;
use syntaxmesh_language_sdk::LanguageExtractor;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {}

pub(super) async fn integrity<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    input: Result<Query<Input>, axum::extract::rejection::QueryRejection>,
) -> Response {
    if input.is_err() {
        return failure(StatusCode::BAD_REQUEST);
    }
    let (generation, report) = match host.backend_integrity().await {
        Ok(value) => value,
        Err(status) => return failure(status),
    };
    let value = json!({"schema_version":1,"generation":generation.0.to_hex(),"data":{
        "kind":report.kind.as_str(),"passed":report.passed,"findings":report.findings,
    }});
    match super::neighborhood::bounded_json_bytes(&value, 4 * 1024 * 1024) {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
        Err(status) => failure(status),
    }
}
