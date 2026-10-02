use std::error::Error;
use std::fs;

use rmcp::handler::server::wrapper::Parameters;
use syntaxmesh_api_model::{ContextItemKind, ContextPack, ContextRequest};
use syntaxmesh_core::{GenerationId, IndexRunId, RepositoryId, WorktreeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_query::{ContextTokenCounter, QueryError};
use syntaxmesh_scanner::scan;
use syntaxmesh_store_turso::TursoGraphStore;

use super::{ContextArgs, SyntaxMeshMcp, content_json, indexed_server};

fn arguments(generation: Option<GenerationId>, query: &str) -> ContextArgs {
    ContextArgs {
        generation: generation.map(|id| id.0.to_hex()),
        query: query.to_owned(),
        seed_nodes: None,
        token_budget: 8192,
        max_hops: Some(1),
        max_candidates: Some(32),
    }
}

fn context_pack(
    server: &SyntaxMeshMcp,
    runtime: &tokio::runtime::Runtime,
    generation: Option<GenerationId>,
    query: &str,
) -> Result<ContextPack, Box<dyn Error>> {
    let result = runtime.block_on(server.context(Parameters(arguments(generation, query))))?;
    Ok(serde_json::from_value(content_json(&result)?)?)
}

struct Counter;
impl ContextTokenCounter for Counter {
    fn tokenizer_id(&self) -> &str {
        "historical-context-gap-bytes-v1"
    }
    fn count_tokens(&self, text: &str) -> Result<u64, String> {
        u64::try_from(text.len()).map_err(|error| error.to_string())
    }
}

#[test]
fn historical_context_uses_explicit_temporal_reads_and_verified_source_bytes()
-> Result<(), Box<dyn Error>> {
    let (fixture, old_server, previous) = indexed_server()?;
    let root = fs::canonicalize(&fixture.0)?;
    let original = fs::read(root.join("src/lib.rs"))?;
    fs::write(root.join("src/lib.rs"), "pub fn replacement() {}\n")?;
    let database = root.join("graph.db");
    let latest = GenerationId::derive(&[b"historical-context-new-generation"]);
    let mut updater = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        RepositoryId::derive(&[b"mcp-test-repository"]),
        WorktreeId::derive(&[b"mcp-test-worktree"]),
    );
    updater.index(
        &scan(&root, &["rs"])?.files,
        IndexRunId::derive(&[b"historical-context-edit"]),
        latest,
    )?;
    let request = ContextRequest {
        query: "caller".to_owned(),
        seed_nodes: Vec::new(),
        token_budget: 8192,
        max_hops: 1,
        max_candidates: 32,
    };
    let reader = super::super::RepositorySourceReader::open(&root)?;
    let unsupported = updater.query(previous).context(&request, &reader, &Counter);
    if !matches!(
        unsupported,
        Err(QueryError::Store(error)) if format!("{error:?}").starts_with("StaleBase {")
    ) {
        return Err(std::io::Error::other(
            "ordinary context unexpectedly accepted a superseded generation",
        )
        .into());
    }
    let historical = updater
        .query(previous)
        .historical_context(&request, &reader, &Counter)?;
    if historical.generation != previous
        || historical
            .items
            .iter()
            .any(|item| item.kind == ContextItemKind::SourceEvidence)
        || !historical
            .warnings
            .iter()
            .any(|warning| warning.code == "stale_source")
        || !historical
            .items
            .iter()
            .any(|item| item.text.contains("caller"))
    {
        return Err(std::io::Error::other(
            "historical context mixed current graph or stale source bytes",
        )
        .into());
    }
    fs::write(root.join("src/lib.rs"), &original)?;
    let verified = updater
        .query(previous)
        .historical_context(&request, &reader, &Counter)?;
    if verified.generation != previous
        || !verified.items.iter().any(|item| {
            item.kind == ContextItemKind::SourceEvidence && item.text.contains("caller")
        })
        || verified.token_count > verified.token_budget
    {
        return Err(std::io::Error::other(
            "historical context failed to verify matching source bytes",
        )
        .into());
    }
    fs::write(root.join("src/lib.rs"), "pub fn replacement() {}\n")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let stale =
        runtime.block_on(old_server.context(Parameters(arguments(Some(previous), "caller"))));
    if !matches!(stale, Err(error) if error.message.contains("restart syntaxmesh-mcp")) {
        return Err(
            std::io::Error::other("historical selection bypassed stale-host protection").into(),
        );
    }
    drop(updater);
    drop(old_server);
    let server = SyntaxMeshMcp::open(&database, &root, "cl100k_base")?;
    let result = runtime.block_on(server.context(Parameters(ContextArgs {
        generation: None,
        query: "replacement".to_owned(),
        seed_nodes: None,
        token_budget: 8192,
        max_hops: Some(1),
        max_candidates: Some(32),
    })))?;
    let current = content_json(&result)?;
    if current.get("generation") != Some(&serde_json::to_value(latest)?)
        || !current
            .get("items")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("kind").and_then(serde_json::Value::as_str) == Some("source_evidence")
                        && item
                            .get("text")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|text| text.contains("replacement"))
                })
            })
    {
        return Err(
            std::io::Error::other("current context stopped returning verified source").into(),
        );
    }
    let old = context_pack(&server, &runtime, Some(previous), "caller")?;
    if old.generation != previous
        || old
            .items
            .iter()
            .any(|item| item.kind == ContextItemKind::SourceEvidence)
        || !old
            .warnings
            .iter()
            .any(|warning| warning.code == "stale_source")
        || !old.items.iter().any(|item| item.text.contains("caller"))
    {
        return Err(
            std::io::Error::other("MCP historical selection substituted current evidence").into(),
        );
    }
    fs::write(root.join("src/lib.rs"), original)?;
    let matching = context_pack(&server, &runtime, Some(previous), "caller")?;
    if matching.generation != previous
        || matching.token_count > matching.token_budget
        || !matching.items.iter().any(|item| {
            item.kind == ContextItemKind::SourceEvidence && item.text.contains("caller")
        })
        || matching
            .items
            .iter()
            .any(|item| item.evidence_class != Some(syntaxmesh_core::EvidenceClass::SourceFact))
    {
        return Err(
            std::io::Error::other("MCP historical source validation or provenance failed").into(),
        );
    }
    Ok(())
}

