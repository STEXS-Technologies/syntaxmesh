use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::json;
use syntaxmesh_language_sdk::LanguageExtractor;

use super::failure;
use crate::SyntaxMeshHttp;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    include_files: Option<bool>,
}

pub(super) async fn status<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    input: Result<Query<Input>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(input)) = input else {
        return failure(StatusCode::BAD_REQUEST);
    };
    match host.read_engine(move |_, engine| {
        let status = engine.status().map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::CONFLICT)?;
        let workflow = engine.workflow_diagnostics().map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
        let indexed_files = if input.include_files.unwrap_or(false) {
            Some(engine.query(status.manifest.generation).files().map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?)
        } else { None };
        let value = json!({"schema_version":1,"generation":status.manifest.generation.0.to_hex(),"data":{
            "manifest":status.manifest,"files":status.files,"nodes":status.nodes,"edges":status.edges,"provenance":status.provenance,
            "graph_root_matches":status.integrity.graph_root_matches,"references_valid":status.integrity.references_valid,
            "workflow_prepared":workflow.prepared_operations,"workflow_completed":workflow.completed_operations,"workflow_rejected":workflow.rejected_operations,
            "indexed_files":indexed_files,
        }});
        super::neighborhood::bounded_json_bytes(&value, 4 * 1024 * 1024)
    }).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
        Err(status) => failure(status),
    }
}
