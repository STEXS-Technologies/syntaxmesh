use super::{ContextArgs, SyntaxMeshMcp, content_json, indexed_server};
use rmcp::handler::server::wrapper::Parameters;
use std::error::Error;
use std::sync::Arc;
use syntaxmesh_query::GenerationIdentifierIndex;

#[test]
fn source_content_context_preserves_coverage_history_and_explicit_reads()
-> Result<(), Box<dyn Error>> {
    use syntaxmesh_api_model::ContextPack;
    use syntaxmesh_core::{GenerationId, IndexRunId, NodeKind, RepositoryId, WorktreeId};
    use syntaxmesh_engine::{SourceSyntaxPolicy, SyntaxMeshEngine};
    use syntaxmesh_lang_rust::RustExtractor;
    use syntaxmesh_store_turso::TursoGraphStore;
    let (fixture, initial_host, _) = indexed_server()?;
    drop(initial_host);
    let root = fixture.0.path().canonicalize()?;
    let database = root.join("graph.db");
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"mcp-test-repository"]),
        WorktreeId::derive(&[b"mcp-test-worktree"]),
    )
    .with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures);
    let retained = GenerationId::derive(&[b"mcp-content-backfill"]);
    engine.index(
        &syntaxmesh_scanner::scan(&root, &["rs"])?.files,
        IndexRunId::derive(&[b"mcp-content-backfill"]),
        retained,
    )?;
    let metadata = engine
        .query(retained)
        .search("lib", 100)?
        .into_iter()
        .find(|node| {
            matches!(&node.kind, NodeKind::External {namespace, ..}
            if namespace == syntaxmesh_language_sdk::SOURCE_PROCESSING_NAMESPACE)
        })
        .ok_or("coverage metadata was not published")?
        .id;
    std::fs::write(root.join("src/extra.rs"), "pub fn extra() {}")?;
    let latest = GenerationId::derive(&[b"mcp-content-latest"]);
    engine.index(
        &syntaxmesh_scanner::scan(&root, &["rs"])?.files,
        IndexRunId::derive(&[b"mcp-content-latest"]),
        latest,
    )?;
    drop(engine);
    let plain = SyntaxMeshMcp::open(database, &root, "cl100k_base")?;
    let content = plain.clone().with_source_content_context();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    for (generation, expected) in [
        (None, latest),
        (Some(retained.0.to_hex()), retained),
        (None, latest),
    ] {
        let args = ContextArgs {
            generation: generation.clone(),
            query: "lib caller".to_owned(),
            seed_nodes: None,
            token_budget: 8192,
            max_hops: Some(0),
            max_candidates: Some(32),
        };
        let pack: ContextPack = serde_json::from_value(content_json(
            &runtime.block_on(content.context(Parameters(args)))?,
        )?)?;
        if pack.generation != expected
            || pack.items.is_empty()
            || pack
                .items
                .iter()
                .any(|item| item.node_ids.contains(&metadata))
        {
            return Err(
                "content context lost source evidence or selected coverage metadata".into(),
            );
        }
        let explicit = || ContextArgs {
            generation: generation.clone(),
            query: "lib".to_owned(),
            seed_nodes: Some(vec![metadata.0.to_hex()]),
            token_budget: 8192,
            max_hops: Some(0),
            max_candidates: Some(32),
        };
        let ordinary = content_json(&runtime.block_on(plain.context(Parameters(explicit())))?)?;
        let selected = content_json(&runtime.block_on(content.context(Parameters(explicit())))?)?;
        if selected != ordinary {
            return Err("source-content policy altered explicit evidence reads".into());
        }
        let explicit_pack: ContextPack = serde_json::from_value(selected)?;
        if !explicit_pack
            .items
            .iter()
            .any(|item| item.node_ids.contains(&metadata))
        {
            return Err("explicit coverage evidence became inaccessible".into());
        }
    }
    Ok(())
}

