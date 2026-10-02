use std::error::Error;

use reqwest::blocking::{Client, Response};
use serde_json::Value;
use syntaxmesh_core::{GenerationId, IndexRunId, NodeId, RepositoryId, WorktreeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_docs::DocumentationExtractor;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::CompositeExtractor;
use syntaxmesh_scanner::scan;
use syntaxmesh_store_turso::TursoGraphStore;

use super::SyntaxMeshHttp;

mod context;
mod diagnostics;
mod files;
mod history;
mod live;
mod neighborhood;
mod neighbors;
mod rejections;
mod watch_live;

fn require_status(
    response: Response,
    expected: u16,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    if response.status().as_u16() != expected {
        return Err(format!("expected HTTP {expected}, got {}", response.status()).into());
    }
    Ok(response.json()?)
}

#[test]
fn tcp_queries_match_engine_history_and_reject_stale_or_untrusted_requests()
-> Result<(), Box<dyn Error + Send + Sync>> {
    for tokenizer in ["cl100k_base", "o200k_base"] {
        verify_tcp_host(tokenizer)?;
    }
    Ok(())
}

fn verify_tcp_host(tokenizer: &'static str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("source");
    std::fs::create_dir(&source)?;
    let source_file = source.join("lib.rs");
    std::fs::write(&source_file, "pub fn old_symbol() {}\n")?;
    std::fs::write(
        source.join("architecture.md"),
        "# History policy\n\nRetain historical evidence because architectural decisions must remain auditable.\n",
    )?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let mut extractors = CompositeExtractor::new();
    extractors.register(RustExtractor, ["rs"])?;
    extractors.register(DocumentationExtractor, ["md"])?;
    let mut index_engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        extractors,
        RepositoryId::derive(&[b"http-repository"]),
        WorktreeId::derive(&[b"http-worktree"]),
    );
    let old = GenerationId::derive(&[b"http-old"]);
    index_engine.index(
        &scan(&source, &["rs", "md"])?.files,
        IndexRunId::derive(&[b"http-old-run"]),
        old,
    )?;
    let old_node = index_engine
        .query(old)
        .search("old_symbol", 20)?
        .into_iter()
        .next()
        .ok_or("missing old fixture node")?;
    std::fs::write(&source_file, "pub fn current_symbol() {}\n")?;
    std::fs::write(
        source.join("architecture.md"),
        "# History policy\n\nVerify historical evidence because current source bytes may describe a different decision.\n",
    )?;
    let current = GenerationId::derive(&[b"http-current"]);
    index_engine.index(
        &scan(&source, &["rs", "md"])?.files,
        IndexRunId::derive(&[b"http-current-run"]),
        current,
    )?;
    let expected = serde_json::to_value(index_engine.query(current).search("current_symbol", 20)?)?;
    // Extraction stays on the creating thread; the query fixture needs only
    // the persisted graph and a Send-compatible host extractor.
    drop(index_engine);
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"http-repository"]),
        WorktreeId::derive(&[b"http-worktree"]),
    );
    let host = SyntaxMeshHttp::open(&database)?.with_context(&source, tokenizer)?;
    let guard = host.clone();
    let admission = host.admission.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker_host = host.clone();
        let worker = tokio::spawn(async move {
            worker_host
                .query(None, move |_query| {
                    entered_tx
                        .send(())
                        .map_err(|_value| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
                    release_rx
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .map_err(|_error| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
                    Ok(())
                })
                .await
        });
        entered_rx.await?;
        worker.abort();
        let cancelled = worker.await;
        if !cancelled.is_err_and(|error| error.is_cancelled())
            || admission.available_permits() != 15
        {
            release_tx.send(())?;
            return Err("cancelled HTTP query released a running blocking worker's permit".into());
        }
        release_tx.send(())?;
        // Acquiring the complete pool proves the abandoned blocking job finished
        // and released its own permit, rather than leaking admission capacity.
        let restored = admission.clone().acquire_many_owned(16).await?;
        drop(restored);
        let non_loopback = tokio::net::TcpListener::bind("0.0.0.0:0").await?;
        if host.serve_until(non_loopback, async {}).await.is_ok() {
            return Err("HTTP host accepted a non-loopback listener".into());
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let shutdown_host = host.clone();
        let server = tokio::spawn(async move {
            host.serve_until(listener, async {
                drop(shutdown_rx.await);
            })
            .await
        });
        let client_result =
            tokio::task::spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
                let client = Client::builder()
                    .no_proxy()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()?;
                let base = format!("http://{address}");
                let diagnostics = engine.workflow_diagnostics()?;
                let readiness = require_status(client.get(format!("{base}/readyz")).send()?, 200)?;
                if readiness
                    != serde_json::json!({
                        "schema_version":1,"status":"ready","generation":current.0.to_hex(),
                        "recovery":{
                            "prepared_operations":diagnostics.prepared_operations,
                            "completed_operations":diagnostics.completed_operations,
                            "rejected_operations":diagnostics.rejected_operations,
                        }
                    })
                {
                    return Err("HTTP readiness differs from Engine diagnostics".into());
                }
                context::verify(
                    &client,
                    &base,
                    &engine,
                    &source,
                    [old, current],
                    old_node.id,
                    tokenizer,
                )?;
                let query = require_status(
                    client
                        .get(format!("{base}/api/v1/search?text=current_symbol"))
                        .send()?,
                    200,
                )?;
                if query.get("generation").and_then(Value::as_str)
                    != Some(current.0.to_hex().as_str())
                    || query.get("data") != Some(&expected)
                {
                    return Err("HTTP search differs from embedded Engine query".into());
                }
                let retained = require_status(
                    client
                        .get(format!(
                            "{base}/api/v1/nodes/{}?generation={}",
                            old_node.id.0.to_hex(),
                            old.0.to_hex()
                        ))
                        .send()?,
                    200,
                )?;
                for selected in [old, current] {
                    for text in ["old_symbol", "current_symbol"] {
                        let historical_search = require_status(
                            client
                                .get(format!("{base}/api/v1/search"))
                                .query(&[
                                    ("text", text.to_owned()),
                                    ("generation", selected.0.to_hex()),
                                ])
                                .send()?,
                            200,
                        )?;
                        let expected_search = serde_json::to_value(
                            engine.query(selected).historical_search(text, 20)?,
                        )?;
                        if historical_search.get("data") != Some(&expected_search)
                            || historical_search.get("generation").and_then(Value::as_str)
                                != Some(selected.0.to_hex().as_str())
                        {
                            return Err(
                                "HTTP generation-selected search differs from Engine history"
                                    .into(),
                            );
                        }
                    }
                }
                if retained.get("data") != Some(&serde_json::to_value(&old_node)?)
                    || retained.get("generation").and_then(Value::as_str)
                        != Some(old.0.to_hex().as_str())
                {
                    return Err("HTTP retained node differs from historical Engine query".into());
                }
                require_status(
                    client
                        .get(format!("{base}/api/v1/nodes/{}", old_node.id.0.to_hex()))
                        .send()?,
                    404,
                )?;
                let missing = NodeId::derive(&[b"missing-http-node"]).0.to_hex();
                for (path, status) in [
                    ("/api/v1/generation".to_owned(), 200),
                    ("/api/v1/nodes/not-an-id".to_owned(), 400),
                    (format!("/api/v1/nodes/{missing}"), 404),
                    (format!("/api/v1/nodes/{missing}?generation=bad"), 400),
                    (
                        format!(
                            "/api/v1/nodes/{missing}?generation={}",
                            GenerationId::derive(&[b"missing-generation"]).0.to_hex()
                        ),
                        404,
                    ),
                    ("/api/v1/search?text=x&limit=0".to_owned(), 400),
                    ("/api/v1/search?text=x&generation=bad".to_owned(), 400),
                    (
                        format!(
                            "/api/v1/search?text=x&generation={}",
                            GenerationId::derive(&[b"missing-search-generation"])
                                .0
                                .to_hex()
                        ),
                        404,
                    ),
                    ("/api/v1/search?text=x&limit=101".to_owned(), 400),
                    ("/api/v1/search?text=x&extra=y".to_owned(), 400),
                    (format!("/api/v1/search?text={}", "a".repeat(4097)), 400),
                    (format!("/api/v1/search?text={}", "a".repeat(8193)), 414),
                ] {
                    require_status(client.get(format!("{base}{path}")).send()?, status)?;
                }
                for (header, value) in [
                    ("host", "attacker.invalid"),
                    ("origin", "http://attacker.invalid"),
                    ("sec-fetch-site", "cross-site"),
                ] {
                    require_status(
                        client
                            .get(format!("{base}/api/v1/search?text=x"))
                            .header(header, value)
                            .send()?,
                        403,
                    )?;
                }
                require_status(
                    client
                        .get(format!("{base}/api/v1/search?text=x"))
                        .header("x-syntaxmesh-owner-instance", "a".repeat(64))
                        .send()?,
                    412,
                )?;
                let permit = admission.try_acquire_many_owned(16)?;
                require_status(client.get(format!("{base}/readyz")).send()?, 503)?;
                require_status(
                    client
                        .post(format!("{base}/api/v1/context"))
                        .json(&serde_json::json!({"query":"current_symbol","token_budget":8192}))
                        .send()?,
                    503,
                )?;
                require_status(client.get(format!("{base}/api/v1/generation")).send()?, 503)?;
                require_status(client.get(format!("{base}/healthz")).send()?, 200)?;
                drop(permit);
                require_status(client.get(format!("{base}/api/v1/generation")).send()?, 200)?;
                require_status(client.get(format!("{base}/readyz")).send()?, 200)?;
                std::fs::write(&source_file, "pub fn newer_symbol() {}\n")?;
                engine.index(
                    &scan(&source, &["rs"])?.files,
                    IndexRunId::derive(&[b"http-new-run"]),
                    GenerationId::derive(&[b"http-new"]),
                )?;
                require_status(
                    client
                        .get(format!("{base}/api/v1/search?text=newer_symbol"))
                        .send()?,
                    409,
                )?;
                require_status(client.get(format!("{base}/healthz")).send()?, 200)?;
                require_status(
                    client
                        .post(format!("{base}/api/v1/context"))
                        .json(&serde_json::json!({"query":"current_symbol","token_budget":8192}))
                        .send()?,
                    409,
                )?;
                require_status(client.get(format!("{base}/readyz")).send()?, 503)?;
                Ok(())
            })
            .await?;
        shutdown_tx
            .send(())
            .map_err(|_value| "shutdown receiver disappeared")?;
        server.await??;
        if shutdown_host.readiness().await.is_ok() {
            return Err("shutdown host remained ready".into());
        }
        if shutdown_host.query(None, |_query| Ok(())).await
            != Err(axum::http::StatusCode::SERVICE_UNAVAILABLE)
        {
            return Err("shutdown host clone accepted new query work".into());
        }
        client_result
    });
    drop(runtime);
    drop(guard);
    result
}

#[test]
fn absent_database_is_not_created() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let database = fixture.path().join("absent.db");
    if SyntaxMeshHttp::open(&database).is_ok() || database.exists() {
        return Err("read-only host created an absent database".into());
    }
    Ok(())
}
