use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ContentBlock;
use serde_json::Value;
use syntaxmesh_core::{GenerationId, IndexRunId, RepositoryId, WorktreeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_query::ContextSourceProvider;
use syntaxmesh_scanner::{SUPPORTED_SOURCE_EXTENSIONS, scan};
use syntaxmesh_store_turso::TursoGraphStore;

use super::{ContextArgs, NodeArgs, SearchArgs, SyntaxMeshMcp};
use syntaxmesh_core::FileVersion;

#[path = "corpus_probe/tests.rs"]
mod corpus_probe;

#[path = "indexed_context/tests.rs"]
mod indexed_context_tests;

pub(super) struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new() -> Result<Self, std::io::Error> {
        let root = tempfile::Builder::new()
            .prefix("syntaxmesh-mcp-fixture-")
            .tempdir()?;
        fs::create_dir_all(root.path().join("src"))?;
        fs::write(
            root.path().join("src/lib.rs"),
            "pub fn caller() { helper(); }\npub fn helper() {}\npub fn caller_again() {}\n",
        )?;
        Ok(Self(root))
    }
}

fn content_json(result: &rmcp::model::CallToolResult) -> Result<Value, Box<dyn Error>> {
    let Some(ContentBlock::Text(content)) = result.content.first() else {
        return Err(std::io::Error::other("tool returned no text content").into());
    };
    Ok(serde_json::from_str(&content.text)?)
}

#[test]
fn fixture_directories_are_unique_and_cleaned_by_their_owner() -> Result<(), std::io::Error> {
    let first = Fixture::new()?;
    let second = Fixture::new()?;
    let first_path = first.0.path().to_owned();
    if first_path == second.0.path() || !first_path.join("src/lib.rs").is_file() {
        return Err(std::io::Error::other("fixture allocation was not unique"));
    }
    drop(first);
    if first_path.exists() || !second.0.path().join("src/lib.rs").is_file() {
        return Err(std::io::Error::other(
            "fixture cleanup crossed ownership boundaries",
        ));
    }
    Ok(())
}

pub(super) fn indexed_server() -> Result<(Fixture, SyntaxMeshMcp, GenerationId), Box<dyn Error>> {
    indexed_server_with_source(None)
}

pub(super) fn indexed_server_with_source(
    source: Option<&str>,
) -> Result<(Fixture, SyntaxMeshMcp, GenerationId), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    if let Some(source) = source {
        fs::write(fixture.0.path().join("src/lib.rs"), source)?;
    }
    let root = fs::canonicalize(fixture.0.path())?;
    let database = root.join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let scan = scan(&root, &["rs"])?;
    let generation = GenerationId::derive(&[b"mcp-test-generation"]);
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"mcp-test-repository"]),
        WorktreeId::derive(&[b"mcp-test-worktree"]),
    );
    engine.index(
        &scan.files,
        IndexRunId::derive(&[b"mcp-test-run"]),
        generation,
    )?;
    drop(engine);
    Ok((
        fixture,
        SyntaxMeshMcp::open(database, &root, "cl100k_base")?,
        generation,
    ))
}

fn indexed_engine_source_server() -> Result<(Fixture, SyntaxMeshMcp, GenerationId), Box<dyn Error>>
{
    let fixture = Fixture::new()?;
    let root = fs::canonicalize(fixture.0.path())?;
    let engine_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../syntaxmesh-engine/src");
    fs::copy(engine_source.join("engine.rs"), root.join("src/engine.rs"))?;
    fs::copy(engine_source.join("lib.rs"), root.join("src/lib.rs"))?;
    let database = root.join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let scan = scan(&root, &["rs"])?;
    let generation = GenerationId::derive(&[b"mcp-engine-source-generation"]);
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"mcp-engine-source-repository"]),
        WorktreeId::derive(&[b"mcp-engine-source-worktree"]),
    );
    engine.index(
        &scan.files,
        IndexRunId::derive(&[b"mcp-engine-source-run"]),
        generation,
    )?;
    drop(engine);
    Ok((
        fixture,
        SyntaxMeshMcp::open(database, &root, "o200k_base")?,
        generation,
    ))
}

type ExternalSourceFixture = (Fixture, SyntaxMeshMcp, GenerationId, String, usize);