#[test]
fn source_content_builder_isolates_policy_cache_and_clones_share_it() -> Result<(), Box<dyn Error>>
{
    let (_fixture, plain, _) = indexed_server()?;
    let raw = plain.with_indexed_context();
    let content = raw.clone().with_source_content_context();
    let cloned = content.clone();
    if raw.source_content_context
        || !content.source_content_context
        || !cloned.source_content_context
        || Arc::ptr_eq(&raw.planner_cache, &content.planner_cache)
        || !Arc::ptr_eq(&content.planner_cache, &cloned.planner_cache)
    {
        return Err("source-content policy reused another policy's cache".into());
    }
    Ok(())
}

#[test]
fn superseded_roles_survive_publication_and_host_reopen() -> Result<(), Box<dyn Error>> {
    use syntaxmesh_api_model::ContextPack;
    use syntaxmesh_core::{GenerationId, IndexRunId, RepositoryId, WorktreeId};
    use syntaxmesh_engine::SyntaxMeshEngine;
    use syntaxmesh_lang_rust::RustExtractor;
    use syntaxmesh_query::SourceRolePreference;
    use syntaxmesh_store_turso::TursoGraphStore;

    let (fixture, plain, previous) = super::indexed_server_with_source(Some(
        "pub fn shared_logic() {}\n#[tokio::test]\nasync fn shared_test() {}\n",
    ))?;
    drop(plain);
    let root = fixture.0.path().canonicalize()?;
    std::fs::write(
        root.join("src/lib.rs"),
        "#[test]\nfn shared_logic() {}\npub async fn shared_test() {}\n",
    )?;
    let database = root.join("graph.db");
    let latest = GenerationId::derive(&[b"mcp-role-history-new"]);
    let mut updater = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"mcp-test-repository"]),
        WorktreeId::derive(&[b"mcp-test-worktree"]),
    );
    updater.index(
        &syntaxmesh_scanner::scan(&root, &["rs"])?.files,
        IndexRunId::derive(&[b"mcp-role-history-run"]),
        latest,
    )?;
    drop(updater);
    let host = SyntaxMeshMcp::open(database, &root, "cl100k_base")?
        .with_indexed_context_role_preference(SourceRolePreference::TestIntentFirst);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // Alternate roots to exercise cache identity eviction as well as persisted roles.
    for (selected, expected, test_first) in [
        (None, latest, false),
        (Some(previous), previous, true),
        (None, latest, false),
    ] {
        let pack: ContextPack = serde_json::from_value(content_json(&runtime.block_on(
            host.context(Parameters(ContextArgs {
                generation: selected.map(|id| id.0.to_hex()),
                query: "shared".to_owned(),
                seed_nodes: None,
                token_budget: 8192,
                max_hops: Some(0),
                max_candidates: Some(32),
            })),
        )?)?)?;
        let positions = ["shared_logic", "shared_test"]
            .map(|name| pack.items.iter().position(|item| item.text.contains(name)));
        let [Some(logic), Some(test)] = positions else {
            return Err("superseded role context lost callable evidence".into());
        };
        if pack.generation != expected || pack.token_count > 8192 || (test < logic) != test_first {
            return Err("role context reused another generation's role evidence".into());
        }
    }
    Ok(())
}

#[test]
fn explicit_roles_reach_current_and_retained_host_context() -> Result<(), Box<dyn Error>> {
    use syntaxmesh_api_model::ContextPack;
    use syntaxmesh_query::SourceRolePreference;
    let (_fixture, plain, generation) = super::indexed_server_with_source(Some(
        "pub fn shared_logic() {}\n#[tokio::test]\nasync fn shared_test() {}\n",
    ))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    for retained in [None, Some(generation.0.to_hex())] {
        let mut orders = Vec::new();
        for preference in [
            SourceRolePreference::TestIntentFirst,
            SourceRolePreference::TestIntentLast,
        ] {
            let host = plain
                .clone()
                .with_indexed_context_role_preference(preference);
            let args = ContextArgs {
                query: "shared".to_owned(),
                generation: retained.clone(),
                seed_nodes: None,
                token_budget: 8192,
                max_hops: Some(0),
                max_candidates: Some(32),
            };
            let pack: ContextPack = serde_json::from_value(content_json(
                &runtime.block_on(host.context(Parameters(args)))?,
            )?)?;
            if pack.generation != generation || pack.token_count > 8192 {
                return Err("role preference lost generation or token bounds".into());
            }
            let positions = ["shared_logic", "shared_test"]
                .map(|name| pack.items.iter().position(|item| item.text.contains(name)));
            let [Some(logic), Some(test)] = positions else {
                return Err("role fixture did not retrieve both callable definitions".into());
            };
            orders.push(test < logic);
        }
        if orders != [true, false] {
            return Err("host failed to apply explicit first/last role order".into());
        }
    }
    Ok(())
}

