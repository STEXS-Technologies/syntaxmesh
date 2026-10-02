use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use syntaxmesh_core::GenerationId;
use syntaxmesh_engine::EngineWorkflowDiagnostics;

use super::failure;
use crate::SyntaxMeshHttp;
use syntaxmesh_language_sdk::LanguageExtractor;

#[cfg(test)]
mod tests;

pub(super) async fn ready<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
) -> Response {
    match host.readiness().await {
        Ok((generation, diagnostics)) => report(generation, diagnostics),
        Err(_status) => failure(StatusCode::SERVICE_UNAVAILABLE),
    }
}

fn report(generation: GenerationId, diagnostics: EngineWorkflowDiagnostics) -> Response {
    let ready = diagnostics.prepared_operations == 0;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({
            "schema_version":1,
            "status":if ready { "ready" } else { "not_ready" },
            "generation":generation.0.to_hex(),
            "recovery":{
                "prepared_operations":diagnostics.prepared_operations,
                "completed_operations":diagnostics.completed_operations,
                "rejected_operations":diagnostics.rejected_operations,
            }
        })),
    )
        .into_response()
}
