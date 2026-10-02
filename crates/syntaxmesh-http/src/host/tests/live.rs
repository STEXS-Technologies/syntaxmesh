use super::*;
use std::sync::{Arc, Mutex};

#[test]
fn shared_engine_publication_refreshes_live_tcp_reads() -> Result<(), Box<dyn Error + Send + Sync>>
{
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("source");
    std::fs::create_dir(&source)?;
    let file = source.join("lib.rs");
    std::fs::write(&file, "pub fn live_before() {}\n")?;
    let documentation = source.join("architecture.md");
    std::fs::write(
        &documentation,
        "# Before policy\n\nRetain historical decisions.\n",
    )?;
    for (name, content) in [
        ("module.ts", "export function typescript_before() {}\n"),
        ("module.js", "export function javascript_before() {}\n"),
        ("module.py", "def python_before():\n    pass\n"),
        ("module.sh", "bash_before() { :; }\n"),
        ("notes.txt", "Before text policy\n"),
    ] {
        std::fs::write(source.join(name), content)?;
    }
    let database = fixture.path().join("graph.db");
    let _ownership = syntaxmesh_ownership_host::WriterLease::acquire(&database)?;
    TursoGraphStore::migrate(&database)?;
    let repository = RepositoryId::derive(&[b"live-http-repository"]);
    let worktree = WorktreeId::derive(&[b"live-http-worktree"]);
    let old = GenerationId::derive(&[b"live-http-old"]);
    let current = GenerationId::derive(&[b"live-http-current"]);
    let extractors = syntaxmesh_source_host::supported_source_extractors()?;
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        extractors,
        repository,
        worktree,
    )
    .with_statechronicle_verification();
    engine.recover_pending_workflows()?;
    engine.index(
        &scan(&source, &["rs", "md", "txt", "ts", "js", "py", "sh"])?.files,
        IndexRunId::derive(&[b"live-http-old-run"]),
        old,
    )?;
    let shared = Arc::new(Mutex::new(engine));
    let host = SyntaxMeshHttp::from_shared_engine(Arc::clone(&shared), repository, worktree)?;
    if SyntaxMeshHttp::from_shared_engine(
        Arc::clone(&shared),
        RepositoryId::derive(&[b"foreign-scope"]),
        worktree,
    )
    .is_ok()
    {
        return Err("shared host accepted a foreign scope".into());
    }
    let publisher = Arc::clone(&shared);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
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
                let initial = require_status(
                    client
                        .get(format!("{base}/api/v1/search?text=live_before"))
                        .send()?,
                    200,
                )?;
                if initial.get("generation").and_then(Value::as_str)
                    != Some(old.0.to_hex().as_str())
                {
                    return Err("live HTTP initial generation differs".into());
                }
                std::fs::write(&file, "pub fn live_after() {}\n")?;
                std::fs::write(
                    &documentation,
                    "# After policy\n\nRetain updated decisions.\n",
                )?;
                for (name, content) in [
                    ("module.ts", "export function typescript_after() {}\n"),
                    ("module.js", "export function javascript_after() {}\n"),
                    ("module.py", "def python_after():\n    pass\n"),
                    ("module.sh", "bash_after() { :; }\n"),
                    ("notes.txt", "After text policy\n"),
                ] {
                    std::fs::write(source.join(name), content)?;
                }
                {
                    let mut writer = publisher.lock().map_err(|error| error.to_string())?;
                    writer.index(
                        &scan(&source, &["rs", "md", "txt", "ts", "js", "py", "sh"])?.files,
                        IndexRunId::derive(&[b"live-http-current-run"]),
                        current,
                    )?;
                    if writer.verify_statechronicle_history()?.is_none() {
                        return Err(
                            "live host publication did not retain verification history".into()
                        );
                    }
                }
                for (selected, text) in [
                    (None, "live_after"),
                    (None, "live_before"),
                    (Some(old), "live_before"),
                    (Some(old), "live_after"),
                    (None, "After policy"),
                    (None, "Before policy"),
                    (Some(old), "Before policy"),
                    (Some(old), "After policy"),
                    (None, "typescript_after"),
                    (Some(old), "typescript_before"),
                    (None, "javascript_after"),
                    (Some(old), "javascript_before"),
                    (None, "python_after"),
                    (Some(old), "python_before"),
                    (None, "bash_after"),
                    (Some(old), "bash_before"),
                ] {
                    let mut request = client
                        .get(format!("{base}/api/v1/search"))
                        .query(&[("text", text)]);
                    if let Some(generation) = selected {
                        request = request.query(&[("generation", generation.0.to_hex())]);
                    }
                    let response = require_status(request.send()?, 200)?;
                    let expected_generation = selected.unwrap_or(current);
                    let expected = {
                        let reader = publisher.lock().map_err(|error| error.to_string())?;
                        let nodes = reader
                            .query(expected_generation)
                            .historical_search(text, 20)?;
                        let should_exist = (expected_generation == old
                            && (text.ends_with("_before") || text.starts_with("Before")))
                            || (expected_generation == current
                                && (text.ends_with("_after") || text.starts_with("After")));
                        if nodes.is_empty() == should_exist {
                            return Err(format!(
                                "missing fixture extraction or unretracted symbol: {text}"
                            )
                            .into());
                        }
                        serde_json::to_value(nodes)?
                    };
                    if response.get("generation").and_then(Value::as_str)
                        != Some(expected_generation.0.to_hex().as_str())
                        || response.get("data") != Some(&expected)
                    {
                        return Err(
                            "live HTTP response mixed generations or missed publication".into()
                        );
                    }
                }
                let ready = require_status(client.get(format!("{base}/readyz")).send()?, 200)?;
                if ready.get("generation").and_then(Value::as_str)
                    != Some(current.0.to_hex().as_str())
                {
                    return Err("live readiness retained startup generation".into());
                }
                Ok(())
            })
            .await?;
        let _shutdown_result = shutdown_tx.send(());
        server.await??;
        client_result
    });
    drop(runtime);
    drop(shared);
    result
}
