use std::error::Error;
use std::fmt::Write as _;
use std::process::Command;
use std::sync::{Arc, Mutex};

use syntaxmesh_http::SyntaxMeshHttp;
use syntaxmesh_ownership_host::{OwnerEndpoint, WriterLease};
use syntaxmesh_source_host::{
    ProjectConfig, configured_source_engine, reconcile_structural_sources, repository_scope,
};
use syntaxmesh_store_turso::TursoGraphStore;

#[path = "daemon_attachment/transport.rs"]
mod transport;

#[path = "daemon_attachment/neighbor_pages.rs"]
mod neighbor_pages;

#[path = "daemon_attachment/publication.rs"]
mod publication;

#[path = "daemon_attachment/diagnostic_pages.rs"]
mod diagnostic_pages;

#[path = "daemon_attachment/rejection_pages.rs"]
mod rejection_pages;

#[path = "daemon_attachment/status_reports.rs"]
mod status_reports;

#[path = "daemon_attachment/integrity_reports.rs"]
mod integrity_reports;

fn wide_source(name: &str) -> Result<String, std::fmt::Error> {
    let mut source = format!("pub fn {name}() {{");
    for index in 0..120 {
        write!(source, "target_{index}();")?;
    }
    source.push('}');
    for index in 0..120 {
        write!(source, "fn target_{index}() {{}}")?;
    }
    Ok(source)
}