#[test]
fn historical_context_rejects_invalid_and_unknown_generation_ids() -> Result<(), Box<dyn Error>> {
    let (_fixture, server, current) = indexed_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let missing: ContextArgs =
        serde_json::from_value(serde_json::json!({"query":"caller", "token_budget":8192}))?;
    if missing.generation.is_some() {
        return Err(std::io::Error::other("missing generation changed default behavior").into());
    }
    let implicit = context_pack(&server, &runtime, None, "caller")?;
    let explicit = context_pack(&server, &runtime, Some(current), "caller")?;
    if implicit != explicit {
        return Err(
            std::io::Error::other("explicit current generation differs from default").into(),
        );
    }
    for value in ["", "ab", "not-hex"] {
        let mut args = arguments(None, "caller");
        args.generation = Some(value.to_owned());
        let result = runtime.block_on(server.context(Parameters(args)));
        if !matches!(result, Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS) {
            return Err(
                std::io::Error::other("invalid generation ID was not rejected as input").into(),
            );
        }
    }
    let unknown = GenerationId::derive(&[b"unknown-mcp-context-generation"]);
    if runtime
        .block_on(server.context(Parameters(arguments(Some(unknown), "caller"))))
        .is_ok()
    {
        return Err(std::io::Error::other("unknown generation silently used current state").into());
    }
    Ok(())
}

#[test]
fn historical_context_scope_guard_and_optional_schema_are_explicit() -> Result<(), Box<dyn Error>> {
    let (_fixture, server, generation) = indexed_server()?;
    let engine = server
        .engine
        .lock()
        .map_err(|_error| std::io::Error::other("fixture engine lock is poisoned"))?;
    let manifest = engine.query(generation).manifest()?;
    if SyntaxMeshMcp::<RustExtractor>::ensure_selected_scope(
        &manifest,
        server.repository,
        server.worktree,
    )
    .is_err()
        || SyntaxMeshMcp::<RustExtractor>::ensure_selected_scope(
            &manifest,
            RepositoryId::derive(&[b"other-context-repository"]),
            server.worktree,
        )
        .is_ok()
        || SyntaxMeshMcp::<RustExtractor>::ensure_selected_scope(
            &manifest,
            server.repository,
            WorktreeId::derive(&[b"other-context-worktree"]),
        )
        .is_ok()
    {
        return Err(std::io::Error::other("historical context scope guard is inconsistent").into());
    }
    let schema = serde_json::to_value(schemars::schema_for!(ContextArgs))?;
    if schema
        .get("properties")
        .and_then(|properties| properties.get("generation"))
        .is_none()
        || schema
            .get("required")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|fields| {
                fields
                    .iter()
                    .any(|field| field.as_str() == Some("generation"))
            })
    {
        return Err(std::io::Error::other(
            "generation selection is missing or required in tool schema",
        )
        .into());
    }
    Ok(())
}
