use std::error::Error;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use syntaxmesh_api_model::HistoricalNeighborCursor;
use syntaxmesh_core::{
    Edge, EdgeDirection, GenerationId, IndexRunId, Node, NodeId, RepositoryId, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_scanner::scan;
use syntaxmesh_store_turso::TursoGraphStore;

use super::{SyntaxMeshHttp, require_status};

#[test]
fn tcp_neighbor_pages_match_indexed_engine_queries_and_bind_continuations()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("source");
    std::fs::create_dir(&source)?;
    let file = source.join("lib.rs");
    std::fs::write(
        &file,
        "pub fn caller(){first();second();third();}\npub fn other(){first();}\npub fn first(){}\npub fn second(){}\npub fn third(){}\n",
    )?;
    let database = fixture.path().join("graph.db");
    let auxiliary = source.join("auxiliary.rs");
    std::fs::write(&auxiliary, "pub fn inventory_auxiliary() {}\n")?;
    TursoGraphStore::migrate(&database)?;
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"http-neighbors-repository"]),
        WorktreeId::derive(&[b"http-neighbors-worktree"]),
    );
    let old = GenerationId::derive(&[b"http-neighbors-old"]);
    engine.index(
        &scan(&source, &["rs"])?.files,
        IndexRunId::derive(&[b"http-neighbors-old-run"]),
        old,
    )?;
    let find = |name| -> Result<NodeId, Box<dyn Error + Send + Sync>> {
        Ok(engine
            .query(old)
            .search(name, 20)?
            .into_iter()
            .find(|node| node.name == name)
            .ok_or("fixture symbol missing")?
            .id)
    };
    let caller = find("caller")?;
    let first = find("first")?;
    let third = find("third")?;
    std::fs::write(
        &file,
        "pub fn caller(){first();second();}\npub fn first(){}\npub fn second(){}\n",
    )?;
    let current = GenerationId::derive(&[b"http-neighbors-current"]);
    std::fs::remove_file(&auxiliary)?;
    engine.index(
        &scan(&source, &["rs"])?.files,
        IndexRunId::derive(&[b"http-neighbors-current-run"]),
        current,
    )?;
    let host = SyntaxMeshHttp::open(&database)?;
    let guard = host.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            host.serve_until(listener, async { drop(shutdown_rx.await); }).await
        });
        let client_result = tokio::task::spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
            let client = Client::builder().no_proxy().timeout(std::time::Duration::from_secs(10)).build()?;
            let base = format!("http://{address}");
            super::require_status(client.post(format!("{base}/api/v1/context"))
                .json(&serde_json::json!({"query":"caller","token_budget":8192})).send()?, 503)?;
            let path = |node: NodeId| format!("{base}/api/v1/nodes/{}/neighbors", node.0.to_hex());
            let mut retained_cursor = None;
            for generation in [old, current] {
                for endpoint in [caller, first] {
                    for direction in [EdgeDirection::Outgoing, EdgeDirection::Incoming] {
                        let direction_text = match direction { EdgeDirection::Outgoing => "outgoing", EdgeDirection::Incoming => "incoming" };
                        let mut after = None;
                        let mut token: Option<String> = None;
                        let mut pages = 0usize;
                        loop {
                            pages = pages.saturating_add(1);
                            if pages > 16 { return Err("HTTP continuation did not terminate".into()); }
                            let expected = engine.query(generation).historical_neighbors(endpoint, direction, 1, after)?;
                            let mut request = client.get(path(endpoint)).query(&[
                                ("generation", generation.0.to_hex()), ("direction", direction_text.to_owned()), ("limit", "1".to_owned()),
                            ]);
                            if let Some(cursor) = &token { request = request.query(&[("cursor", cursor)]); }
                            let response = require_status(request.send()?, 200)?;
                            let data = response.get("data").ok_or("missing HTTP page data")?;
                            let items = data.get("items").and_then(Value::as_array).ok_or("missing page items")?;
                            let actual = items.iter().map(|item| -> Result<(Edge, Node), serde_json::Error> {
                                Ok((serde_json::from_value(item.get("edge").cloned().unwrap_or(Value::Null))?, serde_json::from_value(item.get("neighbor").cloned().unwrap_or(Value::Null))?))
                            }).collect::<Result<Vec<_>, _>>()?;
                            let expected_items = expected.items.into_iter().map(|item| (item.edge, item.neighbor)).collect::<Vec<_>>();
                            if actual != expected_items || data.get("has_more").and_then(Value::as_bool) != Some(expected.has_more)
                                || data.get("endpoint").and_then(Value::as_str) != Some(endpoint.0.to_hex().as_str())
                                || data.get("direction").and_then(Value::as_str) != Some(direction_text)
                                || response.get("generation").and_then(Value::as_str) != Some(generation.0.to_hex().as_str()) {
                                return Err("HTTP neighbor page differs from independent Engine query".into());
                            }
                            token = data.get("next_cursor").and_then(Value::as_str).map(str::to_owned);
                            let decoded = token.as_ref().map(|token| -> Result<HistoricalNeighborCursor, Box<dyn Error + Send + Sync>> {
                                let value: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(token)?)?;
                                if value.get("version") != Some(&json!(1)) { return Err("unexpected cursor version".into()); }
                                Ok(serde_json::from_value(value.get("cursor").cloned().ok_or("missing cursor coordinates")?)?)
                            }).transpose()?;
                            if decoded != expected.next_cursor || expected.has_more != token.is_some() {
                                return Err("HTTP cursor differs from query continuation".into());
                            }
                            if generation == old && endpoint == caller && direction == EdgeDirection::Outgoing && retained_cursor.is_none() {
                                retained_cursor = token.clone();
                            }
                            after = expected.next_cursor;
                            if !expected.has_more { break; }
                        }
                    }
                }
            }
            let token = retained_cursor.ok_or("fixture did not exercise multi-page continuation")?;
            let after = engine.query(old).historical_neighbors(caller, EdgeDirection::Outgoing, 1, None)?.next_cursor;
            let expected = engine.query(old).historical_neighbors(caller, EdgeDirection::Outgoing, 2, after)?;
            let resumed = require_status(client.get(path(caller)).query(&[("generation", old.0.to_hex()), ("cursor", token.clone()), ("limit", "2".to_owned())]).send()?, 200)?;
            let expected_items = expected.items.into_iter().map(|item| json!({"edge": item.edge, "neighbor": item.neighbor})).collect::<Vec<_>>();
            if resumed.get("data").and_then(|data| data.get("items")) != Some(&json!(expected_items)) {
                return Err("changing continuation page size changed query semantics".into());
            }
            for (endpoint, generation, direction) in [(first, old, "outgoing"), (caller, current, "outgoing"), (caller, old, "incoming")] {
                require_status(client.get(path(endpoint)).query(&[("generation", generation.0.to_hex()), ("direction", direction.to_owned()), ("cursor", token.clone())]).send()?, 400)?;
            }
            for cursor in ["invalid!".to_owned(), "x".repeat(2049), URL_SAFE_NO_PAD.encode(b"not JSON"), URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({"version": 2, "cursor": engine.query(old).historical_neighbors(caller, EdgeDirection::Outgoing, 1, None)?.next_cursor}))?)] {
                require_status(client.get(path(caller)).query(&[("cursor", cursor)]).send()?, 400)?;
            }
            for (key, value) in [("limit", "0"), ("limit", "101"), ("direction", "both"), ("generation", "bad"), ("unknown", "x")] {
                require_status(client.get(path(caller)).query(&[(key, value)]).send()?, 400)?;
            }
            // The third symbol was removed; its old incoming page remains available.
            require_status(client.get(path(third)).send()?, 404)?;
            require_status(client.get(path(third)).query(&[("generation", old.0.to_hex())]).send()?, 200)?;
            require_status(client.get(path(NodeId::derive(&[b"missing-neighbor-endpoint"]))).send()?, 404)?;
            let default_page = require_status(client.get(path(caller)).send()?, 200)?;
            if default_page.get("generation").and_then(Value::as_str) != Some(current.0.to_hex().as_str()) { return Err("default neighbors did not use startup generation".into()); }
            let newer = GenerationId::derive(&[b"http-neighbors-newer"]);
            super::diagnostics::verify(&client, &base, &engine, [old, current])?;
            super::files::verify(&client, &base, &engine, [old, current])?;
            super::history::verify(&client, &base, &engine, [old, current])?;
            super::rejections::verify(&client, &base, &mut engine, current, caller)?;
            super::neighborhood::verify(&client, &base, &engine, caller, [old, current], third)?;
            engine.index(&scan(&source, &["rs"])?.files, IndexRunId::derive(&[b"http-neighbors-newer-run"]), newer)?;
            require_status(client.get(path(caller)).send()?, 409)?;
            require_status(client.get(format!("{base}/api/v1/nodes/{}/neighborhood", caller.0.to_hex())).send()?, 409)?;
            Ok(())
        }).await?;
        shutdown_tx.send(()).map_err(|_value| "HTTP shutdown receiver vanished")?;
        server.await??;
        client_result
    });
    drop(runtime);
    drop(guard);
    result
}
