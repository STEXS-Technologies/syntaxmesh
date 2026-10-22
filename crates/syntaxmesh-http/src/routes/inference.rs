use super::*;
use axum::extract::rejection::JsonRejection;
use syntaxmesh_api_model::{InferenceCheckpointRequest, InferencePageRequest};

fn input<T>(value: Result<Json<T>, JsonRejection>) -> Result<T, StatusCode> {
    value
        .map(|Json(request)| request)
        .map_err(|error| match error.status() {
            StatusCode::PAYLOAD_TOO_LARGE => StatusCode::PAYLOAD_TOO_LARGE,
            StatusCode::UNSUPPORTED_MEDIA_TYPE => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            _ => StatusCode::BAD_REQUEST,
        })
}

pub(super) async fn checkpoint<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    value: Result<Json<InferenceCheckpointRequest>, JsonRejection>,
) -> Response {
    let request = match input(value) {
        Ok(request) => request,
        Err(status) => return failure(status),
    };
    match host.inference_checkpoint(request).await {
        Ok(response) => Json(response).into_response(),
        Err(status) => failure(status),
    }
}

pub(super) async fn page<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    value: Result<Json<InferencePageRequest>, JsonRejection>,
) -> Response {
    let request = match input(value) {
        Ok(request) => request,
        Err(status) => return failure(status),
    };
    match host.inference_page(request).await {
        Ok(response) => Json(response).into_response(),
        Err(status) => failure(status),
    }
}