#[test]
fn search_attaches_to_owned_live_engine_preserving_output_and_rejecting_stale_records()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    let source = root.join("lib.rs");
    std::fs::write(&source, wide_source("attached_before")?)?;
    for index in 0..105 {
        std::fs::write(
            root.join(format!("inventory-{index}.txt")),
            "Inventory fixture\n",
        )?;
    }
    let typescript = root.join("main.ts");
    std::fs::write(&typescript, "import { missing } from './missing';\n")?;
    std::fs::write(
        root.join("syntaxmesh.toml"),
        "[module_resolution]\nprofile = \"node\"\n",
    )?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&root)
        .arg(&database)
        .output()?;
    if !indexed.status.success() {
        return Err(String::from_utf8_lossy(&indexed.stderr).into_owned().into());
    }
    let search = |text: &str| {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("search-turso")
            .arg(&database)
            .arg(text)
            .output()
    };
    let embedded = search("attached_before")?;
    if !embedded.status.success() || embedded.stdout.is_empty() {
        return Err("embedded search baseline missing".into());
    }
    let node_id = String::from_utf8(embedded.stdout.clone())?
        .split('\t')
        .nth(2)
        .ok_or("baseline node ID missing")?
        .trim()
        .to_owned();
    let baseline_store = TursoGraphStore::open(&database)?;
    let old = baseline_store
        .latest_generation()
        .ok_or("baseline generation missing")?
        .generation
        .0
        .to_hex();
    drop(baseline_store);
    let node = |generation: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
        command
            .arg(if generation.is_some() {
                "node-at-turso"
            } else {
                "node-turso"
            })
            .arg(&database);
        if let Some(generation) = generation {
            command.arg(generation);
        }
        command.arg(&node_id).output()
    };
    let baseline_node = node(None)?;
    let rejection_cursor = rejection_pages::prepare_rejections(&database, &root)?;
    let rejections = |limit: usize, after: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
        command
            .arg("workflow-rejections-turso")
            .arg(&database)
            .arg(limit.to_string());
        if let Some(cursor) = after {
            command.arg(cursor);
        }
        command.output()
    };
    let mut rejection_baselines = Vec::new();
    for (limit, after) in [
        (0, None),
        (1, None),
        (100, None),
        (1000, None),
        (1, Some(rejection_cursor.as_str())),
        (100, Some(rejection_cursor.as_str())),
    ] {
        let output = rejections(limit, after)?;
        if !output.status.success() {
            return Err("embedded rejection baseline failed".into());
        }
        if limit > 0 && !String::from_utf8_lossy(&output.stdout).contains("reason=stale_base") {
            return Err("nonempty rejection baseline missing".into());
        }
        rejection_baselines.push((limit, after, output.stdout));
    }
    let diagnostics = || {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("resolution-diagnostics-turso")
            .arg(&database)
            .output()
    };
    let status = |check_source: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
        command.arg("status-turso").arg(&database);
        if check_source {
            command.arg(&root);
        }
        command.output()
    };
    let mut status_baselines = Vec::new();
    let integrity = || {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("integrity-turso")
            .arg(&database)
            .output()
    };
    let baseline_integrity = integrity()?;
    if baseline_integrity.stdout.is_empty() {
        return Err("backend integrity report missing".into());
    }
    let foreign_root = fixture.path().join("foreign");
    std::fs::create_dir(&foreign_root)?;
    let foreign_status = || {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("status-turso")
            .arg(&database)
            .arg(&foreign_root)
            .output()
    };
    let baseline_foreign = foreign_status()?;
    if baseline_foreign.status.success() || !baseline_foreign.stdout.is_empty() {
        return Err("embedded status accepted foreign source scope".into());
    }
    for check_source in [false, true] {
        let output = status(check_source)?;
        if !output.status.success() {
            return Err("embedded status baseline failed".into());
        }
        let file_count = String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("files="))
            .ok_or("status inventory count missing")?
            .parse::<usize>()?;
        if file_count <= 100 {
            return Err("status fixture did not require file paging".into());
        }
        status_baselines.push((check_source, output.stdout));
    }
    let baseline_diagnostics = diagnostics()?;
    if !baseline_diagnostics.status.success() {
        return Err("embedded diagnostic baseline failed".into());
    }
    if !String::from_utf8_lossy(&baseline_diagnostics.stdout).contains("ModuleResolutionDiagnostic")
    {
        return Err("nonempty diagnostic fixture missing".into());
    }
    let baseline_history = node(Some(&old))?;
    if !baseline_node.status.success() || !baseline_history.status.success() {
        return Err("embedded node baseline failed".into());
    }
    let neighbors = |seed: &str| {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("neighbors-turso")
            .arg(&database)
            .arg(seed)
            .output()
    };
    let baseline_neighbors = neighbors(&node_id)?;
    if !baseline_neighbors.status.success()
        || String::from_utf8_lossy(&baseline_neighbors.stdout)
            .lines()
            .count()
            <= 100
    {
        return Err("multi-page embedded neighbors baseline missing".into());
    }
    let historical =
        |seed: &str, direction: &str, limit: usize, after: Option<&str>, selected: &str| {
            let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
            command
                .arg("neighbors-at-turso")
                .arg(&database)
                .arg(selected)
                .arg(seed)
                .arg(direction)
                .arg(limit.to_string());
            if let Some(after) = after {
                command.arg(after);
            }
            command.output()
        };
    let first_page = historical(&node_id, "outgoing", 1, None, &old)?;
    let first_record: syntaxmesh_api_model::TemporalRecord = serde_json::from_str(
        String::from_utf8_lossy(&first_page.stdout)
            .lines()
            .next()
            .ok_or("first neighbor absent")?,
    )?;
    let syntaxmesh_api_model::TemporalRecord::HistoricalNeighbor { edge, neighbor, .. } =
        first_record
    else {
        return Err("first neighbor item missing".into());
    };
    let history_cases = [
        (node_id.clone(), "outgoing", 1, None),
        (node_id.clone(), "outgoing", 101, None),
        (node_id.clone(), "outgoing", 1000, None),
        (node_id.clone(), "outgoing", 101, Some(edge.id.0.to_hex())),
        (neighbor.id.0.to_hex(), "incoming", 1, None),
        (neighbor.id.0.to_hex(), "incoming", 1000, None),
        ("f".repeat(64), "outgoing", 1000, None),
    ];
    let mut historical_baselines = Vec::new();
    for (seed, direction, limit, after) in &history_cases {
        let output = historical(seed, direction, *limit, after.as_deref(), &old)?;
        if !output.status.success() {
            return Err("historical baseline failed".into());
        }
        historical_baselines.push(output.stdout);
    }
    let lease = WriterLease::acquire(&database)?;
    let setup = configured_source_engine(
        TursoGraphStore::open(&database)?,
        &root,
        &ProjectConfig::load(&root)?,
        false,
    )?;
    let shared = Arc::new(Mutex::new(setup.engine));
    let (repository, worktree) = repository_scope(&root);
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = OwnerEndpoint::new(&database, listener.local_addr()?)?;
    let host = SyntaxMeshHttp::from_shared_engine(Arc::clone(&shared), repository, worktree)?
        .with_owner_instance(endpoint.instance())?;
    endpoint.publish(&lease)?;
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime
            .block_on(async {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                host.serve_until(listener, async {
                    drop(stop_rx.await);
                })
                .await
            })
            .map_err(|error| error.to_string())
    });
    let mut latest_status = Vec::new();
    let result = (|| -> Result<(), Box<dyn Error>> {
        let attached_integrity = integrity()?;
        if attached_integrity.status.code() != baseline_integrity.status.code()
            || attached_integrity.stdout != baseline_integrity.stdout
            || attached_integrity.stderr != baseline_integrity.stderr
        {
            return Err("attached backend integrity report or exit differs".into());
        }
        let foreign = foreign_status()?;
        if foreign.status.success()
            || foreign.stdout != baseline_foreign.stdout
            || foreign.stderr != baseline_foreign.stderr
        {
            return Err("attached status changed foreign-root rejection".into());
        }
        for (check_source, baseline) in &status_baselines {
            let output = status(*check_source)?;
            if !output.status.success() || &output.stdout != baseline {
                return Err(format!(
                    "attached status differs: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
        }
        for (limit, after, baseline) in &rejection_baselines {
            let output = rejections(*limit, *after)?;
            if !output.status.success() || &output.stdout != baseline {
                return Err(format!(
                    "attached rejection output differs: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
        }
        let attached_diagnostics = diagnostics()?;
        if !attached_diagnostics.status.success()
            || attached_diagnostics.stdout != baseline_diagnostics.stdout
        {
            return Err(format!(
                "attached diagnostic export differs: {}",
                String::from_utf8_lossy(&attached_diagnostics.stderr)
            )
            .into());
        }
        for ((seed, direction, limit, after), baseline) in
            history_cases.iter().zip(&historical_baselines)
        {
            let output = historical(seed, direction, *limit, after.as_deref(), &old)?;
            if !output.status.success() || &output.stdout != baseline {
                return Err(format!(
                    "attached historical neighbors differ: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
        }
        let unknown_history = historical(&node_id, "outgoing", 1, None, &"f".repeat(64))?;
        if unknown_history.status.success() || !unknown_history.stdout.is_empty() {
            return Err("unknown historical generation accepted".into());
        }
        let attached = search("attached_before")?;
        if !attached.status.success() || attached.stdout != embedded.stdout {
            return Err("attached search changed output or reopened owned Turso".into());
        }
        let attached_node = node(None)?;
        let attached_neighbors = neighbors(&node_id)?;
        let missing_neighbors = neighbors(&"f".repeat(64))?;
        if !attached_neighbors.status.success()
            || attached_neighbors.stdout != baseline_neighbors.stdout
            || !missing_neighbors.status.success()
            || !missing_neighbors.stdout.is_empty()
        {
            return Err(
                "attached neighbors truncated pages, changed output, or rejected a missing seed"
                    .into(),
            );
        }
        let attached_history = node(Some(&old))?;
        if !attached_node.status.success()
            || attached_node.stdout != baseline_node.stdout
            || !attached_history.status.success()
            || attached_history.stdout != baseline_history.stdout
        {
            return Err("attached node formatter or historical selection changed output".into());
        }
        let unknown_generation = node(Some(&"f".repeat(64)))?;
        if unknown_generation.status.success()
            || !unknown_generation.stdout.is_empty()
            || !String::from_utf8_lossy(&unknown_generation.stderr).contains("HTTP 404")
        {
            return Err("unknown attached generation did not fail closed".into());
        }
        std::fs::write(&source, "pub fn attached_after() {}")?;
        std::fs::write(&typescript, "export const resolved = 1;\n")?;
        let stale_status = status(true)?;
        if stale_status.status.success()
            || !String::from_utf8_lossy(&stale_status.stdout).contains("index_freshness=stale")
        {
            return Err("attached status lost stale freshness exit behavior".into());
        }
        let mut writer = shared.lock().map_err(|error| error.to_string())?;
        reconcile_structural_sources(
            &mut writer,
            &root,
            syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
            &setup.extractor_fingerprint,
            &setup.resolver_fingerprint,
        )?;
        drop(writer);
        let refreshed_status = status(true)?;
        if !refreshed_status.status.success()
            || !String::from_utf8_lossy(&refreshed_status.stdout)
                .contains("index_freshness=current")
        {
            return Err("attached status did not refresh after publication".into());
        }
        latest_status = refreshed_status.stdout;
        let refreshed_diagnostics = diagnostics()?;
        if !refreshed_diagnostics.status.success()
            || String::from_utf8_lossy(&refreshed_diagnostics.stdout)
                .contains("ModuleResolutionDiagnostic")
            || refreshed_diagnostics.stdout == baseline_diagnostics.stdout
        {
            return Err("attached diagnostics failed to refresh after publication".into());
        }
        for ((seed, direction, limit, after), baseline) in
            history_cases.iter().zip(&historical_baselines)
        {
            let output = historical(seed, direction, *limit, after.as_deref(), &old)?;
            if !output.status.success() || &output.stdout != baseline {
                return Err("publication changed attached historical neighbors".into());
            }
        }
        let after = search("attached_after")?;
        if !after.status.success()
            || after.stdout.is_empty()
            || !search("attached_before")?.stdout.is_empty()
        {
            return Err("attached CLI search failed to refresh".into());
        }
        let retained = node(Some(&old))?;
        let removed = node(None)?;
        if !retained.status.success()
            || retained.stdout != baseline_history.stdout
            || removed.status.success()
            || !removed.stdout.is_empty()
        {
            return Err(
                "attached node reads lost historical selection or retained a removed current node"
                    .into(),
            );
        }
        let wrong = OwnerEndpoint::new(&database, endpoint.address())?;
        let before_rejection = std::fs::read(&database)?;
        wrong.publish(&lease)?;
        let rejected = search("attached_after")?;
        if rejected.status.success()
            || !rejected.stdout.is_empty()
            || !String::from_utf8_lossy(&rejected.stderr)
                .contains("owner instance response mismatch")
        {
            return Err("stale instance did not fail closed".into());
        }
        lease.publish_discovery(b"{}")?;
        let malformed = search("attached_after")?;
        if malformed.status.success()
            || !malformed.stdout.is_empty()
            || !String::from_utf8_lossy(&malformed.stderr).contains("invalid endpoint record")
            || std::fs::read(&database)? != before_rejection
        {
            return Err("invalid active discovery allowed fallback or changed storage".into());
        }
        Ok(())
    })();
    let _stopped = stop_tx.send(());
    worker.join().map_err(|_panic| "HTTP worker panicked")??;
    drop(shared);
    drop(lease);
    result?;
    let fallback_integrity = integrity()?;
    if fallback_integrity.status.code() != baseline_integrity.status.code()
        || fallback_integrity.stdout != baseline_integrity.stdout
    {
        return Err("leased backend integrity fallback differs".into());
    }
    let fallback_status = status(true)?;
    if !fallback_status.status.success() || fallback_status.stdout != latest_status {
        return Err("leased status fallback changed current report".into());
    }
    for (limit, after, baseline) in &rejection_baselines {
        let output = rejections(*limit, *after)?;
        if !output.status.success() || &output.stdout != baseline {
            return Err("leased rejection fallback changed output".into());
        }
    }
    let fallback = search("attached_after")?;
    if !fallback.status.success() || fallback.stdout.is_empty() {
        return Err("inactive stale record prevented leased embedded fallback".into());
    }
    let retained_fallback = node(Some(&old))?;
    for ((seed, direction, limit, after), baseline) in
        history_cases.iter().zip(&historical_baselines)
    {
        let output = historical(seed, direction, *limit, after.as_deref(), &old)?;
        if !output.status.success() || &output.stdout != baseline {
            return Err("leased embedded historical-neighbor fallback changed output".into());
        }
    }
    let removed_fallback = node(None)?;
    if !retained_fallback.status.success()
        || retained_fallback.stdout != baseline_history.stdout
        || removed_fallback.status.success()
        || !removed_fallback.stdout.is_empty()
    {
        return Err("leased embedded node fallback changed retained/current semantics".into());
    }
    Ok(())
}