#[test]
fn explicit_role_builder_clones_preference_and_neutral_builder_resets_it()
-> Result<(), Box<dyn Error>> {
    use syntaxmesh_query::SourceRolePreference;
    let (_fixture, plain, _) = indexed_server()?;
    if plain.indexed_context || plain.source_role_preference != SourceRolePreference::Neutral {
        return Err("ordinary host changed its default policy".into());
    }
    for preference in [
        SourceRolePreference::TestIntentFirst,
        SourceRolePreference::TestIntentLast,
    ] {
        let selected = plain
            .clone()
            .with_indexed_context_role_preference(preference);
        let cloned = selected.clone();
        if !cloned.indexed_context
            || cloned.source_role_preference != preference
            || !Arc::ptr_eq(&selected.planner_cache, &cloned.planner_cache)
        {
            return Err("explicit preference or shared cache was lost on clone".into());
        }
        let reset = cloned.with_indexed_context();
        if !reset.indexed_context || reset.source_role_preference != SourceRolePreference::Neutral {
            return Err("neutral builder retained an earlier role preference".into());
        }
    }
    Ok(())
}

#[test]
fn discovery_policy_is_explicit_cloneable_and_preserves_candidate_cache()
-> Result<(), Box<dyn Error>> {
    use syntaxmesh_query::{DiscoveryPackingPreference, SourceRolePreference};
    let (_fixture, plain, _) = indexed_server()?;
    if plain.discovery_preference != DiscoveryPackingPreference::Balanced {
        return Err("ordinary host changed discovery default".into());
    }
    let selected = plain
        .clone()
        .with_indexed_context_role_preference(SourceRolePreference::TestIntentLast)
        .with_indexed_context_discovery_preference(DiscoveryPackingPreference::LexicalFirst);
    let cloned = selected.clone();
    if !cloned.indexed_context
        || cloned.discovery_preference != DiscoveryPackingPreference::LexicalFirst
        || cloned.source_role_preference != SourceRolePreference::TestIntentLast
        || !Arc::ptr_eq(&selected.planner_cache, &cloned.planner_cache)
        || !Arc::ptr_eq(&plain.planner_cache, &cloned.planner_cache)
    {
        return Err("discovery composition changed candidate cache or role policy".into());
    }
    let reset = cloned.with_indexed_context();
    if reset.discovery_preference != DiscoveryPackingPreference::Balanced
        || reset.source_role_preference != SourceRolePreference::Neutral
    {
        return Err("neutral builder did not reset both ordering preferences".into());
    }
    Ok(())
}

fn cached(
    server: &SyntaxMeshMcp,
) -> Result<Option<Arc<GenerationIdentifierIndex>>, Box<dyn Error>> {
    Ok(server
        .planner_cache
        .lock()
        .map_err(|error| std::io::Error::other(error.to_string()))?
        .cached_index())
}

fn request(seed_nodes: Option<Vec<String>>, generation: Option<String>) -> ContextArgs {
    ContextArgs {
        generation,
        query: "caller helper".to_owned(),
        seed_nodes,
        token_budget: 8192,
        max_hops: Some(0),
        max_candidates: Some(1),
    }
}

#[test]
fn absent_indexed_queries_return_empty_current_and_historical_packs() -> Result<(), Box<dyn Error>>
{
    let (_fixture, server, generation) = indexed_server()?;
    let server = server.with_indexed_context();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    for retained in [None, Some(generation.0.to_hex())] {
        let mut absent = request(None, retained);
        absent.query = "uniquelyabsentzzzz".to_owned();
        let result = content_json(&runtime.block_on(server.context(Parameters(absent)))?)?;
        if result
            .get("items")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|items| !items.is_empty())
        {
            return Err(std::io::Error::other("absent indexed query selected evidence").into());
        }
    }
    Ok(())
}

