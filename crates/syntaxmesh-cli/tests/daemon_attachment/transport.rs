use std::error::Error;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use axum::http::{HeaderValue, StatusCode};
use axum::response::IntoResponse;
use syntaxmesh_ownership_host::{OwnerEndpoint, WriterLease};

#[derive(Clone, Copy)]
pub(super) enum Guard {
    Matching,
    Missing,
    Wrong,
    Duplicate,
}

fn exercise_reply(
    status: StatusCode,
    guard: Guard,
    body: String,
    expected_error: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    exercise_command(
        status,
        guard,
        body,
        expected_error,
        "search-turso",
        &["transport fixture"],
    )
}

pub(super) fn exercise_command(
    status: StatusCode,
    guard: Guard,
    body: String,
    expected_error: Option<&str>,
    command: &str,
    arguments: &[&str],
) -> Result<(), Box<dyn Error>> {
    exercise_pages(
        status,
        guard,
        vec![body],
        expected_error,
        command,
        arguments,
    )
}

pub(super) fn exercise_pages(
    status: StatusCode,
    guard: Guard,
    bodies: Vec<String>,
    expected_error: Option<&str>,
    command: &str,
    arguments: &[&str],
) -> Result<(), Box<dyn Error>> {
    exercise_guarded_pages(
        status,
        bodies.into_iter().map(|body| (guard, body)).collect(),
        expected_error,
        command,
        arguments,
    )
}

