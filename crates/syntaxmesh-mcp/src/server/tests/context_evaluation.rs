//! Opt-in corpus evaluations, reusing the shared MCP fixtures and evidence paths.
use super::*;

fn target_name_matches(name: &str, target: &str, kind: &str) -> bool {
    if kind == "Function" {
        name == target
    } else {
        name.contains(target)
    }
}

#[test]
fn function_target_identity_excludes_prefixed_test_names() {
    assert!(target_name_matches(
        "apply_pending_local_migrations",
        "apply_pending_local_migrations",
        "Function"
    ));
    assert!(!target_name_matches(
        "apply_pending_local_migrations_is_idempotent",
        "apply_pending_local_migrations",
        "Function"
    ));
    assert!(target_name_matches(
        "Benchmark evidence and limitations",
        "Benchmark evidence",
        "Section"
    ));
}

#[test]
#[ignore = "manual retrieval evaluation; run with cargo make benchmark-context-retrieval"]
fn context_real_source_retrieval_evaluation() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut server, generation) = indexed_engine_source_server()?;
    let profile = std::sync::Arc::new(crate::server::context_profile::Profile::default());
    server.token_counter.profile = Some(std::sync::Arc::clone(&profile));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let cases = [
        (
            "lineage_publication",
            "publish prepared lineage",
            "::publish_prepared_with_lineage",
        ),
        (
            "consequence_publication",
            "publish prepared consequence evidence",
            "::publish_prepared_with_consequences",
        ),
        (
            "workflow_recovery",
            "recover pending workflow operations",
            "::recover_pending_workflows",
        ),
    ];
    let budgets = [2048_u64, 8192_u64];

    for (case_id, query, target_suffix) in cases {
        let search = runtime.block_on(server.search(Parameters(SearchArgs {
            text: target_suffix.trim_start_matches("::").to_owned(),
            limit: Some(100),
        })))?;
        let search_value = content_json(&search)?;
        let target_id = search_value
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.ends_with(target_suffix))
                        && node.get("kind").and_then(Value::as_str) == Some("Function")
                })
            })
            .and_then(|node| node.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                std::io::Error::other(format!(
                    "retrieval evaluation target {target_suffix} was not indexed"
                ))
            })?;
        let target_record =
            serde_json::to_value(SyntaxMeshMcp::<RustExtractor>::parse_node_id(target_id)?)?;

        report_lookup_diagnostics(&runtime, &server, case_id, query, target_id, 32)?;

        for budget in budgets {
            let before_profile = profile.snapshot();
            let started = std::time::Instant::now();
            let result = runtime.block_on(server.context(Parameters(ContextArgs {
                generation: None,
                query: query.to_owned(),
                seed_nodes: None,
                token_budget: budget,
                max_hops: Some(1),
                max_candidates: Some(32),
            })))?;
            let elapsed = started.elapsed();
            let after_profile = profile.snapshot();
            println!(
                "context_profile case={case_id} budget={budget} tokenizer_calls={} tokenizer_us={} query_us={}",
                after_profile.0.saturating_sub(before_profile.0),
                after_profile.1.saturating_sub(before_profile.1),
                elapsed.as_micros()
            );
            let value = content_json(&result)?;
            let items = value
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    std::io::Error::other("evaluation context omitted its evidence items")
                })?;
            let retrieved = items.iter().any(|item| {
                item.get("kind").and_then(Value::as_str) == Some("source_evidence")
                    && item
                        .get("node_ids")
                        .and_then(Value::as_array)
                        .is_some_and(|ids| ids.iter().any(|id| id == &target_record))
            });
            let token_count = value
                .get("token_count")
                .and_then(Value::as_u64)
                .ok_or_else(|| std::io::Error::other("evaluation omitted token_count"))?;
            let omitted = value.get("omitted").map_or(0, |omitted| {
                ["source_evidence", "signatures", "graph_paths", "summaries"]
                    .into_iter()
                    .filter_map(|kind| omitted.get(kind).and_then(Value::as_u64))
                    .sum::<u64>()
            });
            let retrieval_required =
                budget == 8192 || case_id == "documentation_benchmark_rationale";
            if value.get("generation") != Some(&serde_json::to_value(generation)?)
                || token_count > budget
                || (retrieval_required && !retrieved)
            {
                return Err(std::io::Error::other(format!(
                    "retrieval evaluation invariant failed for {case_id} at {budget} tokens: {value}"
                ))
                .into());
            }
            eprintln!(
                "context_eval case={case_id} target={target_suffix} budget={budget} retrieved={retrieved} tokens={token_count} items={} omitted={omitted} query_us={}",
                items.len(),
                elapsed.as_micros(),
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "manual sibling-repository retrieval evaluation; set SYNTAXMESH_CONTEXT_EVAL_ROOT"]
fn context_sibling_source_retrieval_evaluation() -> Result<(), Box<dyn Error>> {
    let mode =
        std::env::var("SYNTAXMESH_CONTEXT_EVAL_SCOPE").unwrap_or_else(|_| "selected".to_owned());
    let whole_repository = match mode.as_str() {
        "selected" => false,
        "repository" => true,
        _ => {
            return Err(std::io::Error::other(
                "SYNTAXMESH_CONTEXT_EVAL_SCOPE must be selected or repository",
            )
            .into());
        }
    };
    let source_root = std::env::var_os("SYNTAXMESH_CONTEXT_EVAL_ROOT")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("set SYNTAXMESH_CONTEXT_EVAL_ROOT to Shardline"))?;
    let source_root = fs::canonicalize(source_root)?;
    let corpus = source_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let cases = match corpus.as_str() {
        "shardline" => vec![
            (
                "rust_migrations",
                "apply pending local database migrations",
                "apply_pending_local_migrations",
                "crates/shardline-index/src/local_sqlite/helpers.rs",
                "Function",
            ),
            (
                "rust_migration_test",
                "test pending local migrations are idempotent",
                "tests::apply_pending_local_migrations_is_idempotent",
                "crates/shardline-index/src/local_sqlite/helpers.rs",
                "Function",
            ),
            (
                "rust_webhook",
                "apply provider webhook events",
                "apply_provider_webhook",
                "crates/shardline-server/src/provider_events.rs",
                "Function",
            ),
            (
                "python_metadata",
                "load package publish metadata",
                "load_metadata",
                "scripts/publish-order.py",
                "Function",
            ),
            (
                "bash_query",
                "query external DuckDB data",
                "external_query",
                "scripts/benchmark-duckdb-query.sh",
                "Function",
            ),
            (
                "documentation_benchmark",
                "benchmark evidence",
                "Benchmark evidence",
                "docs/benchmarks/README.md",
                "Section",
            ),
            (
                "documentation_benchmark_rationale",
                "why are Shardline benchmark numbers not product claims on other machines",
                "engineering evidence",
                "docs/benchmarks/README.md",
                "DocumentChunk",
            ),
        ],
        _ => {
            return Err(std::io::Error::other(format!(
                "unsupported evaluation corpus {corpus}; expected Shardline"
            ))
            .into());
        }
    };
    let indexed_host_probe = std::env::var_os("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE").is_some();
    let syntax_setting =
        std::env::var("SYNTAXMESH_CONTEXT_SYNTAX_POLICY").unwrap_or_else(|_| "strict".to_owned());
    let syntax_policy = match syntax_setting.as_str() {
        "strict" => syntaxmesh_engine::SourceSyntaxPolicy::Strict,
        "record-failures" => syntaxmesh_engine::SourceSyntaxPolicy::RecordFailures,
        _ => return Err("context syntax policy must be strict or record-failures".into()),
    };
    let source_content = std::env::var_os("SYNTAXMESH_CONTEXT_SOURCE_CONTENT_PROBE").is_some();
    let discovery_setting = std::env::var("SYNTAXMESH_CONTEXT_DISCOVERY_PREFERENCE")
        .unwrap_or_else(|_| "balanced".to_owned());
    let discovery_preference = match discovery_setting.as_str() {
        "balanced" => syntaxmesh_query::DiscoveryPackingPreference::Balanced,
        "lexical-first" if indexed_host_probe => {
            syntaxmesh_query::DiscoveryPackingPreference::LexicalFirst
        }
        _ => return Err("invalid discovery packing preference".into()),
    };
    if source_content && !indexed_host_probe {
        return Err("source-content probe requires indexed host probe".into());
    }
    let role_setting = std::env::var("SYNTAXMESH_CONTEXT_ROLE_PREFERENCE")
        .unwrap_or_else(|_| "neutral".to_owned());
    let role_preference = match role_setting.as_str() {
        "neutral" => syntaxmesh_query::SourceRolePreference::Neutral,
        "test-first" if indexed_host_probe => {
            syntaxmesh_query::SourceRolePreference::TestIntentFirst
        }
        "test-last" if indexed_host_probe => syntaxmesh_query::SourceRolePreference::TestIntentLast,
        _ => {
            return Err(std::io::Error::other(
                "role preference must be neutral, or test-first/test-last with indexed host probe",
            )
            .into());
        }
    };
    if indexed_host_probe
        && [
            "SYNTAXMESH_CONTEXT_CORPUS_PROBE",
            "SYNTAXMESH_CONTEXT_IDENTIFIER_INDEX_PROBE",
            "SYNTAXMESH_CONTEXT_GRAPH_SEED_PROBE",
            "SYNTAXMESH_CONTEXT_FAMILY_SEED_PROBE",
            "SYNTAXMESH_CONTEXT_PATH_IDENTIFIER_INDEX_PROBE",
            "SYNTAXMESH_CONTEXT_MIXED_SEED_PROBE",
            "SYNTAXMESH_CONTEXT_DEFINITION_SEED_PROBE",
            "SYNTAXMESH_CONTEXT_ALL_FACT_CHANNEL_SEED_PROBE",
            "SYNTAXMESH_CONTEXT_TARGET_SEED_PROBE",
        ]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return Err(
            std::io::Error::other("indexed host probe must run without diagnostic probes").into(),
        );
    }
    let mut selected_files = Vec::new();
    for case in &cases {
        if !selected_files.contains(&case.3) {
            selected_files.push(case.3);
        }
    }
    let (_fixture, server, generation, input_fingerprint, indexed_files) =
        indexed_external_source_server(
            &source_root,
            &selected_files,
            whole_repository,
            syntax_policy,
        )?;
    let server = if indexed_host_probe {
        server.with_indexed_context_role_preference(role_preference)
    } else {
        server
    };
    let server = if source_content {
        server.with_source_content_context()
    } else {
        server
    };
    let server = if std::env::var_os("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE").is_some() {
        server.with_indexed_context_discovery_preference(discovery_preference)
    } else {
        server
    };
    eprintln!(
        "context_processing_policy syntax_policy={syntax_setting} source_content={source_content} discovery={discovery_setting}"
    );
    eprintln!(
        "context_host_policy role_preference={role_setting} mode={}",
        if indexed_host_probe {
            "indexed_opt_in"
        } else {
            "default"
        }
    );
    eprintln!(
        "context_fixture corpus={corpus} files={} input_fingerprint_blake3={input_fingerprint}",
        indexed_files
    );
    eprintln!(
        "context_scope scope={mode} target_files={}",
        selected_files.len()
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let budgets = [2048_u64, 8192_u64];

    let path_probe = std::env::var_os("SYNTAXMESH_CONTEXT_PATH_IDENTIFIER_INDEX_PROBE").is_some();
    let mixed_probe = std::env::var_os("SYNTAXMESH_CONTEXT_MIXED_SEED_PROBE").is_some();
    let family_probe = std::env::var_os("SYNTAXMESH_CONTEXT_FAMILY_SEED_PROBE").is_some();
    if family_probe
        && (mixed_probe
            || path_probe
            || std::env::var_os("SYNTAXMESH_CONTEXT_CORPUS_PROBE").is_none()
            || std::env::var_os("SYNTAXMESH_CONTEXT_IDENTIFIER_INDEX_PROBE").is_none()
            || std::env::var_os("SYNTAXMESH_CONTEXT_GRAPH_SEED_PROBE").is_none()
            || std::env::var_os("SYNTAXMESH_CONTEXT_DEFINITION_SEED_PROBE").is_some()
            || std::env::var_os("SYNTAXMESH_CONTEXT_ALL_FACT_CHANNEL_SEED_PROBE").is_some())
    {
        return Err(std::io::Error::other(
            "family seed probe requires corpus/index/graph probes without competing seed policies",
        )
        .into());
    }
    if mixed_probe
        && (!path_probe || std::env::var_os("SYNTAXMESH_CONTEXT_GRAPH_SEED_PROBE").is_none())
    {
        return Err(
            std::io::Error::other("mixed seed probe requires path and graph probes").into(),
        );
    }
    if path_probe
        && (std::env::var_os("SYNTAXMESH_CONTEXT_CORPUS_PROBE").is_none()
            || std::env::var_os("SYNTAXMESH_CONTEXT_IDENTIFIER_INDEX_PROBE").is_none()
            || std::env::var_os("SYNTAXMESH_CONTEXT_DEFINITION_SEED_PROBE").is_some()
            || std::env::var_os("SYNTAXMESH_CONTEXT_ALL_FACT_CHANNEL_SEED_PROBE").is_some())
    {
        return Err(std::io::Error::other(
            "path probe requires corpus and identifier probes, without other channel seed policies",
        )
        .into());
    }
    let path_index = if path_probe {
        let started = std::time::Instant::now();
        let index = runtime.block_on(server.with_query(|service| {
            service
                .build_path_identifier_index(
                    1_000_000,
                    8_000_000,
                    256 * 1024 * 1024,
                    100_000,
                    32 * 1024 * 1024,
                )
                .map(std::sync::Arc::new)
                .map_err(|error| error.to_string())
        }))?;
        eprintln!(
            "context_path_identifier_index_build elapsed_us={}",
            started.elapsed().as_micros()
        );
        Some(index)
    } else {
        None
    };

    let identifier_index = if std::env::var_os("SYNTAXMESH_CONTEXT_CORPUS_PROBE").is_some()
        && std::env::var_os("SYNTAXMESH_CONTEXT_IDENTIFIER_INDEX_PROBE").is_some()
    {
        let started = std::time::Instant::now();
        let index = runtime.block_on(server.with_query(move |service| {
            let built = if family_probe {
                service.build_planner_identifier_index(1_000_000, 8_000_000, 256 * 1024 * 1024)
            } else {
                service.build_identifier_index(1_000_000, 8_000_000, 256 * 1024 * 1024)
            };
            built
                .map(std::sync::Arc::new)
                .map_err(|error| error.to_string())
        }))?;
        eprintln!(
            "context_identifier_index_build elapsed_us={}",
            started.elapsed().as_micros()
        );
        Some(index)
    } else {
        None
    };
    // This fixture cannot publish while the diagnostic runs. Retain one corpus
    // for its pinned generation instead of repeating the same backend scans.
    let probe_corpus = if std::env::var_os("SYNTAXMESH_CONTEXT_CORPUS_PROBE").is_some() {
        Some(runtime.block_on(server.with_query(|service| {
            let nodes = service
                .search("", usize::try_from(i64::MAX).unwrap_or(usize::MAX))
                .map_err(|error| error.to_string())?;
            let mut files = Vec::new();
            let mut after_file = None;
            loop {
                let page = service
                    .historical_files_page(after_file, 1000)
                    .map_err(|error| error.to_string())?;
                if page.items.len() > 1000 || (page.has_more && page.items.is_empty()) {
                    return Err("invalid diagnostic file inventory page".to_owned());
                }
                for file in page.items {
                    if after_file.is_some_and(|previous| file.file_id <= previous) {
                        return Err("nonadvancing diagnostic file inventory".to_owned());
                    }
                    after_file = Some(file.file_id);
                    files.push(file);
                }
                if !page.has_more {
                    break;
                }
            }
            let field_nodes = corpus_probe::with_file_paths(&nodes, &files);
            Ok(std::sync::Arc::new((nodes, field_nodes, files)))
        }))?)
    } else {
        None
    };
    let mut missed_required = Vec::new();
    for (case_id, query, target_name, target_path, target_kind) in cases {
        let expected_file =
            serde_json::to_value(syntaxmesh_core::FileId::derive(&[target_path.as_bytes()]))?;
        let search = runtime.block_on(server.search(Parameters(SearchArgs {
            text: target_name.to_owned(),
            limit: Some(100),
        })))?;
        let search_value = content_json(&search)?;
        let target_id = search_value
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| target_name_matches(name, target_name, target_kind))
                        && node.get("kind").and_then(Value::as_str) == Some(target_kind)
                        && node.pointer("/source/file_id") == Some(&expected_file)
                })
            })
            .and_then(|node| node.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                std::io::Error::other(format!(
                    "retrieval evaluation target {target_name:?} ({target_kind}) was not indexed in {corpus}: {search_value}"
                ))
            })?;
        let target_record =
            serde_json::to_value(SyntaxMeshMcp::<RustExtractor>::parse_node_id(target_id)?)?;

        report_lookup_diagnostics(&runtime, &server, case_id, query, target_id, 64)?;
        if let Some(pinned_corpus) = probe_corpus.clone() {
            let target = SyntaxMeshMcp::<RustExtractor>::parse_node_id(target_id)?;
            let probe_query = query.to_owned();
            let probe_index = identifier_index.clone();
            let probe_path_index = path_index.clone();
            let result = runtime.block_on(server.with_query(move |service| {
                let (nodes, field_nodes, files) = pinned_corpus.as_ref();
                let mut comparison = corpus_probe::rank(nodes, &probe_query, target);
                comparison
                    .as_object_mut()
                    .ok_or("invalid probe object")?
                    .insert(
                        "label_and_source_path".to_owned(),
                        corpus_probe::stemmed_rank(field_nodes, &probe_query, target),
                    );
                comparison
                    .as_object_mut()
                    .ok_or("invalid probe object")?
                    .insert(
                        "stemmed".to_owned(),
                        corpus_probe::stemmed_rank(nodes, &probe_query, target),
                    );
                let reference_started = std::time::Instant::now();
                let ranked = corpus_probe::label_candidates(nodes, &probe_query)?;
                let reference_elapsed_us = reference_started.elapsed().as_micros();
                if let Some(index) = probe_index {
                    let index_started = std::time::Instant::now();
                    let indexed = service
                        .identifier_candidates(&index, &probe_query, 8_000_000, 256)
                        .map_err(|error| error.to_string())?;
                    let index_elapsed_us = index_started.elapsed().as_micros();
                    if indexed != ranked {
                        return Err("identifier index differs from the complete reference".to_owned());
                    }
                    let seed_candidates = if let Some(selected_path_index) = probe_path_index {
                        let expected = corpus_probe::path_candidates(nodes, files, &probe_query)?;
                        let started = std::time::Instant::now();
                        let path_ranked = service.identifier_candidates(&selected_path_index, &probe_query, 8_000_000, 256)
                            .map_err(|error| error.to_string())?;
                        let elapsed = started.elapsed().as_micros();
                        if path_ranked != expected { return Err("path index differs from complete canonical oracle".to_owned()); }
                        comparison.as_object_mut().ok_or("invalid probe object")?.insert(
                            "path_identifier_index".to_owned(), serde_json::json!({"equivalent": true, "query_elapsed_us": elapsed,
                                "target_rank": path_ranked.iter().position(|candidate| candidate.node.id == target).map(|position| position + 1)}));
                        path_ranked
                    } else { indexed };
                    if std::env::var_os("SYNTAXMESH_CONTEXT_GRAPH_SEED_PROBE").is_some() {
                        let definition_probe = std::env::var_os("SYNTAXMESH_CONTEXT_DEFINITION_SEED_PROBE").is_some();
                        let all_fact_probe = std::env::var_os("SYNTAXMESH_CONTEXT_ALL_FACT_CHANNEL_SEED_PROBE").is_some();
                        if definition_probe && all_fact_probe {
                            return Err("choose one diagnostic channel seed policy".to_owned());
                        }
                        let mut seeds = if family_probe {
                            let request = syntaxmesh_query::LexicalPlanRequest { query: &probe_query, posting_budget: 8_000_000 };
                            let started = std::time::Instant::now();
                            let plan = service.lexical_seed_plan(&index, &request).map_err(|error| error.to_string())?;
                            let elapsed = started.elapsed().as_micros();
                            let oracle_started = std::time::Instant::now();
                            let expected = corpus_probe::family_channel_seeds(nodes, &probe_query);
                            let oracle_elapsed = oracle_started.elapsed().as_micros();
                            if plan != expected { return Err("indexed family seeds differ from canonical oracle".to_owned()); }
                            comparison.as_object_mut().ok_or("invalid probe object")?.insert(
                                "indexed_family_seeds".to_owned(), serde_json::json!({"equivalent": true, "query_elapsed_us": elapsed, "oracle_elapsed_us": oracle_elapsed}));
                            plan
                        } else if mixed_probe {
                            let path_ids = seed_candidates.iter().map(|candidate| candidate.node.id).collect::<Vec<_>>();
                            corpus_probe::mixed_channel_seeds(nodes, &probe_query, &path_ids)
                        } else if all_fact_probe {
                            corpus_probe::all_fact_channel_seeds(nodes, &probe_query, 8)
                        } else if definition_probe {
                            corpus_probe::definition_channel_seeds(nodes, &probe_query, 8)
                        } else {
                            seed_candidates.iter().take(8).map(|candidate| candidate.node.id).collect::<Vec<_>>()
                        };
                        if family_probe {
                            let initial = syntaxmesh_api_model::ContextRequest {
                                query: probe_query.clone(), seed_nodes: seeds,
                                token_budget: 8192, max_hops: 2, max_candidates: 64,
                            };
                            let selected = service.ranked_seeded_context_selection(&initial)
                                .map_err(|error| error.to_string())?;
                            comparison.as_object_mut().ok_or("invalid probe object")?.insert(
                                "graph_refinement".to_owned(),
                                serde_json::json!({"initial_seed_ids": initial.seed_nodes.iter().map(|id| id.0.to_hex()).collect::<Vec<_>>(),
                                    "selected_nodes": selected.nodes.len(), "target_depth": selected.nodes.iter().find(|entry| entry.0 == target).map(|entry| entry.1)}),
                            );
                            let eligible = selected.nodes.iter().map(|entry| entry.0).collect::<std::collections::BTreeSet<_>>();
                            let original = initial.seed_nodes.iter().copied().collect::<std::collections::BTreeSet<_>>();
                            let request = syntaxmesh_query::LexicalPlanRequest { query: &probe_query, posting_budget: 8_000_000 };
                            let started = std::time::Instant::now();
                            seeds = service.lexical_packing_plan(&index, &request, &eligible, &original).map_err(|error| error.to_string())?;
                            let elapsed = started.elapsed().as_micros();
                            let oracle_started = std::time::Instant::now();
                            let expected = corpus_probe::discovery_balanced_plan(nodes, &probe_query, &eligible, &original)?;
                            let oracle_elapsed = oracle_started.elapsed().as_micros();
                            if seeds != expected { return Err("indexed family packing differs from canonical oracle".to_owned()); }
                            comparison.as_object_mut().ok_or("invalid probe object")?.insert(
                                "indexed_family_packing".to_owned(), serde_json::json!({"equivalent": true, "query_elapsed_us": elapsed, "oracle_elapsed_us": oracle_elapsed}));
                            comparison.as_object_mut().ok_or("invalid probe object")?.insert(
                                "packing_target_rank".to_owned(),
                                serde_json::json!(seeds.iter().position(|id| *id == target).map(|position| position.saturating_add(1))),
                            );
                        }
                        let graph_started = std::time::Instant::now();
                        let preview = if family_probe {
                            (seeds.contains(&target).then_some(0_usize), seeds.len(), 0, 0, false)
                        } else {
                            let neighborhood = service
                                .historical_neighborhood(&seeds, 2, 64, 256, 1024, 1024 * 1024)
                                .map_err(|error| error.to_string())?;
                            (neighborhood.nodes.iter().find(|entry| entry.id == target).map(|entry| entry.depth),
                                neighborhood.nodes.len(), neighborhood.edges.len(), neighborhood.scanned_incidence_entries, neighborhood.truncated)
                        };
                        comparison.as_object_mut().ok_or("invalid probe object")?.insert(
                            "graph_seed_expansion".to_owned(),
                            serde_json::json!({"seed_policy": if family_probe { "indexed_global_family_primary_first_discovery_balanced_plan" } else if mixed_probe { "mixed_all_fact_definition_path_2_4_2" } else if all_fact_probe { "exact_and_stemmed_all_facts" } else if definition_probe { "exact_and_stemmed_definitions" } else if path_probe { "exact_label_and_source_path" } else { "exact_all_facts" }, "seed_count": seeds.len(), "seed_ids": seeds.iter().map(|id| id.0.to_hex()).collect::<Vec<_>>(), "target_depth": preview.0, "nodes": preview.1, "edges": preview.2, "scanned_incidence_entries": preview.3, "truncated": preview.4, "elapsed_us": graph_started.elapsed().as_micros()}),
                        );
                    }
                    comparison.as_object_mut().ok_or("invalid probe object")?.insert(
                        "identifier_index".to_owned(),
                        {
                            let manifest_started = std::time::Instant::now();
                            service.manifest().map_err(|error| error.to_string())?;
                            let manifest_elapsed_us = manifest_started.elapsed().as_micros();
                            let hydration_started = std::time::Instant::now();
                            for candidate in &ranked {
                                if service.historical_node(candidate.node.id).map_err(|error| error.to_string())?.as_ref() != Some(&candidate.node) {
                                    return Err("diagnostic hydration differs from reference payload".to_owned());
                                }
                            }
                            let hydration_elapsed_us = hydration_started.elapsed().as_micros();
                            serde_json::json!({"equivalent": true, "reference_mode": "complete_pinned_corpus", "reference_elapsed_us": reference_elapsed_us, "query_elapsed_us": index_elapsed_us, "manifest_elapsed_us": manifest_elapsed_us, "hydration_elapsed_us": hydration_elapsed_us, "hydrated_nodes": ranked.len()})
                        },
                    );
                }
                let object = comparison.as_object_mut().ok_or("invalid probe object")?;
                object.insert(
                    "normalized_reference_rank".to_owned(),
                    serde_json::json!(
                        ranked
                            .iter()
                            .position(|candidate| candidate.node.id == target)
                            .map(|position| position.saturating_add(1))
                    ),
                );
                object.insert(
                    "normalized_reference_top".to_owned(),
                    serde_json::json!(
                        ranked
                            .iter()
                            .take(5)
                            .map(|candidate| (
                                candidate.node.name.chars().take(240).collect::<String>(),
                                candidate.score
                            ))
                            .collect::<Vec<_>>()
                    ),
                );
                Ok(comparison)
            }))?;
            eprintln!("context_corpus_probe case={case_id} result={result}");
            if let Some(seed_values) = result.pointer("/graph_seed_expansion/seed_ids") {
                let seed_nodes: Vec<String> = serde_json::from_value(seed_values.clone())?;
                let selection_request = syntaxmesh_api_model::ContextRequest {
                    query: query.to_owned(),
                    seed_nodes: seed_nodes
                        .iter()
                        .map(|id| SyntaxMeshMcp::<RustExtractor>::parse_node_id(id))
                        .collect::<Result<Vec<_>, _>>()?,
                    token_budget: 8192,
                    max_hops: if family_probe { 0 } else { 2 },
                    max_candidates: 64,
                };
                let selection = runtime.block_on(server.with_query(move |service| {
                    let selection_result = if family_probe {
                        service.ranked_plan_context_selection(&selection_request)
                    } else {
                        service.seeded_context_selection(&selection_request)
                    };
                    selection_result.map_err(|error| error.to_string())
                }))?;
                let target_node = SyntaxMeshMcp::<RustExtractor>::parse_node_id(target_id)?;
                eprintln!(
                    "context_graph_seed_selection case={case_id} selected_nodes={} target_depth={:?}",
                    selection.nodes.len(),
                    selection
                        .nodes
                        .iter()
                        .find(|entry| entry.0 == target_node)
                        .map(|entry| entry.1)
                );
                for budget in budgets {
                    let graph_request = syntaxmesh_api_model::ContextRequest {
                        query: query.to_owned(),
                        seed_nodes: seed_nodes
                            .iter()
                            .map(|id| SyntaxMeshMcp::<RustExtractor>::parse_node_id(id))
                            .collect::<Result<Vec<_>, _>>()?,
                        token_budget: budget,
                        max_hops: if family_probe { 0 } else { 2 },
                        max_candidates: 64,
                    };
                    let reader = server.source_reader.clone();
                    let mut counter = server.token_counter.clone();
                    counter.inner = counter.inner.for_request();
                    let graph_value = runtime.block_on(server.with_query(move |service| {
                        let packing_result = if family_probe {
                            service.ranked_plan_context(&graph_request, &reader, &counter)
                        } else {
                            service.seeded_context(&graph_request, &reader, &counter)
                        };
                        let pack = packing_result.map_err(|error| error.to_string())?;
                        serde_json::to_value(pack).map_err(|error| error.to_string())
                    }))?;
                    if graph_value.get("generation") != Some(&serde_json::to_value(generation)?)
                        || graph_value
                            .get("token_count")
                            .and_then(Value::as_u64)
                            .is_none_or(|count| count > budget)
                    {
                        return Err(std::io::Error::other(
                            "graph seed context violated generation/token bounds",
                        )
                        .into());
                    }
                    let retrieved = graph_value
                        .get("items")
                        .and_then(Value::as_array)
                        .is_some_and(|items| {
                            items.iter().any(|item| {
                                item.get("kind").and_then(Value::as_str) == Some("source_evidence")
                                    && item
                                        .get("node_ids")
                                        .and_then(Value::as_array)
                                        .is_some_and(|ids| ids.contains(&target_record))
                            })
                        });
                    eprintln!(
                        "context_graph_seed_pack case={case_id} budget={budget} selection=explicit_only retrieved={retrieved} tokens={} packing_priority={}",
                        graph_value
                            .get("token_count")
                            .ok_or("missing graph context token count")?,
                        if family_probe {
                            "ranked_plan"
                        } else {
                            "unordered"
                        }
                    );
                    let target_kinds = graph_value
                        .get("items")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter(|item| {
                            item.get("node_ids")
                                .and_then(Value::as_array)
                                .is_some_and(|ids| ids.contains(&target_record))
                        })
                        .filter_map(|item| item.get("kind").and_then(Value::as_str))
                        .collect::<Vec<_>>();
                    let warning_codes = graph_value
                        .get("warnings")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|warning| warning.get("code").and_then(Value::as_str))
                        .collect::<Vec<_>>();
                    eprintln!(
                        "context_graph_seed_items case={case_id} budget={budget} target_kinds={target_kinds:?} warning_codes={warning_codes:?}"
                    );
                    let packed_spans = graph_value
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|item| {
                        serde_json::json!({
                            "kind": item.get("kind"),
                            "path": item.get("source_path"),
                            "line_start": item.get("line_start"),
                            "line_end": item.get("line_end"),
                            "text_bytes": item.get("text").and_then(Value::as_str).map(str::len),
                            "node_ids": item.get("node_ids"),
                        })
                    })
                    .collect::<Vec<_>>();
                    eprintln!(
                        "context_graph_seed_packing case={case_id} budget={budget} spans={} omitted={}",
                        serde_json::to_string(&packed_spans)?,
                        graph_value
                            .get("omitted")
                            .ok_or("missing context omissions")?
                    );
                }
            }
        }
        if std::env::var_os("SYNTAXMESH_CONTEXT_TARGET_SEED_PROBE").is_some() {
            for budget in budgets {
                let seeded = runtime.block_on(server.context(Parameters(ContextArgs {
                    generation: None,
                    query: query.to_owned(),
                    seed_nodes: Some(vec![target_id.to_owned()]),
                    token_budget: budget,
                    max_hops: Some(1),
                    max_candidates: Some(64),
                })))?;
                let value = content_json(&seeded)?;
                let tokens = value
                    .get("token_count")
                    .and_then(Value::as_u64)
                    .ok_or("seed probe omitted token count")?;
                if tokens > budget
                    || value.get("generation") != Some(&serde_json::to_value(generation)?)
                {
                    return Err(std::io::Error::other(
                        "seed probe violated generation/token bounds",
                    )
                    .into());
                }
                let retrieved = value
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or("seed probe omitted items")?
                    .iter()
                    .any(|item| {
                        item.get("kind").and_then(Value::as_str) == Some("source_evidence")
                            && item
                                .get("node_ids")
                                .and_then(Value::as_array)
                                .is_some_and(|ids| ids.iter().any(|id| id == &target_record))
                    });
                eprintln!(
                    "context_target_seed_probe corpus={corpus} case={case_id} budget={budget} retrieved={retrieved} tokens={tokens}"
                );
            }
        }
        for budget in budgets {
            let cached_before = if indexed_host_probe {
                server
                    .planner_cache
                    .lock()
                    .map_err(|error| error.to_string())?
                    .cached_index()
                    .is_some()
            } else {
                false
            };
            let started = std::time::Instant::now();
            let result = runtime.block_on(server.context(Parameters(ContextArgs {
                generation: None,
                query: query.to_owned(),
                seed_nodes: None,
                token_budget: budget,
                max_hops: Some(1),
                max_candidates: Some(64),
            })))?;
            let elapsed = started.elapsed();
            #[cfg(feature = "benchmark-instrumentation")]
            if indexed_host_probe && !cached_before {
                let built_index = server
                    .planner_cache
                    .lock()
                    .map_err(|error| error.to_string())?
                    .cached_index()
                    .ok_or("successful indexed host request did not retain its index")?;
                let metrics = built_index.build_metrics();
                eprintln!(
                    "context_index_build case={case_id} pages={} scans={} nodes={} postings={} payload_bytes={} total_us={} page_read_us={} processing_us={}",
                    metrics.node_pages,
                    metrics.node_scans,
                    metrics.nodes,
                    metrics.postings,
                    metrics.payload_bytes,
                    metrics.elapsed.as_micros(),
                    metrics.page_read_elapsed.as_micros(),
                    metrics.processing_elapsed.as_micros(),
                );
            }
            if indexed_host_probe {
                eprintln!(
                    "context_host_cache case={case_id} budget={budget} state={} query_us={}",
                    if cached_before { "retained" } else { "cold" },
                    elapsed.as_micros()
                );
                if budget == 2048 {
                    let inspected_index = server
                        .planner_cache
                        .lock()
                        .map_err(|error| error.to_string())?
                        .cached_index()
                        .ok_or("indexed request did not retain its index")?;
                    let expected_id = SyntaxMeshMcp::<RustExtractor>::parse_node_id(target_id)?;
                    let diagnostic = runtime.block_on(server.with_query(move |service| {
                        let seeds = service.lexical_seed_plan(&inspected_index,
                            &syntaxmesh_query::LexicalPlanRequest { query, posting_budget: 8_000_000 })
                            .map_err(|error| error.to_string())?;
                        let selection = service.ranked_seeded_context_selection(
                            &syntaxmesh_api_model::ContextRequest {
                                query: query.to_owned(), seed_nodes: seeds.clone(), token_budget: 2048,
                                max_hops: 1, max_candidates: 64,
                            }).map_err(|error| error.to_string())?;
                        let selected = selection.nodes.iter().map(|entry| entry.0)
                            .collect::<std::collections::BTreeSet<_>>();
                        let original = seeds.iter().filter(|id| selected.contains(id)).copied().collect();
                        let packing = service.lexical_packing_plan_with_discovery(
                            &inspected_index, &syntaxmesh_query::LexicalPlanRequest { query, posting_budget: 8_000_000 },
                            &syntaxmesh_query::SourceRolePackingRequest { selected: &selected, original: &original, preference: role_preference }, discovery_preference)
                            .map_err(|error| error.to_string())?;
                        Ok(serde_json::json!({"seed_position": seeds.iter().position(|id| *id == expected_id),
                            "seed_count": seeds.len(), "selected": selected.contains(&expected_id),
                            "packing_position": packing.iter().position(|id| *id == expected_id),
                            "selected_count": selected.len()}))
                    }))?;
                    eprintln!("context_target_candidates case={case_id} diagnostic={diagnostic}");
                }
            }
            let value = content_json(&result)?;
            let items = value
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    std::io::Error::other("evaluation context omitted its evidence items")
                })?;
            let retrieved = items.iter().any(|item| {
                item.get("kind").and_then(Value::as_str) == Some("source_evidence")
                    && item
                        .get("node_ids")
                        .and_then(Value::as_array)
                        .is_some_and(|ids| ids.iter().any(|id| id == &target_record))
            });
            let token_count = value
                .get("token_count")
                .and_then(Value::as_u64)
                .ok_or_else(|| std::io::Error::other("evaluation omitted token_count"))?;
            let omitted = value.get("omitted").map_or(0, |omitted| {
                ["source_evidence", "signatures", "graph_paths", "summaries"]
                    .into_iter()
                    .filter_map(|kind| omitted.get(kind).and_then(Value::as_u64))
                    .sum::<u64>()
            });
            if value.get("generation") != Some(&serde_json::to_value(generation)?)
                || token_count > budget
            {
                return Err(std::io::Error::other(format!(
                    "retrieval evaluation invariant failed for {case_id} at {budget} tokens: {value}"
                ))
                .into());
            }
            let target_label = target_name.replace(' ', "_");
            if budget == 8192 && !retrieved {
                missed_required.push(case_id);
            }
            if !retrieved {
                eprintln!(
                    "context_eval_miss corpus={corpus} case={case_id} budget={budget} target={target_name:?} items={}",
                    serde_json::to_string(items)?
                );
            }
            eprintln!(
                "context_eval corpus={corpus} case={case_id} target={target_label} budget={budget} retrieved={retrieved} tokens={token_count} items={} omitted={omitted} query_us={}",
                items.len(),
                elapsed.as_micros(),
            );
        }
    }
    if !missed_required.is_empty() {
        return Err(std::io::Error::other(format!(
            "required retrieval targets missed: {missed_required:?}"
        ))
        .into());
    }
    Ok(())
}
