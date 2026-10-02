use std::error::Error;

use reqwest::blocking::Client;
use serde_json::Value;
use syntaxmesh_core::{GenerationId, Node};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store_turso::TursoGraphStore;

#[test]
fn oversized_diagnostic_route_rejects_current_and_preserves_retained_page()
-> Result<(), Box<dyn Error + Send + Sync>> {
    use syntaxmesh_core::{
        EvidenceClass, GraphDelta, IndexRunId, ModuleResolutionDiagnosticStatus, NodeId, NodeKind,
        Provenance, ProvenanceId, RepositoryId, WorktreeId,
    };
    use syntaxmesh_store::GraphStore as _;

    let fixture = tempfile::tempdir()?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let mut store = TursoGraphStore::open(&database)?;
    let repository = RepositoryId::derive(&[b"oversized-diagnostic"]);
    let worktree = WorktreeId::derive(&[b"oversized-diagnostic"]);
    let old = GenerationId::derive(&[b"small-diagnostic"]);
    let current = GenerationId::derive(&[b"oversized-diagnostic"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"diagnostic-fixture"]),
        producer_namespace: "test.diagnostic".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let occurrence = NodeId::derive(&[b"diagnostic-occurrence"]);
    let diagnostic = Node {
        id: NodeId::derive(&[b"diagnostic"]),
        kind: NodeKind::ModuleResolutionDiagnostic {
            occurrence,
            status: ModuleResolutionDiagnosticStatus::Unresolved,
            candidate_paths: vec![],
        },
        name: "small diagnostic".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    for (generation, base, name) in [
        (old, None, "small diagnostic".to_owned()),
        (current, Some(old), "x".repeat(4 * 1024 * 1024)),
    ] {
        let mut node = diagnostic.clone();
        node.name = name;
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[generation.0.to_hex().as_bytes()]),
            expected_base: base,
            next_generation: generation,
            changed_files: vec![],
            removed_files: vec![],
            upsert_provenance: vec![provenance.clone()],
            upsert_nodes: vec![
                node,
                Node {
                    id: occurrence,
                    kind: NodeKind::Function,
                    name: "fixture occurrence".to_owned(),
                    owner_file: None,
                    source: None,
                    provenance: provenance.id,
                    extension_payload: None,
                },
            ],
            upsert_edges: vec![],
            remove_nodes: vec![],
            remove_edges: vec![],
        })?;
    }
    let host = super::SyntaxMeshHttp::open(&database)?;
    let guard = host.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            host.serve_until(listener, async {
                drop(stop_rx.await);
            })
            .await
        });
        let checked =
            tokio::task::spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
                let client = Client::builder()
                    .no_proxy()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()?;
                let url = format!("http://{address}/api/v1/resolution-diagnostics");
                super::require_status(client.get(&url).send()?, 413)?;
                let retained = super::require_status(
                    client
                        .get(&url)
                        .query(&[("generation", old.0.to_hex())])
                        .send()?,
                    200,
                )?;
                if retained.get("data").and_then(|data| data.get("items"))
                    != Some(&serde_json::json!([diagnostic]))
                    || retained.get("generation").and_then(Value::as_str)
                        != Some(old.0.to_hex().as_str())
                {
                    return Err(
                        "oversized current diagnostic corrupted retained HTTP result".into(),
                    );
                }
                Ok(())
            })
            .await?;
        let _sent = stop_tx.send(());
        server.await??;
        checked
    });
    drop(runtime);
    drop(guard);
    drop(store);
    result
}

pub(super) fn verify(
    client: &Client,
    base: &str,
    engine: &SyntaxMeshEngine<TursoGraphStore, RustExtractor>,
    generations: [GenerationId; 2],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = format!("{base}/api/v1/resolution-diagnostics");
    let rejections = format!("{base}/api/v1/workflow-rejections");
    let status = format!("{base}/api/v1/status");
    let integrity = format!("{base}/api/v1/backend-integrity");
    let workflow_before = engine.workflow_diagnostics()?;
    let expected_integrity = engine.backend_integrity_check()?;
    let integrity_report = super::require_status(client.get(&integrity).send()?, 200)?;
    if integrity_report.get("data")
        != Some(&serde_json::json!({
            "kind":expected_integrity.kind.as_str(),"passed":expected_integrity.passed,"findings":expected_integrity.findings,
        }))
        || engine.workflow_diagnostics()? != workflow_before
    {
        return Err("HTTP backend integrity differs or changed workflow state".into());
    }
    for (key, value) in [("generation", "bad"), ("unknown", "x")] {
        super::require_status(client.get(&integrity).query(&[(key, value)]).send()?, 400)?;
    }
    for (key, value) in [
        ("include_files", "bad"),
        ("generation", "bad"),
        ("unknown", "x"),
    ] {
        super::require_status(client.get(&status).query(&[(key, value)]).send()?, 400)?;
    }
    let rejection_page = super::require_status(client.get(&rejections).send()?, 200)?;
    if rejection_page.get("data") != Some(&serde_json::json!({"items":[],"next_cursor":null})) {
        return Err("empty workflow rejection page differs".into());
    }
    for (key, value) in [
        ("limit", "0"),
        ("limit", "101"),
        ("after_run", "bad"),
        ("generation", "bad"),
    ] {
        super::require_status(client.get(&rejections).query(&[(key, value)]).send()?, 400)?;
    }
    for generation in generations {
        let mut after = None;
        let mut pages = 0usize;
        loop {
            pages = pages.saturating_add(1);
            if pages > 100 {
                return Err("diagnostic continuation did not terminate".into());
            }
            let expected = engine
                .query(generation)
                .module_resolution_diagnostic_page(after, 1)?;
            let mut request = client.get(&path).query(&[
                ("generation", generation.0.to_hex()),
                ("scan_limit", "1".to_owned()),
            ]);
            if let Some(node) = after {
                request = request.query(&[("after_node", node.0.to_hex())]);
            }
            let response = super::require_status(request.send()?, 200)?;
            let data = response.get("data").ok_or("diagnostic data missing")?;
            let items: Vec<Node> = serde_json::from_value(
                data.get("items")
                    .cloned()
                    .ok_or("diagnostic items missing")?,
            )?;
            let next = data.get("next_after").and_then(Value::as_str);
            let expected_next = expected.next_after.map(|node| node.0.to_hex());
            if items != expected.items
                || data.get("scanned_nodes").and_then(Value::as_u64)
                    != Some(u64::try_from(expected.scanned_nodes)?)
                || data.get("has_more").and_then(Value::as_bool)
                    != Some(expected.next_after.is_some())
                || next != expected_next.as_deref()
                || response.get("generation").and_then(Value::as_str)
                    != Some(generation.0.to_hex().as_str())
            {
                return Err("diagnostic HTTP page differs from retained Engine query".into());
            }
            after = expected.next_after;
            if after.is_none() {
                break;
            }
        }
        if pages < 2 {
            return Err("fixture did not exercise empty-match continuation".into());
        }
    }
    for (key, value) in [
        ("scan_limit", "0"),
        ("scan_limit", "101"),
        ("scan_limit", "bad"),
        ("generation", "bad"),
        ("after_node", "bad"),
        ("unknown", "x"),
    ] {
        super::require_status(client.get(&path).query(&[(key, value)]).send()?, 400)?;
    }
    super::require_status(
        client
            .get(&path)
            .query(&[("after_node", "a".repeat(64))])
            .send()?,
        400,
    )?;
    super::require_status(
        client
            .get(&path)
            .query(&[("generation", "f".repeat(64))])
            .send()?,
        404,
    )?;
    Ok(())
}
