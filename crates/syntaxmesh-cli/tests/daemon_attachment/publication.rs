use super::*;
use axum::response::IntoResponse;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn neighbor_continuations_retain_original_graph_after_real_publication()
-> Result<(), Box<dyn Error>> {
    verify_publication(false)
}

#[test]
fn status_inventory_retains_original_generation_after_real_publication()
-> Result<(), Box<dyn Error>> {
    verify_publication(true)
}

fn verify_publication(status_inventory: bool) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    let file = root.join("lib.rs");
    std::fs::write(&file, wide_source("paged_before")?)?;
    if status_inventory {
        for index in 0..105 {
            std::fs::write(
                root.join(format!("inventory-{index}.txt")),
                "Inventory fixture\n",
            )?;
        }
    }
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&root)
        .arg(&database)
        .output()?;
    if !indexed.status.success() {
        return Err("fixture indexing failed".into());
    }
    let found = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("search-turso")
        .arg(&database)
        .arg("paged_before")
        .output()?;
    let seed = String::from_utf8(found.stdout)?
        .split('\t')
        .nth(2)
        .ok_or("seed missing")?
        .trim()
        .to_owned();
    let read_root = root.clone();
    let read = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
        command
            .arg(if status_inventory {
                "status-turso"
            } else {
                "neighbors-turso"
            })
            .arg(&database);
        if status_inventory {
            command.arg(&read_root);
        } else {
            command.arg(&seed);
        }
        command.output()
    };
    let baseline = read()?;
    if !baseline.status.success()
        || (!status_inventory && String::from_utf8_lossy(&baseline.stdout).lines().count() <= 100)
    {
        return Err("multi-page baseline missing".into());
    }
    let lease = WriterLease::acquire(&database)?;
    let setup = configured_source_engine(
        TursoGraphStore::open(&database)?,
        &root,
        &ProjectConfig::default(),
        false,
    )?;
    let shared = Arc::new(Mutex::new(setup.engine));
    let (repository, worktree) = repository_scope(&root);
    let before = shared
        .lock()
        .map_err(|error| error.to_string())?
        .current_generation(repository, worktree)?
        .ok_or("generation missing")?
        .generation;
    let backend = std::net::TcpListener::bind("127.0.0.1:0")?;
    backend.set_nonblocking(true)?;
    let backend_address = backend.local_addr()?;
    let proxy = std::net::TcpListener::bind("127.0.0.1:0")?;
    proxy.set_nonblocking(true)?;
    let endpoint = OwnerEndpoint::new(&database, proxy.local_addr()?)?;
    endpoint.publish(&lease)?;
    let host = SyntaxMeshHttp::from_shared_engine(Arc::clone(&shared), repository, worktree)?
        .with_owner_instance(endpoint.instance())?;
    let published = Arc::new(AtomicBool::new(false));
    let triggered = Arc::clone(&published);
    let writer = Arc::clone(&shared);
    let identity = endpoint.instance().to_owned();
    let transport = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let router = axum::Router::new().fallback(move |request: axum::extract::Request| {
        let transport = transport.clone();
        let identity = identity.clone();
        let writer = Arc::clone(&writer);
        let triggered = Arc::clone(&triggered);
        let root = root.clone();
        let file = file.clone();
        let resolver = setup.resolver_fingerprint.clone();
        async move {
            let forwarded = transport
                .get(format!("http://{backend_address}{}", request.uri()))
                .header("x-syntaxmesh-owner-instance", &identity)
                .send()
                .await;
            let Ok(response) = forwarded else {
                return axum::http::StatusCode::BAD_GATEWAY.into_response();
            };
            let status = response.status();
            let headers = response.headers().clone();
            let Ok(bytes) = response.bytes().await else {
                return axum::http::StatusCode::BAD_GATEWAY.into_response();
            };
            let first_page = if status_inventory {
                request.uri().path() == "/api/v1/files"
                    && !request
                        .uri()
                        .query()
                        .is_some_and(|query| query.contains("after_file="))
            } else {
                !request
                    .uri()
                    .query()
                    .is_some_and(|query| query.contains("cursor="))
            };
            if status_inventory && first_page {
                let has_more = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("data")
                            .and_then(|data| data.get("has_more"))
                            .and_then(serde_json::Value::as_bool)
                    });
                if has_more != Some(true) {
                    return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            if first_page && !triggered.swap(true, Ordering::AcqRel) {
                let publication = tokio::task::spawn_blocking(move || -> Result<(), String> {
                    std::fs::write(file, "pub fn paged_after() {}")
                        .map_err(|error| error.to_string())?;
                    if status_inventory {
                        std::fs::remove_file(root.join("inventory-104.txt"))
                            .map_err(|error| error.to_string())?;
                    }
                    let mut engine = writer.lock().map_err(|error| error.to_string())?;
                    reconcile_structural_sources(
                        &mut engine,
                        &root,
                        syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
                        &setup.extractor_fingerprint,
                        &resolver,
                    )
                    .map_err(|error| error.to_string())?;
                    Ok(())
                })
                .await;
                if !matches!(publication, Ok(Ok(()))) {
                    return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            let mut result = (status, bytes).into_response();
            *result.headers_mut() = headers;
            result
        }
    });
    let (stop_host, host_stopped) = tokio::sync::oneshot::channel();
    let (stop_proxy, proxy_stopped) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime
            .block_on(async {
                let backend = tokio::net::TcpListener::from_std(backend)?;
                let proxy = tokio::net::TcpListener::from_std(proxy)?;
                let (serving, forwarding) = tokio::join!(
                    host.serve_until(backend, async {
                        drop(host_stopped.await);
                    }),
                    axum::serve(proxy, router)
                        .with_graceful_shutdown(async {
                            drop(proxy_stopped.await);
                        })
                        .into_future()
                );
                serving?;
                forwarding
            })
            .map_err(|error| error.to_string())
    });
    let output = read();
    let _host_stop = stop_host.send(());
    let _proxy_stop = stop_proxy.send(());
    worker
        .join()
        .map_err(|_panic| "publication worker panicked")??;
    let output = output?;
    let engine = shared.lock().map_err(|error| error.to_string())?;
    let current = engine
        .current_generation(repository, worktree)?
        .ok_or("current missing")?
        .generation;
    if !published.load(Ordering::Acquire)
        || current == before
        || !output.status.success()
        || output.stdout != baseline.stdout
        || engine.query(current).search("paged_after", 10)?.is_empty()
    {
        return Err("publication between pages changed pinned output or was not exercised".into());
    }
    drop(engine);
    drop(shared);
    drop(lease);
    Ok(())
}
