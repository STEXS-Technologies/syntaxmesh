use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use syntaxmesh_core::{GenerationId, IndexRunId, RepositoryId, WorktreeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_scanner::scan;
use syntaxmesh_store_turso::TursoGraphStore;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[test]
fn malformed_launch_options_fail_before_storage_access() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let database = temporary.path().join("must-not-create.db");
    for (tokenizer, flags) in [
        ("invalid", vec![]),
        ("cl100k_base", vec!["--unknown"]),
        (
            "cl100k_base",
            vec!["--source-content-context", "--source-content-context"],
        ),
        (
            "cl100k_base",
            vec!["--lexical-first-context", "--lexical-first-context"],
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh-mcp"))
            .arg(&database)
            .arg(temporary.path())
            .arg(tokenizer)
            .args(flags)
            .output()?;
        if output.status.success() || !output.stdout.is_empty() || database.exists() {
            return Err("invalid MCP launch emitted protocol output or touched storage".into());
        }
    }
    Ok(())
}

struct Fixture(PathBuf);

impl Fixture {
    fn indexed_database() -> Result<(Self, PathBuf, GenerationId, GenerationId), Box<dyn Error>> {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "syntaxmesh-mcp-stdio-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src"))?;
        fs::write(
            root.join("src/lib.rs"),
            "pub fn caller() { helper(); }\npub fn helper() {}\npub fn caller_again() {}\n",
        )?;
        let root = fs::canonicalize(root)?;
        let database = root.join("graph.db");
        TursoGraphStore::migrate(&database)?;
        let scan = scan(&root, &["rs"])?;
        let generation = GenerationId::derive(&[b"mcp-stdio-test-generation"]);
        let mut engine = SyntaxMeshEngine::new(
            TursoGraphStore::open(&database)?,
            RustExtractor,
            RepositoryId::derive(&[b"mcp-stdio-test-repository"]),
            WorktreeId::derive(&[b"mcp-stdio-test-worktree"]),
        );
        engine.index(
            &scan.files,
            IndexRunId::derive(&[b"mcp-stdio-test-run"]),
            generation,
        )?;
        fs::write(
            root.join("src/lib.rs"),
            "pub fn caller() { helper(); }\npub fn helper() {}\npub fn caller_again() {}\npub fn later() {}\n",
        )?;
        let current = GenerationId::derive(&[b"mcp-stdio-current-generation"]);
        engine.index(
            &syntaxmesh_scanner::scan(&root, &["rs"])?.files,
            IndexRunId::derive(&[b"mcp-stdio-edit-run"]),
            current,
        )?;
        drop(engine);
        Ok((Self(root), database, current, generation))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

#[test]
fn stdio_initializes_lists_read_tools_and_executes_search() -> Result<(), Box<dyn Error>> {
    exercise_stdio(false, false)
}

#[test]
fn stdio_source_content_selection_uses_existing_protocol() -> Result<(), Box<dyn Error>> {
    exercise_stdio(true, false)
}

#[test]
fn stdio_composed_discovery_selection_uses_existing_protocol() -> Result<(), Box<dyn Error>> {
    exercise_stdio(true, true)
}

fn exercise_stdio(source_content: bool, lexical_first: bool) -> Result<(), Box<dyn Error>> {
    let (fixture, database, generation, previous) = Fixture::indexed_database()?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh-mcp"));
    command.arg(database).arg(&fixture.0).arg("cl100k_base");
    if source_content {
        command.arg("--source-content-context");
    }
    if lexical_first {
        command.arg("--lexical-first-context");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "syntaxmesh-mcp-test", "version": "0.1.0"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "search", "arguments": {"text": "caller", "limit": 1}}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "context",
                "arguments": {"query": "caller", "token_budget": 512}
            }
        }),
        json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": {"name":"context", "arguments":{"query":"caller", "token_budget":512, "generation":generation.0.to_hex()}}
        }),
        json!({
            "jsonrpc": "2.0", "id": 6, "method": "tools/call",
            "params": {"name":"context", "arguments":{"query":"caller", "token_budget":8192, "generation":previous.0.to_hex()}}
        }),
    ];
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::other("MCP child stdin was not piped"))?;
        for request in requests {
            serde_json::to_writer(&mut *stdin, &request)?;
            stdin.write_all(b"\n")?;
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "MCP stdio process exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let responses = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?;
    let response = |id| {
        responses
            .iter()
            .find(|value| value.get("id").and_then(Value::as_i64) == Some(id))
    };

    let initialized =
        response(1).ok_or_else(|| std::io::Error::other("missing MCP initialize response"))?;
    if !initialized.get("result").is_some_and(Value::is_object) {
        return Err(std::io::Error::other(format!(
            "unexpected initialize response: {initialized}"
        ))
        .into());
    }

    let listed =
        response(2).ok_or_else(|| std::io::Error::other("missing MCP tools/list response"))?;
    let names = listed
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .ok_or_else(|| std::io::Error::other("MCP tools/list did not return a tools array"))?
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if names != ["context", "neighbors", "node", "path", "search", "status"] {
        return Err(std::io::Error::other(format!("unexpected MCP tools: {names:?}")).into());
    }

    let called =
        response(3).ok_or_else(|| std::io::Error::other("missing MCP tools/call response"))?;
    let content = called
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text").and_then(Value::as_str))
        .ok_or_else(|| std::io::Error::other(format!("unexpected search response: {called}")))?;
    let search: Value = serde_json::from_str(content)?;
    if search.get("generation").and_then(Value::as_str) != Some(generation.0.to_hex().as_str())
        || search.get("nodes").and_then(Value::as_array).map(Vec::len) != Some(1)
    {
        return Err(
            std::io::Error::other(format!("unexpected bounded search result: {search}")).into(),
        );
    }
    let context_called =
        response(4).ok_or_else(|| std::io::Error::other("missing MCP context call response"))?;
    let context_content = context_called
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text").and_then(Value::as_str))
        .ok_or_else(|| {
            std::io::Error::other(format!("unexpected context response: {context_called}"))
        })?;
    let context: Value = serde_json::from_str(context_content)?;
    let expected_generation = serde_json::to_value(generation)?;
    if context.get("generation") != Some(&expected_generation)
        || context.get("tokenizer").and_then(Value::as_str)
            != Some("tiktoken-rs-0.12.1/cl100k_base/ordinary")
        || context
            .get("token_count")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX)
            > 512
    {
        return Err(std::io::Error::other(format!(
            "unexpected generation-pinned context pack: {context}"
        ))
        .into());
    }
    let selected = response(5)
        .ok_or_else(|| std::io::Error::other("missing selected-generation context response"))?;
    let selected_text = selected
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| std::io::Error::other("selected context response is not text"))?;
    let selected_pack: Value = serde_json::from_str(selected_text)?;
    if selected_pack != context {
        return Err(std::io::Error::other(
            "stdio generation selection changed the current context pack",
        )
        .into());
    }
    let historical =
        response(6).ok_or_else(|| std::io::Error::other("missing historical context response"))?;
    let historical_text = historical
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| std::io::Error::other("historical context response is not text"))?;
    verify_historical_context(&serde_json::from_str(historical_text)?, previous)?;
    Ok(())
}

fn verify_historical_context(
    pack: &syntaxmesh_api_model::ContextPack,
    previous: GenerationId,
) -> Result<(), Box<dyn Error>> {
    use syntaxmesh_api_model::ContextItemKind;

    if pack.generation != previous
        || pack.token_count > pack.token_budget
        || !pack.items.iter().any(|item| item.text.contains("caller"))
        || pack
            .items
            .iter()
            .any(|item| item.kind == ContextItemKind::SourceEvidence || item.text.contains("later"))
        || !pack
            .warnings
            .iter()
            .any(|warning| warning.code == "stale_source")
    {
        return Err(std::io::Error::other(
            "stdio historical context mixed generations or attached changed source bytes",
        )
        .into());
    }
    Ok(())
}