fn report_lookup_diagnostics(
    runtime: &tokio::runtime::Runtime,
    server: &SyntaxMeshMcp,
    case: &str,
    query: &str,
    target_id: &str,
    max_candidates: usize,
) -> Result<(), Box<dyn Error>> {
    if std::env::var_os("SYNTAXMESH_CONTEXT_LOOKUP_DIAGNOSTICS").is_none() {
        return Ok(());
    }
    let target = SyntaxMeshMcp::<RustExtractor>::parse_node_id(target_id)?;
    let terms = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect::<std::collections::BTreeSet<_>>();
    let diagnostics = runtime.block_on(server.with_query(move |service| {
        let mut rows = Vec::new();
        for term in terms.iter().take(16) {
            for limit in [max_candidates.saturating_mul(4).min(1024), 1024] {
                let nodes = service.search(term, limit).map_err(|error| error.to_string())?;
                rows.push(serde_json::json!({"term": term, "limit": limit, "returned": nodes.len(), "target_present": nodes.iter().any(|node| node.id == target)}));
            }
        }
        Ok(rows)
    }))?;
    eprintln!(
        "context_lookup case={case} rows={}",
        serde_json::to_string(&diagnostics)?
    );
    Ok(())
}

fn indexed_external_source_server(
    source_root: &Path,
    selected_files: &[&str],
    whole_repository: bool,
    syntax_policy: syntaxmesh_engine::SourceSyntaxPolicy,
) -> Result<ExternalSourceFixture, Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let root = fs::canonicalize(fixture.0.path())?;
    let source_root = fs::canonicalize(source_root)?;
    let database = root.join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let scan = scan(&source_root, SUPPORTED_SOURCE_EXTENSIONS)?;
    let files = scan
        .files
        .into_iter()
        .filter(|file| {
            whole_repository || selected_files.contains(&file.file.normalized_path.as_str())
        })
        .collect::<Vec<_>>();
    if selected_files.iter().any(|selected| {
        !files
            .iter()
            .any(|file| file.file.normalized_path == *selected)
    }) {
        return Err(std::io::Error::other(format!(
            "evaluation selected {} of {} requested source files",
            files.len(),
            selected_files.len()
        ))
        .into());
    }
    let mut input_fingerprint = blake3::Hasher::new();
    for file in &files {
        input_fingerprint.update(file.file.normalized_path.as_bytes());
        input_fingerprint.update(&[0]);
        input_fingerprint.update(&file.file.content_hash);
        input_fingerprint.update(&file.file.size_bytes.to_le_bytes());
        input_fingerprint.update(&[0]);
    }
    let input_fingerprint = input_fingerprint.finalize().to_hex().to_string();
    eprintln!(
        "context_ingestion stage=prepared files={} input_fingerprint_blake3={input_fingerprint}",
        files.len()
    );
    let extractors = syntaxmesh_source_host::supported_source_extractors()?;
    let generation = GenerationId::derive(&[b"mcp-external-source-generation"]);
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        extractors,
        RepositoryId::derive(&[b"mcp-external-source-repository"]),
        WorktreeId::derive(&[b"mcp-external-source-worktree"]),
    );
    engine = engine.with_source_syntax_policy(syntax_policy);
    let indexing_started = std::time::Instant::now();
    engine.index(
        &files,
        IndexRunId::derive(&[b"mcp-external-source-run"]),
        generation,
    )?;
    eprintln!(
        "context_ingestion stage=published elapsed_ms={}",
        indexing_started.elapsed().as_millis()
    );
    if syntax_policy == syntaxmesh_engine::SourceSyntaxPolicy::RecordFailures {
        let coverage_started = std::time::Instant::now();
        let coverage = engine.source_processing_coverage_at(generation)?;
        eprintln!(
            "context_processing completed={} syntax_failed={} unclassified={}",
            coverage.completed.len(),
            coverage.syntax_failed.len(),
            coverage.unclassified.len()
        );
        eprintln!(
            "context_ingestion stage=coverage elapsed_ms={}",
            coverage_started.elapsed().as_millis()
        );
    }
    drop(engine);
    Ok((
        fixture,
        SyntaxMeshMcp::open(database, &source_root, "o200k_base")?,
        generation,
        input_fingerprint,
        files.len(),
    ))
}

#[test]
fn tool_discovery_exposes_only_read_tools() {
    let names = SyntaxMeshMcp::<RustExtractor>::tool_router()
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["context", "neighbors", "node", "path", "search", "status"]
    );
}

#[test]
fn context_tool_returns_pinned_token_bounded_evidence() -> Result<(), Box<dyn Error>> {
    let (_fixture, server, generation) = indexed_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(server.context(Parameters(ContextArgs {
        generation: None,
        query: "caller".to_owned(),
        seed_nodes: None,
        token_budget: 1024,
        max_hops: Some(1),
        max_candidates: Some(32),
    })))?;
    let value = content_json(&result)?;
    let expected_generation = serde_json::to_value(generation)?;
    if value.get("generation") != Some(&expected_generation)
        || value.get("tokenizer").and_then(Value::as_str)
            != Some("tiktoken-rs-0.12.1/cl100k_base/ordinary")
        || value
            .get("token_count")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX)
            > 1024
        || value
            .get("items")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    {
        return Err(std::io::Error::other(format!("unexpected context response: {value}")).into());
    }
    Ok(())
}

