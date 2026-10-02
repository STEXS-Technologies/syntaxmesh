use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use syntaxmesh_http::SyntaxMeshHttp;
use syntaxmesh_mcp::SyntaxMeshMcp;
use syntaxmesh_ownership_host::{OwnerEndpoint, WriterLease};
use syntaxmesh_scanner::{SUPPORTED_SOURCE_EXTENSIONS, scan};
use syntaxmesh_source_host::{
    ProjectConfig, configured_source_engine, project_resolvers, reconcile_scanned_sources,
    reconcile_structural_sources, repository_scope,
};
use syntaxmesh_store_turso::TursoGraphStore;
use syntaxmesh_watch_host::run_watch;

use crate::options::Options;

mod pass_inputs;

pub(super) fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.root.canonicalize()?;
    if !root.is_dir() {
        return Err("daemon source root must be a directory".into());
    }
    let database = options.database.canonicalize()?;
    if database.starts_with(&root) {
        return Err("daemon database must be outside the source root".into());
    }
    let configuration = ProjectConfig::load(&root)?;
    let listener = std::net::TcpListener::bind(options.address)?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let ownership = WriterLease::acquire(&database)?;
    let endpoint = OwnerEndpoint::new(&database, address)?;
    let setup = configured_source_engine(
        TursoGraphStore::open(&database)?,
        &root,
        &configuration,
        options.verify,
    )?;
    let mut engine = setup.engine;
    reconcile_structural_sources(
        &mut engine,
        &root,
        SUPPORTED_SOURCE_EXTENSIONS,
        &setup.extractor_fingerprint,
        &setup.resolver_fingerprint,
    )?;
    let shared = Arc::new(Mutex::new(engine));
    let (repository, worktree) = repository_scope(&root);
    let host = SyntaxMeshHttp::from_shared_engine(Arc::clone(&shared), repository, worktree)?
        .with_owner_instance(endpoint.instance())?;
    let mcp = if options.mcp {
        Some(SyntaxMeshMcp::from_shared_engine(
            Arc::clone(&shared),
            repository,
            worktree,
            &root,
            options
                .tokenizer
                .as_deref()
                .ok_or("MCP tokenizer missing")?,
        )?)
    } else {
        None
    };
    let host = if let Some(tokenizer) = options.tokenizer {
        host.with_context(&root, &tokenizer)?
    } else {
        host
    };
    let mut additional = axum::Router::new();
    let mut mcp_config = StreamableHttpServerConfig::default().enforce_origin_validation();
    mcp_config.allowed_hosts = vec![address.to_string()];
    mcp_config.sse_keep_alive = None;
    mcp_config.max_request_body_bytes = 64 * 1024;
    let mcp_shutdown = mcp_config.cancellation_token.clone();
    if let Some(server) = mcp {
        let service = StreamableHttpService::new(
            move || Ok(server.clone()),
            Arc::new(LocalSessionManager::default()),
            mcp_config,
        );
        additional = additional.route_service("/mcp", service);
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let signal_stopped = Arc::clone(&stopped);
    ctrlc::try_set_handler(move || signal_stopped.store(true, Ordering::Release))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let publisher = Arc::clone(&shared);
    let watch_stopped = Arc::clone(&stopped);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let watcher = std::thread::spawn(move || -> Result<(), String> {
        let mut ready = Some(ready_tx);
        let mut active_configuration = configuration;
        let mut resolver_fingerprint = setup.resolver_fingerprint;
        let result = run_watch(
            &root,
            &options.watch,
            &watch_stopped,
            || -> Result<(), String> {
                publisher
                    .lock()
                    .map_err(|error| error.to_string())?
                    .recover_pending_workflows()
                    .map_err(|error| error.to_string())?;
                let (files, next_configuration) = pass_inputs::capture(
                    || {
                        scan(&root, SUPPORTED_SOURCE_EXTENSIONS)
                            .map(|inventory| inventory.files)
                            .map_err(|error| error.to_string())
                    },
                    || ProjectConfig::load(&root).map_err(|error| error.to_string()),
                )?;
                let policy_changed = active_configuration.verified_history()
                    != next_configuration.verified_history()
                    || active_configuration.module_resolution_profiles()
                        != next_configuration.module_resolution_profiles()
                    || active_configuration.python_source_roots()
                        != next_configuration.python_source_roots();
                let next_resolvers = if policy_changed {
                    Some(
                        project_resolvers(&root, &next_configuration)
                            .map_err(|error| error.to_string())?,
                    )
                } else {
                    None
                };
                let mut writer = publisher.lock().map_err(|error| error.to_string())?;
                if let Some(resolvers) = next_resolvers {
                    writer.reconfigure_source_policies(
                        resolvers.provider,
                        options.verify || next_configuration.verified_history(),
                    );
                    resolver_fingerprint = resolvers.fingerprint;
                    active_configuration = next_configuration;
                }
                reconcile_scanned_sources(
                    &mut writer,
                    &files,
                    &setup.extractor_fingerprint,
                    &resolver_fingerprint,
                )
                .map_err(|error| error.to_string())?;
                if let Some(sender) = ready.take() {
                    sender
                        .send(())
                        .map_err(|()| "daemon readiness receiver closed".to_owned())?;
                }
                Ok(())
            },
        )
        .map_err(|error| error.to_string());
        let _finished = finished_tx.send(());
        result
    });
    let retained_ownership = &ownership;
    let serving = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::from_std(listener)?;
        ready_rx.await.map_err(std::io::Error::other)?;
        endpoint
            .publish(retained_ownership)
            .map_err(std::io::Error::other)?;
        writeln!(std::io::stdout(), "syntaxmeshd listening on {address}")?;
        host.serve_with_routes(
            listener,
            async move {
                drop(finished_rx.await);
                mcp_shutdown.cancel();
            },
            additional,
        )
        .await
    });
    stopped.store(true, Ordering::Release);
    let watching = watcher.join();
    // Keep Turso's retained Engine outside the HTTP runtime's teardown.
    drop(runtime);
    drop(shared);
    watching.map_err(|_panic| "daemon writer thread panicked")??;
    serving?;
    Ok(())
}
