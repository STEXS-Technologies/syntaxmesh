use super::*;
use std::sync::{Arc, Mutex};
use syntaxmesh_source_host::{
    ProjectConfig, configured_source_engine, reconcile_structural_sources, repository_scope,
};

#[test]
fn shared_mixed_engine_refreshes_mcp_queries_without_reopening() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    let file = root.join("module.ts");
    fs::write(&file, "export function mcp_before() {}")?;
    fs::write(
        root.join("design.md"),
        "# Architecture\nRust owns execution.\n",
    )?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let config = ProjectConfig::load(&root)?;
    let mut setup =
        configured_source_engine(TursoGraphStore::open(&database)?, &root, &config, true)?;
    let initial = reconcile_structural_sources(
        &mut setup.engine,
        &root,
        SUPPORTED_SOURCE_EXTENSIONS,
        &setup.extractor_fingerprint,
        &setup.resolver_fingerprint,
    )?;
    let shared = Arc::new(Mutex::new(setup.engine));
    let (repository, worktree) = repository_scope(&root);
    if SyntaxMeshMcp::from_shared_engine(
        Arc::clone(&shared),
        RepositoryId::derive(&[b"foreign"]),
        worktree,
        &root,
        "cl100k_base",
    )
    .is_ok()
    {
        return Err("shared MCP accepted a foreign scope".into());
    }
    let server = SyntaxMeshMcp::from_shared_engine(
        Arc::clone(&shared),
        repository,
        worktree,
        &root,
        "cl100k_base",
    )?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let before = content_json(&runtime.block_on(server.search(Parameters(SearchArgs {
        text: "mcp_before".to_owned(),
        limit: None,
    })))?)?;
    if before.get("generation").and_then(Value::as_str)
        != Some(initial.generation.0.to_hex().as_str())
        || !before
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(|nodes| !nodes.is_empty())
    {
        return Err("shared MCP initial search was not scoped correctly".into());
    }
    let indexed = server.clone().with_indexed_context();
    let indexed_request = |selected: Option<GenerationId>, text: &str| ContextArgs {
        generation: selected.map(|id| id.0.to_hex()),
        query: text.to_owned(),
        seed_nodes: None,
        token_budget: 4096,
        max_hops: Some(0),
        max_candidates: Some(8),
    };
    let _initial_pack =
        runtime.block_on(indexed.context(Parameters(indexed_request(None, "mcp before"))))?;
    let before_index = indexed
        .planner_cache
        .lock()
        .map_err(|error| error.to_string())?
        .cached_index()
        .ok_or("initial indexed live request did not cache")?;
    fs::write(&file, "export function mcp_after() {}")?;
    let current = {
        let mut writer = shared.lock().map_err(|error| error.to_string())?;
        reconcile_structural_sources(
            &mut writer,
            &root,
            SUPPORTED_SOURCE_EXTENSIONS,
            &setup.extractor_fingerprint,
            &setup.resolver_fingerprint,
        )?
    };
    let after = content_json(&runtime.block_on(server.search(Parameters(SearchArgs {
        text: "mcp_after".to_owned(),
        limit: None,
    })))?)?;
    let removed = content_json(&runtime.block_on(server.search(Parameters(SearchArgs {
        text: "mcp_before".to_owned(),
        limit: None,
    })))?)?;
    let status = content_json(&runtime.block_on(server.status())?)?;
    if after.get("generation").and_then(Value::as_str)
        != Some(current.generation.0.to_hex().as_str())
        || status.get("generation") != after.get("generation")
        || !after
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(|nodes| !nodes.is_empty())
        || !removed
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return Err("shared MCP did not refresh generation and search atomically".into());
    }
    let retained = runtime.block_on(server.with_selected_query(
        Some(initial.generation),
        |query| {
            query
                .historical_search("mcp_before", 20)
                .map_err(|error| error.to_string())
        },
    ))?;
    if retained.is_empty() {
        return Err("shared MCP lost historical query selection".into());
    }
    let node_id = after
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| nodes.first())
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str)
        .ok_or("shared MCP search node ID missing")?
        .to_owned();
    let node = content_json(&runtime.block_on(server.node(Parameters(NodeArgs {
        id: node_id.clone(),
    })))?)?;
    let neighbors = content_json(&runtime.block_on(server.neighbors(Parameters(
        super::super::NeighborsArgs {
            id: node_id.clone(),
        },
    )))?)?;
    let path = content_json(&runtime.block_on(server.path(Parameters(
        super::super::PathArgs {
            start: node_id.clone(),
            target: node_id,
            max_hops: 1,
        },
    )))?)?;
    for result in [node, neighbors, path] {
        if result.get("generation") != after.get("generation") {
            return Err("shared MCP tool retained its startup generation label".into());
        }
    }
    for selected in [None, Some(initial.generation)] {
        let context =
            content_json(&runtime.block_on(server.context(Parameters(ContextArgs {
                generation: selected.map(|generation| generation.0.to_hex()),
                query: "mcp".to_owned(),
                seed_nodes: None,
                token_budget: 4096,
                max_hops: None,
                max_candidates: None,
            })))?)?;
        if context.get("generation")
            != Some(&serde_json::to_value(
                selected.unwrap_or(current.generation),
            )?)
        {
            return Err("shared MCP context selected the wrong generation".into());
        }
    }
    let refreshed = content_json(
        &runtime.block_on(indexed.context(Parameters(indexed_request(None, "mcp after"))))?,
    )?;
    let after_index = indexed
        .planner_cache
        .lock()
        .map_err(|error| error.to_string())?
        .cached_index()
        .ok_or("refreshed indexed live request did not cache")?;
    if Arc::ptr_eq(&before_index, &after_index)
        || refreshed.get("generation") != Some(&serde_json::to_value(current.generation)?)
    {
        return Err("indexed live request reused a stale generation".into());
    }
    let historical_pack = content_json(&runtime.block_on(indexed.context(Parameters(
        indexed_request(Some(initial.generation), "mcp before"),
    )))?)?;
    if historical_pack.get("generation") != Some(&serde_json::to_value(initial.generation)?)
        || historical_pack
            .get("warnings")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        || historical_pack
            .get("items")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("kind").and_then(Value::as_str) == Some("source_evidence")
                        && item.get("source_path").and_then(Value::as_str) == Some("module.ts")
                })
            })
    {
        return Err("historical indexed context admitted changed source or lost warning".into());
    }
    let again = content_json(
        &runtime.block_on(indexed.context(Parameters(indexed_request(None, "mcp after"))))?,
    )?;
    if again != refreshed {
        return Err("historical cache replacement changed current output".into());
    }
    drop(indexed);
    drop(server);
    drop(runtime);
    if shared
        .lock()
        .map_err(|error| error.to_string())?
        .verify_statechronicle_history()?
        .is_none()
    {
        return Err("shared MCP composition lost verification history".into());
    }
    drop(shared);
    Ok(())
}