#[test]
fn opted_in_host_respects_small_bounds_and_shares_completed_cache() -> Result<(), Box<dyn Error>> {
    let (_fixture, plain, generation) = indexed_server()?;
    let server = plain.with_indexed_context();
    let clone = server.clone();
    if !Arc::ptr_eq(&server.planner_cache, &clone.planner_cache) || cached(&server)?.is_some() {
        return Err(std::io::Error::other("host cache was not initially empty/shared").into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let first = content_json(&runtime.block_on(server.context(Parameters(request(None, None))))?)?;
    let index = cached(&server)?.ok_or("indexed request did not retain cache")?;
    let second = content_json(&runtime.block_on(clone.context(Parameters(request(None, None))))?)?;
    let reused = cached(&clone)?.ok_or("clone lost retained index")?;
    if !Arc::ptr_eq(&index, &reused) || first != second {
        return Err(std::io::Error::other("repeat changed output or rebuilt index").into());
    }
    let historical = content_json(
        &runtime
            .block_on(server.context(Parameters(request(None, Some(generation.0.to_hex())))))?,
    )?;
    if historical.get("generation") != Some(&serde_json::to_value(generation)?)
        || historical
            .get("token_count")
            .and_then(serde_json::Value::as_u64)
            .is_none_or(|tokens| tokens > 8192)
    {
        return Err(
            std::io::Error::other("historical indexed response lost generation/budget").into(),
        );
    }
    let request = syntaxmesh_api_model::ContextRequest {
        query: "caller helper".to_owned(),
        seed_nodes: Vec::new(),
        token_budget: 8192,
        max_hops: 0,
        max_candidates: 1,
    };
    runtime.block_on(server.with_query(move |query| {
        let lexical = syntaxmesh_query::LexicalPlanRequest {
            query: &request.query,
            posting_budget: 8_000_000,
        };
        let mut seeds = query
            .lexical_seed_plan(&index, &lexical)
            .map_err(|error| error.to_string())?;
        seeds.truncate(usize::from(request.max_candidates));
        let initial = syntaxmesh_api_model::ContextRequest {
            seed_nodes: seeds,
            ..request
        };
        let selection = query
            .ranked_seeded_context_selection(&initial)
            .map_err(|error| error.to_string())?;
        if selection.nodes.len() > 1 || selection.nodes.iter().any(|entry| entry.1 != 0) {
            return Err("caller candidate/hop bounds were exceeded".to_owned());
        }
        Ok(())
    }))?;
    Ok(())
}

#[test]
fn explicit_seeds_preserve_default_output_and_do_not_build_cache() -> Result<(), Box<dyn Error>> {
    let (_fixture, plain, generation) = indexed_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let seed = runtime.block_on(plain.with_query(|query| {
        query
            .search("helper", 1)
            .map_err(|error| error.to_string())?
            .first()
            .map(|node| node.id.0.to_hex())
            .ok_or_else(|| "fixture seed missing".to_owned())
    }))?;
    for preference in [
        syntaxmesh_query::SourceRolePreference::Neutral,
        syntaxmesh_query::SourceRolePreference::TestIntentFirst,
        syntaxmesh_query::SourceRolePreference::TestIntentLast,
    ] {
        for discovery in [
            syntaxmesh_query::DiscoveryPackingPreference::Balanced,
            syntaxmesh_query::DiscoveryPackingPreference::LexicalFirst,
        ] {
            for source_content in [false, true] {
                let configured = plain
                    .clone()
                    .with_indexed_context_role_preference(preference)
                    .with_indexed_context_discovery_preference(discovery);
                let enabled = if source_content {
                    configured.with_source_content_context()
                } else {
                    configured
                };
                for retained in [None, Some(generation.0.to_hex())] {
                    let expected = content_json(&runtime.block_on(plain.context(Parameters(
                        request(Some(vec![seed.clone()]), retained.clone()),
                    )))?)?;
                    let args = request(Some(vec![seed.clone()]), retained);
                    let actual =
                        content_json(&runtime.block_on(enabled.context(Parameters(args)))?)?;
                    if actual != expected || cached(&enabled)?.is_some() {
                        return Err(std::io::Error::other(
                            "explicit seed request changed routing or built cache",
                        )
                        .into());
                    }
                }
            }
        }
    }
    Ok(())
}