pub(super) fn exercise_guarded_pages(
    status: StatusCode,
    pages: Vec<(Guard, String)>,
    expected_error: Option<&str>,
    command: &str,
    arguments: &[&str],
) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let database = fixture.path().join("not-a-database.db");
    // A successful request or explicit transport error proves that this path
    // did not try to open the invalid database as an embedded fallback.
    std::fs::write(&database, b"preserve invalid database bytes")?;
    let lease = WriterLease::acquire(&database)?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = OwnerEndpoint::new(&database, listener.local_addr()?)?;
    endpoint.publish(&lease)?;
    let identity = endpoint.instance().to_owned();
    let observed = Arc::new(AtomicBool::new(true));
    let captured = Arc::clone(&observed);
    let expected_requests = pages.len();
    let body = Arc::new(pages);
    let request_count = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&request_count);
    let router = axum::Router::new().fallback(move |request: axum::extract::Request| {
        let identity = identity.clone();
        let captured = Arc::clone(&captured);
        let body = Arc::clone(&body);
        let counted = Arc::clone(&counted);
        async move {
            let sequence = counted.fetch_add(1, Ordering::AcqRel);
            let correct_pin = sequence == 0
                || request.uri().query().is_some_and(|query| {
                    query.contains(&format!("generation={}", "a".repeat(64)))
                        && (request.uri().path() == "/api/v1/files" && sequence == 1
                            || query.contains(
                                if request.uri().path() == "/api/v1/resolution-diagnostics" {
                                    "after_node="
                                } else if request.uri().path() == "/api/v1/files" {
                                    "after_file="
                                } else {
                                    "cursor="
                                },
                            ))
                });
            let valid_request = request
                .headers()
                .get("x-syntaxmesh-owner-instance")
                .and_then(|value| value.to_str().ok())
                == Some(identity.as_str())
                && (request.uri().path().starts_with("/api/v1/nodes/")
                    || (request.uri().path() == "/api/v1/backend-integrity"
                        && request.uri().query().is_none())
                    || (request.uri().path() == "/api/v1/status"
                        && request
                            .uri()
                            .query()
                            .is_some_and(|query| query.contains("include_files=")))
                    || request
                        .uri()
                        .query()
                        .is_some_and(|query| query.contains("limit=100")))
                && correct_pin;
            if !valid_request {
                captured.store(false, Ordering::Release);
            }
            let (guard, reply) = body
                .get(sequence)
                .cloned()
                .unwrap_or_else(|| (Guard::Missing, "unexpected extra request".to_owned()));
            let mut response = (status, reply).into_response();
            let headers = response.headers_mut();
            if !matches!(guard, Guard::Missing) {
                let value = if matches!(guard, Guard::Wrong) {
                    HeaderValue::from_static("wrong-owner")
                } else {
                    match HeaderValue::from_str(&identity) {
                        Ok(value) => value,
                        Err(_error) => {
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        }
                    }
                };
                headers.append("x-syntaxmesh-owner-instance", value.clone());
                if matches!(guard, Guard::Duplicate) {
                    headers.append("x-syntaxmesh-owner-instance", value);
                }
            }
            headers.insert(
                "location",
                HeaderValue::from_static("http://127.0.0.1:1/redirect-target"),
            );
            response
        }
    });
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime
            .block_on(async {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                axum::serve(listener, router)
                    .with_graceful_shutdown(async {
                        drop(stop_rx.await);
                    })
                    .await
            })
            .map_err(|error| error.to_string())
    });
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg(command)
        .arg(&database)
        .args(arguments)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .env("http_proxy", "http://127.0.0.1:1")
        .env("all_proxy", "http://127.0.0.1:1")
        .env("no_proxy", "")
        .output();
    let _stopped = stop_tx.send(());
    worker
        .join()
        .map_err(|_panic| "fixture worker panicked")??;
    let output = output?;
    if request_count.load(Ordering::Acquire) != expected_requests
        || !observed.load(Ordering::Acquire)
        || !output.stdout.is_empty()
        || std::fs::read(&database)? != b"preserve invalid database bytes"
    {
        return Err(
            "request guard, proxy bypass, output, or unchanged-store invariant failed".into(),
        );
    }
    match expected_error {
        Some(expected)
            if output.status.success()
                || !String::from_utf8_lossy(&output.stderr).contains(expected) =>
        {
            Err(format!(
                "expected {expected}, got {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into())
        }
        None if !output.status.success() => {
            Err(String::from_utf8_lossy(&output.stderr).into_owned().into())
        }
        _ => Ok(()),
    }
}

fn empty_response(version: u32) -> String {
    serde_json::json!({"schema_version":version,"generation":"a".repeat(64),"data":[]}).to_string()
}

fn node_response(node_id: u8, generation: &str) -> String {
    use syntaxmesh_core::{Node, NodeId, NodeKind, ProvenanceId, StableId};
    let node = Node {
        id: NodeId(StableId([node_id; 32])),
        kind: NodeKind::Function,
        name: "substituted_node".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId(StableId([1; 32])),
        extension_payload: None,
    };
    serde_json::json!({"schema_version":1,"generation":generation,"data":node}).to_string()
}

#[test]
fn node_attachment_rejects_substituted_nodes_and_historical_generation()
-> Result<(), Box<dyn Error>> {
    let requested_node = "b".repeat(64);
    let requested_generation = "a".repeat(64);
    let foreign_generation = "c".repeat(64);
    exercise_command(
        StatusCode::OK,
        Guard::Matching,
        node_response(0xcc, &requested_generation),
        Some("node or generation mismatch"),
        "node-turso",
        &[&requested_node],
    )?;
    exercise_command(
        StatusCode::OK,
        Guard::Matching,
        node_response(0xbb, &foreign_generation),
        Some("node or generation mismatch"),
        "node-at-turso",
        &[&requested_generation, &requested_node],
    )?;
    exercise_command(
        StatusCode::OK,
        Guard::Matching,
        node_response(0xcc, &requested_generation),
        Some("node or generation mismatch"),
        "node-at-turso",
        &[&requested_generation, &requested_node],
    )?;
    Ok(())
}

#[test]
fn guarded_transport_bypasses_proxy_and_rejects_missing_wrong_duplicate_identity()
-> Result<(), Box<dyn Error>> {
    exercise_reply(StatusCode::OK, Guard::Matching, empty_response(1), None)?;
    for guard in [Guard::Missing, Guard::Wrong, Guard::Duplicate] {
        exercise_reply(
            StatusCode::OK,
            guard,
            empty_response(1),
            Some("owner instance response mismatch"),
        )?;
    }
    Ok(())
}

#[test]
fn transport_rejects_redirect_oversize_bad_schema_and_malformed_body() -> Result<(), Box<dyn Error>>
{
    let mut exact_limit = empty_response(1);
    exact_limit.push_str(&" ".repeat((4_usize * 1024 * 1024).saturating_sub(exact_limit.len())));
    exercise_reply(StatusCode::OK, Guard::Matching, exact_limit, None)?;
    for (status, body, expected) in [
        (StatusCode::FOUND, empty_response(1), "HTTP 302"),
        (
            StatusCode::OK,
            " ".repeat(4 * 1024 * 1024 + 1),
            "response exceeds 4 MiB",
        ),
        (StatusCode::OK, empty_response(2), "invalid response schema"),
        (StatusCode::OK, "not json".to_owned(), "encoding error"),
    ] {
        exercise_reply(status, Guard::Matching, body, Some(expected))?;
    }
    Ok(())
}