#[test]
fn context_retrieves_a_real_engine_symbol_with_exact_repository_evidence()
-> Result<(), Box<dyn Error>> {
    let (_fixture, server, generation) = indexed_engine_source_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let search = runtime.block_on(server.search(Parameters(SearchArgs {
        text: "publish_prepared_with_lineage".to_owned(),
        limit: Some(100),
    })))?;
    let search_value = content_json(&search)?;
    let target = search_value
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| {
            nodes.iter().find(|node| {
                node.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.ends_with("::publish_prepared_with_lineage"))
                    && node.get("kind").and_then(Value::as_str) == Some("Function")
            })
        })
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            std::io::Error::other(format!(
                "engine source fixture did not index its target function: {search_value}"
            ))
        })?;
    let target_id = SyntaxMeshMcp::<RustExtractor>::parse_node_id(target)?;
    let budget = 8192;
    let started = std::time::Instant::now();
    let result = runtime.block_on(server.context(Parameters(ContextArgs {
        generation: None,
        query: "publish prepared lineage".to_owned(),
        seed_nodes: None,
        token_budget: budget,
        max_hops: Some(1),
        max_candidates: Some(32),
    })))?;
    let elapsed = started.elapsed();
    let value = content_json(&result)?;
    let target_record = serde_json::to_value(target_id)?;
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| std::io::Error::other("context response omitted its evidence items"))?;
    let target_evidence = items.iter().find(|item| {
        item.get("kind").and_then(Value::as_str) == Some("source_evidence")
            && item
                .get("node_ids")
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|id| id == &target_record))
    });
    let target_evidence = target_evidence.ok_or_else(|| {
        std::io::Error::other("natural-language context query omitted the expected function")
    })?;
    let token_count = value
        .get("token_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| std::io::Error::other("context response omitted token_count"))?;
    if value.get("generation") != Some(&serde_json::to_value(generation)?)
        || token_count > budget
        || target_evidence.get("source_path").and_then(Value::as_str) != Some("src/engine.rs")
        || target_evidence
            .get("text")
            .and_then(Value::as_str)
            .is_none_or(|text| !text.contains("publish_prepared_with_lineage"))
        || target_evidence
            .get("line_start")
            .and_then(Value::as_u64)
            .is_none_or(|line| line == 0)
    {
        return Err(std::io::Error::other(format!(
            "real-source context evidence was stale or over budget: {value}"
        ))
        .into());
    }
    let omitted = value.get("omitted").map_or(0, |omitted| {
        ["source_evidence", "signatures", "graph_paths", "summaries"]
            .into_iter()
            .filter_map(|kind| omitted.get(kind).and_then(Value::as_u64))
            .sum::<u64>()
    });
    eprintln!(
        "context_eval corpus=syntaxmesh-engine files=2 target=publish_prepared_with_lineage tokenizer={} tokens={token_count}/{budget} items={} omitted={} query_us={}",
        value
            .get("tokenizer")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        items.len(),
        omitted,
        elapsed.as_micros(),
    );
    Ok(())
}

#[test]
fn cached_label_oracle_matches_store_reference() -> Result<(), Box<dyn Error>> {
    let (_fixture, server, _) = indexed_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(server.with_query(|service| {
        let nodes = service
            .search("", 1_000_000)
            .map_err(|error| error.to_string())?;
        for query in ["", "absent", "caller", "helper", "caller helper"] {
            let expected = service
                .reference_ranked_candidates(query, 1_000_000, 256)
                .map_err(|error| error.to_string())?;
            if corpus_probe::label_candidates(&nodes, query)? != expected {
                return Err(format!(
                    "cached label oracle differs from store reference for {query:?}"
                ));
            }
        }
        Ok(())
    }))?;
    Ok(())
}

mod context_evaluation;

