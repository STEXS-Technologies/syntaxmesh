use axum::extract::{Path, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use syntaxmesh_core::{GenerationId, NodeId, StableId};

use crate::SyntaxMeshHttp;
use syntaxmesh_language_sdk::LanguageExtractor;

mod context;
mod diagnostics;
mod files;
mod history;
mod integrity;
mod neighborhood;
mod neighbors;
mod readiness;
mod rejections;
mod status;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchInput {
    text: String,
    limit: Option<usize>,
    generation: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeInput {
    generation: Option<String>,
}

#[derive(Clone)]
struct Boundary {
    authority: String,
    owner_instance: Option<axum::http::HeaderValue>,
}

pub(crate) fn router<E: LanguageExtractor + Send + 'static>(
    host: SyntaxMeshHttp<E>,
    authority: String,
    additional: Router,
) -> Router {
    let boundary_state = Boundary {
        authority,
        owner_instance: host.owner_instance.clone(),
    };
    Router::new()
        .route(
            "/healthz",
            get(|| async { Json(json!({"schema_version": 1, "status": "alive"})) }),
        )
        .route("/api/v1/generation", get(generation::<E>))
        .route("/readyz", get(readiness::ready::<E>))
        .route("/api/v1/search", get(search::<E>))
        .route("/api/v1/status", get(status::status::<E>))
        .route("/api/v1/backend-integrity", get(integrity::integrity::<E>))
        .route("/api/v1/files", get(files::files::<E>))
        .route("/api/v1/fact-history", get(history::history::<E>))
        .route(
            "/api/v1/workflow-rejections",
            get(rejections::rejections::<E>),
        )
        .route(
            "/api/v1/resolution-diagnostics",
            get(diagnostics::diagnostics::<E>),
        )
        .route(
            "/api/v1/context",
            post(context::context::<E>).layer(axum::extract::DefaultBodyLimit::max(64 * 1024)),
        )
        .route("/api/v1/nodes/{id}", get(node::<E>))
        .route(
            "/api/v1/nodes/{id}/neighbors",
            get(neighbors::neighbors::<E>),
        )
        .route(
            "/api/v1/nodes/{id}/neighborhood",
            get(neighborhood::neighborhood::<E>),
        )
        .with_state(host)
        .merge(additional)
        .layer(middleware::from_fn_with_state(boundary_state, boundary))
}

async fn boundary(State(state): State<Boundary>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        != Some(state.authority.as_str())
        || headers.contains_key(header::ORIGIN)
        || headers
            .get("sec-fetch-site")
            .is_some_and(|value| value != "none")
    {
        return failure(StatusCode::FORBIDDEN);
    }
    if request.uri().to_string().len() > 8192 {
        return failure(StatusCode::URI_TOO_LONG);
    }
    let mut instances = headers.get_all("x-syntaxmesh-owner-instance").iter();
    if let Some(instance) = instances.next()
        && (instances.next().is_some() || state.owner_instance.as_ref() != Some(instance))
    {
        return failure(StatusCode::PRECONDITION_FAILED);
    }
    let mut response = next.run(request).await;
    if let Some(instance) = state.owner_instance {
        response
            .headers_mut()
            .insert("x-syntaxmesh-owner-instance", instance);
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response
}

fn failure(status: StatusCode) -> Response {
    (
        status,
        Json(json!({"schema_version": 1, "error": status.as_str()})),
    )
        .into_response()
}

fn envelope(generation: GenerationId, data: &Value) -> Response {
    Json(json!({"schema_version": 1, "generation": generation.0.to_hex(), "data": data}))
        .into_response()
}

async fn generation<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
) -> Response {
    match host
        .labeled_query(None, |query| {
            let manifest = query
                .manifest()
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            serde_json::to_value(manifest).map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await
    {
        Ok((generation, data)) => envelope(generation, &data),
        Err(status) => failure(status),
    }
}

async fn search<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    input: Result<Query<SearchInput>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(input)) = input else {
        return failure(StatusCode::BAD_REQUEST);
    };
    let limit = input.limit.unwrap_or(20);
    if limit == 0 || limit > 100 || input.text.len() > 4096 {
        return failure(StatusCode::BAD_REQUEST);
    }
    let selected = match input.generation.as_deref().map(parse_id).transpose() {
        Ok(value) => value.map(GenerationId),
        Err(status) => return failure(status),
    };
    match host
        .labeled_query(selected, move |query| {
            let nodes = if selected.is_some() {
                query.historical_search(&input.text, limit)
            } else {
                query.search(&input.text, limit)
            }
            .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            serde_json::to_value(nodes).map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await
    {
        Ok((generation, data)) => envelope(generation, &data),
        Err(status) => failure(status),
    }
}

fn parse_id(value: &str) -> Result<StableId, StatusCode> {
    if value.len() != 64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let bytes = hex::decode(value).map_err(|_error| StatusCode::BAD_REQUEST)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_error| StatusCode::BAD_REQUEST)?;
    Ok(StableId(bytes))
}

async fn node<E: LanguageExtractor + Send + 'static>(
    State(host): State<SyntaxMeshHttp<E>>,
    Path(id): Path<String>,
    input: Result<Query<NodeInput>, axum::extract::rejection::QueryRejection>,
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
    match host
        .labeled_query(selected, move |query| {
            let node = query
                .historical_node(NodeId(id))
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;
            serde_json::to_value(node).map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await
    {
        Ok((generation, data)) => envelope(generation, &data),
        Err(status) => failure(status),
    }
}
