use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use syntaxmesh_core::IndexRunId;
use syntaxmesh_engine::{WorkflowRejection, WorkflowRejectionReason};
use syntaxmesh_language_sdk::LanguageExtractor;

use super::{failure, parse_id};
use crate::SyntaxMeshHttp;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    limit: Option<usize>,
    after_run: Option<String>,
}

pub(super) async fn rejections<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    input: Result<Query<Input>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(input)) = input else {
        return failure(StatusCode::BAD_REQUEST);
    };
    let limit = input.limit.unwrap_or(20);
    if limit == 0 || limit > 100 {
        return failure(StatusCode::BAD_REQUEST);
    }
    let after = match input.after_run.as_deref().map(parse_id).transpose() {
        Ok(value) => value.map(IndexRunId),
        Err(status) => return failure(status),
    };
    let (generation, page) = match host.workflow_rejections(after, limit).await {
        Ok(result) => result,
        Err(status) => return failure(status),
    };
    let value = json!({"schema_version":1,"generation":generation.0.to_hex(),"data":{
        "items":page.items.into_iter().map(item).collect::<Vec<_>>(),
        "next_cursor":page.next_cursor.map(|run| run.0.to_hex()),
    }});
    match super::neighborhood::bounded_json_bytes(&value, 4 * 1024 * 1024) {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
        Err(status) => failure(status),
    }
}

fn item(rejection: WorkflowRejection) -> Value {
    let reason = match rejection.reason {
        WorkflowRejectionReason::UnknownLegacy => json!({"kind":"unknown_legacy"}),
        WorkflowRejectionReason::StaleBase {
            repository,
            worktree,
            expected,
            actual,
        } => json!({
            "kind":"stale_base","repository":repository.0.to_hex(),"worktree":worktree.0.to_hex(),
            "expected":expected.map(|generation| generation.0.to_hex()),
            "actual":actual.map(|generation| generation.0.to_hex()),
        }),
    };
    json!({"run_id":rejection.run_id.0.to_hex(),"reason":reason})
}