#[test]
fn source_reader_rejects_paths_outside_the_configured_root() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let root = fs::canonicalize(fixture.0.path())?;
    let reader = super::RepositorySourceReader::open(root)?;
    let file = FileVersion {
        file_id: syntaxmesh_core::FileId::derive(&[b"outside"]),
        normalized_path: "../outside.rs".to_owned(),
        content_hash: [0; 32],
        size_bytes: 0,
    };
    if reader.read_source(&file).is_ok() {
        return Err(std::io::Error::other("parent traversal source path was accepted").into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn source_reader_rejects_symlink_escape_from_the_configured_root() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new()?;
    let root = fs::canonicalize(fixture.0.path())?;
    let external_root = root.with_extension("external");
    fs::create_dir_all(&external_root)?;
    let external_file = external_root.join("outside.rs");
    fs::write(&external_file, "outside source")?;
    let link = root.join("src/escape.rs");
    symlink(&external_file, &link)?;
    let reader = super::RepositorySourceReader::open(root)?;
    let file = FileVersion {
        file_id: syntaxmesh_core::FileId::derive(&[b"symlink-escape"]),
        normalized_path: "src/escape.rs".to_owned(),
        content_hash: [0; 32],
        size_bytes: 0,
    };
    let access = reader.read_source(&file);
    fs::remove_file(link)?;
    fs::remove_file(external_file)?;
    fs::remove_dir(external_root)?;
    if access.is_ok() {
        return Err(std::io::Error::other("symlink source escape was accepted").into());
    }
    Ok(())
}

#[test]
fn invalid_ids_and_limits_are_reported_as_invalid_parameters() -> Result<(), Box<dyn Error>> {
    let invalid_id = SyntaxMeshMcp::<RustExtractor>::parse_node_id("not-a-node-id")
        .err()
        .ok_or_else(|| std::io::Error::other("malformed node ID was accepted"))?;
    if invalid_id.code != rmcp::model::ErrorCode::INVALID_PARAMS {
        return Err(std::io::Error::other("malformed node ID was not an input error").into());
    }

    let (_fixture, server, _) = indexed_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let invalid_limit = runtime
        .block_on(server.search(Parameters(SearchArgs {
            text: "caller".to_owned(),
            limit: Some(0),
        })))
        .err()
        .ok_or_else(|| std::io::Error::other("zero search limit was accepted"))?;
    if invalid_limit.code != rmcp::model::ErrorCode::INVALID_PARAMS {
        return Err(std::io::Error::other("zero search limit was not an input error").into());
    }

    let malformed = runtime
        .block_on(server.node(Parameters(NodeArgs {
            id: "00".to_owned(),
        })))
        .err()
        .ok_or_else(|| std::io::Error::other("short node ID was accepted"))?;
    if malformed.code != rmcp::model::ErrorCode::INVALID_PARAMS {
        return Err(std::io::Error::other("short node ID was not an input error").into());
    }
    Ok(())
}

#[test]
fn search_is_bounded_and_reports_the_startup_generation() -> Result<(), Box<dyn Error>> {
    let (fixture, server, generation) = indexed_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(server.search(Parameters(SearchArgs {
        text: "caller".to_owned(),
        limit: Some(1),
    })))?;
    let value = content_json(&result)?;
    if value.get("generation") != Some(&Value::String(generation.0.to_hex()))
        || value.get("nodes").and_then(Value::as_array).map(Vec::len) != Some(1)
    {
        return Err(
            std::io::Error::other(format!("unexpected bounded search response: {value}")).into(),
        );
    }

    fs::write(
        fixture.0.path().join("src/lib.rs"),
        "pub fn replacement() {}\n",
    )?;
    let root = fs::canonicalize(fixture.0.path())?;
    let scan = scan(&root, &["rs"])?;
    let database = root.join("graph.db");
    let mut updater = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"mcp-test-repository"]),
        WorktreeId::derive(&[b"mcp-test-worktree"]),
    );
    updater.index(
        &scan.files,
        IndexRunId::derive(&[b"mcp-test-run-2"]),
        GenerationId::derive(&[b"mcp-test-generation-2"]),
    )?;
    drop(updater);
    let stale_status = runtime
        .block_on(server.status())
        .err()
        .ok_or_else(|| std::io::Error::other("MCP silently switched to a newer generation"))?;
    if !stale_status.message.contains("restart syntaxmesh-mcp") {
        return Err(std::io::Error::other(format!(
            "MCP did not explain how to recover from a superseded generation: {stale_status:?}"
        ))
        .into());
    }
    Ok(())
}

#[test]
fn missing_database_generation_fails_closed() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let database = fixture.0.path().join("missing.db");
    let root = fs::canonicalize(fixture.0.path())?;
    let error = SyntaxMeshMcp::open(database, root, "cl100k_base")
        .err()
        .ok_or_else(|| std::io::Error::other("missing MCP database unexpectedly opened"))?;
    if !error.to_string().contains("missing") {
        return Err(std::io::Error::other(format!("unexpected error: {error}")).into());
    }
    Ok(())
}

#[path = "helper_route/tests.rs"]
mod helper_route;
mod historical_context;
mod live;
