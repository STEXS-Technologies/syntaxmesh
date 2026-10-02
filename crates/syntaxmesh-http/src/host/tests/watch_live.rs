use super::*;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use syntaxmesh_watch_host::{WatchLoopOptions, run_watch};

#[test]
fn native_and_polling_watch_publish_to_live_http_without_reopening()
-> Result<(), Box<dyn Error + Send + Sync>> {
    for poll_only in [false, true] {
        verify_watch(poll_only)?;
    }
    Ok(())
}

fn verify_watch(poll_only: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("source");
    std::fs::create_dir(&source)?;
    let source = source.canonicalize()?;
    let file = source.join("lib.rs");
    std::fs::write(&file, "pub fn watch_before() {}\n")?;
    let database = fixture.path().join("graph.db");
    let _ownership = syntaxmesh_ownership_host::WriterLease::acquire(&database)?;
    TursoGraphStore::migrate(&database)?;
    let (repository, worktree) = syntaxmesh_source_host::repository_scope(&source);
    let configuration = syntaxmesh_source_host::ProjectConfig::load(&source)?;
    let setup = syntaxmesh_source_host::configured_source_engine(
        TursoGraphStore::open(&database)?,
        &source,
        &configuration,
        true,
    )?;
    let extractor_fingerprint = setup.extractor_fingerprint;
    let resolver_fingerprint = setup.resolver_fingerprint;
    let mut engine = setup.engine;
    engine.recover_pending_workflows()?;
    let initial_files = scan(&source, syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS)?.files;
    let mut initial_fingerprint = syntaxmesh_engine::source_inventory_fingerprint(&initial_files)?;
    initial_fingerprint.extend_from_slice(&resolver_fingerprint);
    let (old, initial_needed) = engine.prepare_source_index(
        &initial_files,
        &extractor_fingerprint,
        &initial_fingerprint,
    )?;
    if !initial_needed {
        return Err("initial watch fixture did not need publication".into());
    }
    engine.index(
        &initial_files,
        IndexRunId::derive(&[b"watch-http-old-run"]),
        old,
    )?;
    let shared = Arc::new(Mutex::new(engine));
    let host = SyntaxMeshHttp::from_shared_engine(Arc::clone(&shared), repository, worktree)?;
    let publisher = Arc::clone(&shared);
    let watch_root = source;
    let stopped = Arc::new(AtomicBool::new(false));
    let watch_stopped = Arc::clone(&stopped);
    let passes = Arc::new(AtomicUsize::new(0));
    let watch_passes = Arc::clone(&passes);
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let watcher = std::thread::spawn(move || -> Result<(), String> {
        let mut ready = Some(ready_tx);
        let options = WatchLoopOptions {
            poll_only,
            reconcile: Duration::from_millis(100),
            duration: Some(Duration::from_secs(5)),
            ..WatchLoopOptions::default()
        };
        run_watch(
            &watch_root,
            &options,
            &watch_stopped,
            || -> Result<(), String> {
                let mut writer = publisher.lock().map_err(|error| error.to_string())?;
                syntaxmesh_source_host::reconcile_structural_sources(
                    &mut writer,
                    &watch_root,
                    syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
                    &extractor_fingerprint,
                    &resolver_fingerprint,
                )
                .map_err(|error| error.to_string())?;
                watch_passes.fetch_add(1, Ordering::Release);
                if let Some(sender) = ready.take() {
                    sender.send(()).map_err(|error| error.to_string())?;
                }
                Ok(())
            },
        )
        .map_err(|error| error.to_string())
    });
    // Registration and the initial callback complete before editing source.
    let registration = ready_rx.recv_timeout(Duration::from_secs(3));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        registration?;
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
                    .timeout(Duration::from_secs(2))
                    .build()?;
                let base = format!("http://{address}");
                std::fs::write(&file, "pub fn watch_after() {}\n")?;
                let waiting = Instant::now();
                let observed_generation = loop {
                    let response = require_status(
                        client
                            .get(format!("{base}/api/v1/search?text=watch_after"))
                            .send()?,
                        200,
                    )?;
                    if let Some(generation) = response.get("generation").and_then(Value::as_str)
                        && generation != old.0.to_hex()
                    {
                        if !response
                            .get("data")
                            .and_then(Value::as_array)
                            .is_some_and(|nodes| !nodes.is_empty())
                        {
                            return Err(
                                "watch published generation without expected new symbol".into()
                            );
                        }
                        break generation.to_owned();
                    }
                    if waiting.elapsed() >= Duration::from_secs(3) {
                        return Err("watch did not refresh live HTTP before deadline".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                while passes.load(Ordering::Acquire) < 3 {
                    if waiting.elapsed() >= Duration::from_secs(3) {
                        return Err("watch did not reconcile repeatedly".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                let repeated =
                    require_status(client.get(format!("{base}/api/v1/generation")).send()?, 200)?;
                if repeated.get("generation").and_then(Value::as_str)
                    != Some(observed_generation.as_str())
                {
                    return Err("unchanged reconciliation published another generation".into());
                }
                let retained = require_status(
                    client
                        .get(format!("{base}/api/v1/search"))
                        .query(&[
                            ("text", "watch_before".to_owned()),
                            ("generation", old.0.to_hex()),
                        ])
                        .send()?,
                    200,
                )?;
                let removed = require_status(
                    client
                        .get(format!("{base}/api/v1/search?text=watch_before"))
                        .send()?,
                    200,
                )?;
                if !retained
                    .get("data")
                    .and_then(Value::as_array)
                    .is_some_and(|nodes| !nodes.is_empty())
                    || !removed
                        .get("data")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty)
                {
                    return Err("watch failed to retain history or retract current source".into());
                }
                Ok(())
            })
            .await?;
        let _shutdown_result = shutdown_tx.send(());
        server.await??;
        client_result
    });
    stopped.store(true, Ordering::Release);
    let watch_result = watcher.join().map_err(|_panic| "watch worker panicked")?;
    drop(runtime);
    watch_result?;
    result?;
    if shared
        .lock()
        .map_err(|error| error.to_string())?
        .verify_statechronicle_history()?
        .is_none()
    {
        return Err("watch publication lost verification history".into());
    }
    drop(shared);
    Ok(())
}
