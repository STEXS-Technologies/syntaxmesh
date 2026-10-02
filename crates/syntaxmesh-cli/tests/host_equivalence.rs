use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use syntaxmesh_api_model::GraphRecord;
use syntaxmesh_api_model::TemporalRecord;
use syntaxmesh_core::{
    AcceptanceTime, ChangeSet, ChangeSetDelta, ChangeSetId, ChangeSetKind, ChangeSetMembership,
    ConsequenceDelta, ConsequenceDerivation, ConsequenceEdge, ConsequenceEdgeId, ConsequenceKind,
    EvidenceClass, FactRef, FactVersionRef, FileId, FileVersion, GenerationId, GraphDelta,
    GraphDeltaWithConsequences, GraphDeltaWithLineage, IndexRunId, LineageEndpoint, Node, NodeId,
    NodeKind, Provenance, ProvenanceId, RepositoryId, StableId, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_extension_sdk::{
    Capability, EXTENSION_MANIFEST_SCHEMA_VERSION, ExtensionManifest, FactBatch,
};
use syntaxmesh_integration_penelope::PenelopeWorkflow;
use syntaxmesh_lang_bash::BashExtractor;
use syntaxmesh_lang_docs::DocumentationExtractor;
use syntaxmesh_lang_ecmascript::{JavaScriptExtractor, TypeScriptExtractor};
use syntaxmesh_lang_python::PythonExtractor;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::{CompositeExtractor, LanguageExtractor};
use syntaxmesh_runtime_protocol::{OBSERVATION_SCHEMA_VERSION, Observation, ObservationOrigin};
use syntaxmesh_scanner::scan;
use syntaxmesh_store::{DurableRecordStore, FileGraphStore, GraphStore, InMemoryGraphStore};
use syntaxmesh_store_sqlite::SqliteGraphStore;
use syntaxmesh_store_turso::TursoGraphStore;
use syntaxmesh_workflow::DurableIndexWorkflow;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[path = "support/extension_import.rs"]
mod extension_import;

#[test]
fn one_command_indexes_docs_with_batched_loopback_semantics() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let docs = fixture.path.join("docs");
    fs::create_dir(&docs)?;
    let quote = "The scheduler uses layered checkpoints to keep history queries fast.";
    fs::write(
        docs.join("architecture.md"),
        format!("# History\n\n{quote}\n"),
    )?;
    let second_quote = "Durable workflows use Penelope to resume interrupted indexing.";
    let updated_quote = "StateChronicle verifies accepted generation manifests.";
    fs::write(
        docs.join("operations.md"),
        format!("# Recovery\n\n{second_quote}\n"),
    )?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = std::thread::spawn(move || -> Result<(), Box<dyn Error + Send + Sync>> {
        for request_number in 0..2 {
            let (mut stream, _) = listener.accept()?;
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            let (header_end, content_length) = loop {
                let count = stream.read(&mut buffer)?;
                if count == 0 {
                    return Err(
                        std::io::Error::other("client closed before request headers").into(),
                    );
                }
                request.extend_from_slice(
                    buffer
                        .get(..count)
                        .ok_or_else(|| std::io::Error::other("invalid request read length"))?,
                );
                if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    let header_end = position + 4;
                    let header_bytes = request
                        .get(..position)
                        .ok_or_else(|| std::io::Error::other("invalid header boundary"))?;
                    let headers = std::str::from_utf8(header_bytes)?;
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>())
                        })
                        .ok_or_else(|| std::io::Error::other("missing content-length"))??;
                    break (header_end, length);
                }
            };
            while request.len().saturating_sub(header_end) < content_length {
                let count = stream.read(&mut buffer)?;
                if count == 0 {
                    return Err(std::io::Error::other("client closed before request body").into());
                }
                request.extend_from_slice(
                    buffer
                        .get(..count)
                        .ok_or_else(|| std::io::Error::other("invalid request read length"))?,
                );
            }
            let body_end = header_end
                .checked_add(content_length)
                .ok_or_else(|| std::io::Error::other("request body length overflow"))?;
            let body_bytes = request
                .get(header_end..body_end)
                .ok_or_else(|| std::io::Error::other("invalid request body boundary"))?;
            let body: serde_json::Value = serde_json::from_slice(body_bytes)?;
            if body.get("model").and_then(serde_json::Value::as_str) != Some("qwen3:latest") {
                return Err(std::io::Error::other(
                    "bare --semantic did not select the documented default model",
                )
                .into());
            }
            let message = body
                .get("messages")
                .and_then(|messages| messages.get(1))
                .and_then(|message| message.get("content"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing semantic request content"))?;
            let input: serde_json::Value = serde_json::from_str(message)?;
            let groups = input
                .get("requests")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| std::io::Error::other("missing semantic request groups"))?;
            let expected_groups = if request_number == 0 { 2 } else { 1 };
            if groups.len() != expected_groups {
                return Err(std::io::Error::other(
                    "packed prompt lost independent document request boundaries",
                )
                .into());
            }
            let chunks = groups
                .iter()
                .map(|group| {
                    group
                        .get("chunks")
                        .and_then(serde_json::Value::as_array)
                        .ok_or_else(|| std::io::Error::other("missing semantic group chunks"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if chunks.iter().any(|group| group.len() != 1) {
                return Err(std::io::Error::other(
                    "each fixture document must retain its own single chunk group",
                )
                .into());
            }
            let chunks = chunks.into_iter().flatten().collect::<Vec<_>>();
            let content = if request_number == 0 {
                let first_chunk = chunks
                    .iter()
                    .find(|chunk| {
                        chunk
                            .get("text")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|text| text.contains(quote))
                    })
                    .ok_or_else(|| std::io::Error::other("missing first semantic chunk"))?;
                let first_hash = first_chunk
                    .get("content_hash")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| std::io::Error::other("missing first semantic chunk hash"))?;
                let second_chunk = chunks
                    .iter()
                    .find(|chunk| {
                        chunk
                            .get("text")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|text| text.contains(second_quote))
                    })
                    .ok_or_else(|| std::io::Error::other("missing second semantic chunk"))?;
                let second_hash = second_chunk
                    .get("content_hash")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| std::io::Error::other("missing second semantic chunk hash"))?;
                serde_json::json!({
                    "claims": [
                        {
                            "subject": "SyntaxMesh",
                            "relation": "uses_layered_checkpoints_for",
                            "object": "fast history queries",
                            "evidence": [{"chunk_content_hash": first_hash, "quote": quote}]
                        },
                        {
                            "subject": "Durable workflows",
                            "relation": "use",
                            "object": "Penelope",
                            "evidence": [{"chunk_content_hash": second_hash, "quote": second_quote}]
                        }
                    ]
                })
            } else {
                let changed_chunk = chunks
                    .first()
                    .ok_or_else(|| std::io::Error::other("missing changed semantic chunk"))?;
                let changed_hash = changed_chunk
                    .get("content_hash")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| std::io::Error::other("missing changed semantic chunk hash"))?;
                let changed_text = changed_chunk
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| std::io::Error::other("missing changed semantic chunk text"))?;
                if !changed_text.contains(updated_quote) {
                    return Err(std::io::Error::other(
                        "edited request did not contain the changed document",
                    )
                    .into());
                }
                serde_json::json!({
                    "claims": [{
                        "subject": "StateChronicle",
                        "relation": "verifies",
                        "object": "accepted generation manifests",
                        "evidence": [{"chunk_content_hash": changed_hash, "quote": updated_quote}]
                    }]
                })
            }
            .to_string();
            let response = serde_json::json!({
                "choices": [{"message": {"content": content}, "finish_reason": "stop"}]
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )?;
        }
        Ok(())
    });
    let snapshot = fixture.path.join(".syntaxmesh/index.snapshot");
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        // A local model must remain directly local even when the host has
        // configured proxies. Change only this child process's environment.
        .envs([
            ("HTTP_PROXY", "http://127.0.0.1:1"),
            ("http_proxy", "http://127.0.0.1:1"),
            ("HTTPS_PROXY", "http://127.0.0.1:1"),
            ("https_proxy", "http://127.0.0.1:1"),
            ("ALL_PROXY", "http://127.0.0.1:1"),
            ("all_proxy", "http://127.0.0.1:1"),
        ])
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .arg("index")
        .arg("--semantic")
        .arg("--semantic-model-revision=fixture-revision-1")
        .arg("--semantic-endpoint")
        .arg(format!("http://{address}/v1"))
        .current_dir(&fixture.path)
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success()
        || !stdout.contains("batches=2")
        || !stdout.contains("provider_requests=1")
        || !stdout.contains("cache_reused=0")
        || !stdout.contains("claims=2")
        || !stdout.contains("concepts=4")
    {
        return Err(std::io::Error::other(format!(
            "one-command semantic index failed: status={}, stdout={stdout}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let store = FileGraphStore::open(&snapshot)?;
    let published_generation = store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("semantic generation was not published"))?
        .generation;
    let history_len = store.generation_history()?.len();
    let repeated = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg("--semantic")
        .arg("--semantic-offline")
        .arg("--semantic-model-revision=fixture-revision-1")
        .arg("--semantic-endpoint")
        // All input is cached now, so this call must avoid HTTP.
        .arg(format!("http://{address}/v1"))
        .current_dir(&fixture.path)
        .output()?;
    let repeated_stdout = String::from_utf8(repeated.stdout)?;
    let repeated_store = FileGraphStore::open(&snapshot)?;
    if !repeated.status.success()
        || !repeated_stdout.contains("provider_requests=0")
        || !repeated_stdout.contains("cache_reused=2")
        || !repeated_stdout.contains("(already current)")
        || repeated_store
            .latest_generation()
            .map(|manifest| manifest.generation)
            != Some(published_generation)
        || repeated_store.generation_history()?.len() != history_len
    {
        return Err(std::io::Error::other(format!(
            "unchanged semantic indexing did not reuse cached output without publication: status={}, stdout={repeated_stdout}, stderr={}, history={:?}",
            repeated.status,
            String::from_utf8_lossy(&repeated.stderr),
            repeated_store.generation_history()?
        ))
        .into());
    }
    fs::write(
        docs.join("operations.md"),
        format!("# Verification\n\n{updated_quote}\n"),
    )?;
    let offline_miss = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg("--semantic")
        .arg("--semantic-offline")
        .arg("--semantic-model-revision=fixture-revision-1")
        .arg("--semantic-endpoint")
        .arg(format!("http://{address}/v1"))
        .current_dir(&fixture.path)
        .output()?;
    if offline_miss.status.success()
        || !String::from_utf8_lossy(&offline_miss.stderr).contains("semantic cache miss")
        || !String::from_utf8_lossy(&offline_miss.stdout)
            .contains("semantic_usage reports=0 missing=0 invalid=0")
    {
        return Err(
            std::io::Error::other("offline cache miss did not fail without inference").into(),
        );
    }
    let after_miss = FileGraphStore::open(&snapshot)?;
    let after_miss_generation = after_miss
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("missing deterministic generation after cache miss"))?
        .generation;
    let semantic_names = |nodes: Vec<syntaxmesh_core::Node>| {
        nodes.into_iter().filter(|node| matches!(&node.kind, NodeKind::External { namespace, .. } if namespace == "syntaxmesh.semantic"))
            .map(|node| node.name).collect::<std::collections::BTreeSet<_>>()
    };
    let previous_semantics = semantic_names(store.nodes(published_generation)?);
    let current_semantics = semantic_names(after_miss.nodes(after_miss_generation)?);
    if !current_semantics.is_subset(&previous_semantics)
        || semantic_names(after_miss.historical_snapshot(published_generation)?.nodes)
            != previous_semantics
        || after_miss.generation_history()?.len() != history_len.saturating_add(1)
    {
        return Err(
            std::io::Error::other("offline cache miss partially replaced semantic facts").into(),
        );
    }
    let changed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg("--semantic")
        .arg("--semantic-model-revision=fixture-revision-1")
        .arg("--semantic-endpoint")
        .arg(format!("http://{address}/v1"))
        .current_dir(&fixture.path)
        .output()?;
    server
        .join()
        .map_err(|error| {
            std::io::Error::other(format!("semantic mock server panicked: {error:?}"))
        })?
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let changed_stdout = String::from_utf8(changed.stdout)?;
    let changed_store = FileGraphStore::open(&snapshot)?;
    let changed_generation = changed_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("edited semantic generation is missing"))?
        .generation;
    let current_semantic_claims = changed_store
        .nodes(changed_generation)?
        .into_iter()
        .filter(|node| {
            matches!(&node.kind, NodeKind::External { namespace, kind }
                if namespace == "syntaxmesh.semantic" && kind == "claim")
        })
        .map(|node| node.name)
        .collect::<Vec<_>>();
    if !changed.status.success()
        || !changed_stdout.contains("batches=2")
        || !changed_stdout.contains("provider_requests=1")
        || !changed_stdout.contains("cache_reused=1")
        || !current_semantic_claims.iter().any(|claim| {
            claim.to_ascii_lowercase().contains("syntaxmesh")
                && claim.contains("fast history queries")
        })
        || !current_semantic_claims.iter().any(|claim| {
            claim.to_ascii_lowercase().contains("statechronicle")
                && claim.contains("accepted generation manifests")
        })
        || current_semantic_claims.iter().any(|claim| {
            claim.to_ascii_lowercase().contains("durable workflows") && claim.contains("Penelope")
        })
    {
        return Err(std::io::Error::other(format!(
            "editing one document failed to reuse the other document's cache and replace stale claims: status={}, stdout={changed_stdout}, stderr={}, claims={current_semantic_claims:?}",
            changed.status,
            String::from_utf8_lossy(&changed.stderr)
        ))
        .into());
    }
    Ok(())
}

fn open_turso(path: impl AsRef<Path>) -> Result<TursoGraphStore, Box<dyn Error>> {
    TursoGraphStore::migrate(path.as_ref())?;
    Ok(TursoGraphStore::open(path)?)
}

fn migrate_turso_cli(path: &Path) -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("turso-migrate")
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "Turso migration command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}

#[test]
fn cli_help_is_available_without_arguments_and_for_help_flags() -> Result<(), Box<dyn Error>> {
    for arguments in [Vec::<&str>::new(), vec!["--help"], vec!["-h"], vec!["help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .args(arguments)
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        if !output.status.success()
            || !stdout.contains(
                "SyntaxMesh local Rust/Python/TypeScript/JavaScript/Bash/documentation graph engine",
            )
            || !stdout.contains("syntaxmesh graph-at-turso")
            || !stdout.contains("syntaxmesh graph-at-known-by-turso")
            || !stdout.contains("syntaxmesh neighborhood-at[-turso]")
            || !stdout.contains("syntaxmesh fact-lineage-turso")
            || !stdout.contains("syntaxmesh change-set-events[-turso]")
            || !stdout.contains("syntaxmesh consequence-trace[-turso]")
            || !stdout.contains("syntaxmesh sqlite-migrate")
            || !stdout.contains("syntaxmesh sqlite-migration-status")
            || !stdout.contains("syntaxmesh workflow-rejections <snapshot> <limit> [after-run-id]")
            || !stdout.contains(
                "syntaxmesh workflow-rejections-turso <database> <limit> [after-run-id]",
            )
            || !stdout.contains("syntaxmesh turso-migrate")
            || !stdout.contains("syntaxmesh turso-migration-status")
            || !stdout.contains("syntaxmesh statechronicle-verify <snapshot>")
            || !stdout.contains("syntaxmesh statechronicle-verify-turso <database>")
            || !stdout.contains(
                "syntaxmesh init [project-root] [--verified-history] [--node-module-resolution] [--python-module-resolution]",
            )
            || !stdout.contains("--verify")
            || !stdout.contains("command default: gpt-6-luna")
            || !stdout.contains("HTTP default: qwen3:latest")
            || !stdout.contains("an exact {model} argument receives the selected model")
            || !stdout.contains("--semantic-command @fixtures/codex-semantic-command.json")
            || !stdout.contains("--semantic <model-name>")
            || !stdout.contains("without a provider call or harness launch")
            || !stdout.contains("command mode cannot verify remote revisions")
            || !stdout.contains("--semantic-api-key-env, and --allow-network are rejected")
        {
            return Err(std::io::Error::other(format!(
                "CLI help was not successful and complete: status={}, stdout={stdout}, stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
    }
    Ok(())
}

#[test]
fn cli_index_history_distinguishes_reverted_snapshots_and_skips_exact_repeat()
-> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let snapshot = fixture.path.join("history.snapshot");
    let source = fixture.path.join("src/main.rs");
    let initial = fs::read(&source)?;

    let index = |root: &Path, store: &Path| -> Result<(), Box<dyn Error>> {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("index")
            .arg(root)
            .arg(store)
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "CLI index failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
        Ok(())
    };

    index(&fixture.path, &snapshot)?;
    let first = FileGraphStore::open(&snapshot)?
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("initial generation missing"))?;

    fs::write(
        &source,
        b"pub fn caller() { helper(); }\npub fn changed() {}\n",
    )?;
    index(&fixture.path, &snapshot)?;
    let second = FileGraphStore::open(&snapshot)?
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("second generation missing"))?;

    fs::write(&source, initial)?;
    index(&fixture.path, &snapshot)?;
    let third = FileGraphStore::open(&snapshot)?;
    let third_manifest = third
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("third generation missing"))?;
    let history = third.generation_history()?;
    if first.generation == second.generation
        || first.generation == third_manifest.generation
        || second.generation == third_manifest.generation
        || history.len() != 3
        || history
            .first()
            .and_then(|entry| entry.delta.as_ref())
            .and_then(|delta| delta.expected_base)
            .is_some()
        || history
            .get(1)
            .and_then(|entry| entry.delta.as_ref())
            .and_then(|delta| delta.expected_base)
            != Some(first.generation)
        || history
            .get(2)
            .and_then(|entry| entry.delta.as_ref())
            .and_then(|delta| delta.expected_base)
            != Some(second.generation)
    {
        return Err(std::io::Error::other(format!(
            "A → B → A did not produce three parent-linked generations: {history:?}"
        ))
        .into());
    }

    index(&fixture.path, &snapshot)?;
    let repeated = FileGraphStore::open(&snapshot)?;
    if repeated.generation_history()?.len() != 3
        || repeated
            .latest_generation()
            .map(|manifest| manifest.generation)
            != Some(third_manifest.generation)
    {
        return Err(std::io::Error::other(
            "re-indexing the current snapshot created another generation",
        )
        .into());
    }
    Ok(())
}

#[test]
fn chronological_consequence_trace_cli_streams_file_and_turso_records() -> Result<(), Box<dyn Error>>
{
    let fixture = FixtureDirectory::new()?;
    let file_snapshot = fixture.path.join("trace.snapshot");
    let turso_database = fixture.path.join("trace.db");
    let first_generation = GenerationId::derive(&[b"cli-trace-first"]);
    let second_generation = GenerationId::derive(&[b"cli-trace-second"]);
    let repository = RepositoryId::derive(&[b"cli-trace-repository"]);
    let worktree = WorktreeId::derive(&[b"cli-trace-worktree"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"cli-trace-provenance"]),
        producer_namespace: "syntaxmesh.cli.trace-test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let source = Node {
        id: NodeId::derive(&[b"cli-trace-source"]),
        kind: NodeKind::Function,
        name: "trace-source".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let target = Node {
        id: NodeId::derive(&[b"cli-trace-target"]),
        kind: NodeKind::Function,
        name: "trace-target".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let source_id = source.id;
    let source_ref = FactVersionRef {
        fact: FactRef::Node(source.id),
        valid_from: first_generation,
    };
    let target_ref = FactVersionRef {
        fact: FactRef::Node(target.id),
        valid_from: first_generation,
    };
    let first = GraphDeltaWithConsequences {
        publication: GraphDeltaWithLineage {
            graph: GraphDelta {
                repository,
                worktree,
                run_id: IndexRunId::derive(&[b"cli-trace-run-first"]),
                expected_base: None,
                next_generation: first_generation,
                changed_files: Vec::new(),
                removed_files: Vec::new(),
                upsert_provenance: vec![provenance.clone()],
                upsert_nodes: vec![source, target],
                upsert_edges: Vec::new(),
                remove_nodes: Vec::new(),
                remove_edges: Vec::new(),
            },
            lineage: Default::default(),
        },
        consequences: ConsequenceDelta::default(),
    };
    let second = GraphDeltaWithConsequences {
        publication: GraphDeltaWithLineage {
            graph: GraphDelta {
                repository,
                worktree,
                run_id: IndexRunId::derive(&[b"cli-trace-run-second"]),
                expected_base: Some(first_generation),
                next_generation: second_generation,
                changed_files: Vec::new(),
                removed_files: Vec::new(),
                upsert_provenance: Vec::new(),
                upsert_nodes: Vec::new(),
                upsert_edges: Vec::new(),
                remove_nodes: Vec::new(),
                remove_edges: Vec::new(),
            },
            lineage: Default::default(),
        },
        consequences: ConsequenceDelta {
            add: vec![ConsequenceEdge {
                id: ConsequenceEdgeId::derive(&[b"cli-trace-edge"]),
                source: LineageEndpoint::FactVersion(source_ref),
                target: LineageEndpoint::FactVersion(target_ref),
                kind: ConsequenceKind::DirectDependencyEffect,
                evidence: vec![source_ref],
                derivation: ConsequenceDerivation::Explicit,
                provenance: provenance.id,
            }],
            retract: Vec::new(),
        },
    };

    let mut file_store = FileGraphStore::open(&file_snapshot)?;
    file_store.apply_delta_with_consequences(first.clone(), None)?;
    file_store.apply_delta_with_consequences(second.clone(), None)?;
    drop(file_store);
    let mut turso_store = open_turso(&turso_database)?;
    turso_store.apply_delta_with_consequences(first, None)?;
    turso_store.apply_delta_with_consequences(second, None)?;
    drop(turso_store);

    for (command, store) in [
        ("consequence-trace", file_snapshot.as_path()),
        ("consequence-trace-turso", turso_database.as_path()),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(store)
            .arg(first_generation.0.to_hex())
            .arg(second_generation.0.to_hex())
            .arg("fact")
            .arg("node")
            .arg(source_id.0.to_hex())
            .arg(first_generation.0.to_hex())
            .args(["8", "20", "20", "100", "1048576"])
            .args(["--evidence-classes", "user_asserted"])
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "{command} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
        let records = String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str::<TemporalRecord>)
            .collect::<Result<Vec<_>, _>>()?;
        if !matches!(records.first(), Some(TemporalRecord::ConsequenceTraceHeader {
            from_generation, until_generation, ..
        }) if *from_generation == first_generation && *until_generation == second_generation)
            || !records.iter().any(|record| {
                matches!(record,
                TemporalRecord::ConsequenceTraceHop { version, reached_generation, .. }
                    if version.edge.kind == ConsequenceKind::DirectDependencyEffect
                        && version.edge.source == LineageEndpoint::FactVersion(source_ref)
                        && *reached_generation == second_generation)
            })
            || !matches!(
                records.last(),
                Some(TemporalRecord::ConsequenceTraceFooter {
                    states: 2,
                    hops: 1,
                    truncated: false,
                    ..
                })
            )
        {
            return Err(std::io::Error::other(format!(
                "{command} did not emit the expected bounded temporal trace"
            ))
            .into());
        }
        let filtered_output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(store)
            .arg(first_generation.0.to_hex())
            .arg(second_generation.0.to_hex())
            .arg("fact")
            .arg("node")
            .arg(source_id.0.to_hex())
            .arg(first_generation.0.to_hex())
            .args(["8", "20", "20", "100", "1048576"])
            .args(["--evidence-classes", "source_fact"])
            .output()?;
        if !filtered_output.status.success() {
            return Err(std::io::Error::other(format!(
                "{command} evidence-class filter failed: {}",
                String::from_utf8_lossy(&filtered_output.stderr)
            ))
            .into());
        }
        let filtered_records = String::from_utf8(filtered_output.stdout)?
            .lines()
            .map(serde_json::from_str::<TemporalRecord>)
            .collect::<Result<Vec<_>, _>>()?;
        if !matches!(
            filtered_records.last(),
            Some(TemporalRecord::ConsequenceTraceFooter {
                states: 1,
                hops: 0,
                truncated: false,
                ..
            })
        ) {
            return Err(std::io::Error::other(format!(
                "{command} did not apply the exact evidence-class filter"
            ))
            .into());
        }
    }
    Ok(())
}

#[test]
fn historical_neighbor_cli_pages_match_file_and_turso() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    fs::write(
        fixture.path.join("src/main.rs"),
        "pub fn caller() { helper(); other(); }\n",
    )?;
    fs::write(
        fixture.path.join("src/support.rs"),
        "pub fn helper() {}\npub fn other() {}\n",
    )?;

    let snapshot = fixture.path.join("neighbors.snapshot");
    let file_indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&snapshot)
        .output()?;
    if !file_indexed.status.success() {
        return Err(std::io::Error::other(format!(
            "File historical-neighbor fixture indexing failed: {}",
            String::from_utf8_lossy(&file_indexed.stderr)
        ))
        .into());
    }
    let database = fixture.path.join("neighbors.turso");
    migrate_turso_cli(&database)?;
    let turso_indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&fixture.path)
        .arg(&database)
        .output()?;
    if !turso_indexed.status.success() {
        return Err(std::io::Error::other(format!(
            "Turso historical-neighbor fixture indexing failed: {}",
            String::from_utf8_lossy(&turso_indexed.stderr)
        ))
        .into());
    }

    let file_store = FileGraphStore::open(&snapshot)?;
    let turso_store = TursoGraphStore::open(&database)?;
    let generation = file_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("historical-neighbor fixture has no generation"))?
        .generation;
    if turso_store.latest_generation().map(|item| item.generation) != Some(generation) {
        return Err(std::io::Error::other("File and Turso generations differ").into());
    }
    drop(turso_store);
    let caller = file_store
        .nodes(generation)?
        .into_iter()
        .find(|item| item.name == "caller")
        .ok_or_else(|| std::io::Error::other("historical-neighbor fixture has no caller"))?;
    let mut expected_edges = file_store
        .edges(generation)?
        .into_iter()
        .filter(|edge| edge.source == caller.id)
        .collect::<Vec<_>>();
    expected_edges.sort_by_key(|edge| edge.id);
    if expected_edges.len() < 2 {
        return Err(std::io::Error::other(format!(
            "fixture must have at least two outgoing caller edges; got {}",
            expected_edges.len()
        ))
        .into());
    }

    let read_page = |command: &str, store: &Path, after: Option<syntaxmesh_core::EdgeId>| {
        let mut process = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
        process
            .arg(command)
            .arg(store)
            .arg(generation.0.to_hex())
            .arg(caller.id.0.to_hex())
            .arg("outgoing")
            .arg("1");
        if let Some(edge) = after {
            process.arg(edge.0.to_hex());
        }
        process.output()
    };
    let parse_page = |output: std::process::Output| -> Result<Vec<TemporalRecord>, Box<dyn Error>> {
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "historical-query CLI failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
        String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str::<TemporalRecord>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    };

    let file_first_page = parse_page(read_page("neighbors-at", &snapshot, None)?)?;
    let turso_first_page = parse_page(read_page("neighbors-at-turso", &database, None)?)?;
    if file_first_page != turso_first_page {
        return Err(
            std::io::Error::other("File and Turso historical-neighbor pages differ").into(),
        );
    }
    let first_edge = match file_first_page.as_slice() {
        [
            TemporalRecord::HistoricalNeighbor { edge, .. },
            TemporalRecord::HistoricalNeighborFooter {
                has_more: true,
                next_cursor: Some(cursor),
                returned: 1,
                ..
            },
        ] if cursor.generation == generation && cursor.endpoint == caller.id => edge.id,
        records => {
            return Err(std::io::Error::other(format!(
                "first historical-neighbor page was not bounded/continuable: {records:?}"
            ))
            .into());
        }
    };
    let expected_first = expected_edges
        .first()
        .ok_or_else(|| std::io::Error::other("fixture has no first caller edge"))?
        .id;
    let expected_second = expected_edges
        .get(1)
        .ok_or_else(|| std::io::Error::other("fixture has no second caller edge"))?
        .id;
    if first_edge != expected_first {
        return Err(std::io::Error::other("first page was not ordered by edge identity").into());
    }
    let file_second_page = parse_page(read_page("neighbors-at", &snapshot, Some(first_edge))?)?;
    let turso_second_page = parse_page(read_page(
        "neighbors-at-turso",
        &database,
        Some(first_edge),
    )?)?;
    if file_second_page != turso_second_page
        || !matches!(file_second_page.as_slice(),
            [TemporalRecord::HistoricalNeighbor { edge, .. }, TemporalRecord::HistoricalNeighborFooter { returned: 1, .. }]
            if edge.id == expected_second)
    {
        return Err(std::io::Error::other(format!(
            "historical-neighbor cursor continuation differs or is out of order: {file_second_page:?}"
        ))
        .into());
    }

    let read_neighborhood = |command: &str, store: &Path| {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(store)
            .arg(generation.0.to_hex())
            .arg(caller.id.0.to_hex())
            .args(["4", "64", "64", "1024", "16777216"])
            .output()
    };
    let file_neighborhood = parse_page(read_neighborhood("neighborhood-at", &snapshot)?)?;
    let turso_neighborhood = parse_page(read_neighborhood("neighborhood-at-turso", &database)?)?;
    if file_neighborhood != turso_neighborhood
        || !matches!(
            file_neighborhood.last(),
            Some(TemporalRecord::HistoricalNeighborhoodFooter {
                schema_version: 1,
                generation: footer_generation,
                truncated: false,
                ..
            }) if *footer_generation == generation
        )
    {
        return Err(std::io::Error::other(format!(
            "File and Turso historical neighborhoods differ or are truncated: {file_neighborhood:?} vs {turso_neighborhood:?}"
        )).into());
    }
    Ok(())
}

#[test]
fn runtime_observations_survive_turso_restart_with_verified_history() -> Result<(), Box<dyn Error>>
{
    let fixture = FixtureDirectory::new()?;
    let database = fixture.path.join("runtime-observations.turso");
    let repository = RepositoryId::derive(&[b"runtime-observation-turso-repository"]);
    let worktree = WorktreeId::derive(&[b"runtime-observation-turso-worktree"]);
    let static_generation = GenerationId::derive(&[b"runtime-observation-turso-static"]);
    let observation_generation = GenerationId::derive(&[b"runtime-observation-turso-observed"]);
    let observation = Observation {
        schema_version: OBSERVATION_SCHEMA_VERSION,
        producer_namespace: "fixture.runtime".to_owned(),
        producer_version: "1.0.0".to_owned(),
        origin: ObservationOrigin::ServerProbe,
        observed_at_unix_nanos: 42,
        subject: "service-17".to_owned(),
        relation: "reported_state".to_owned(),
        object: "degraded".to_owned(),
        correlation_id: Some("request-42".to_owned()),
    };
    let mut engine =
        SyntaxMeshEngine::new(open_turso(&database)?, RustExtractor, repository, worktree)
            .with_statechronicle_verification();
    let source = b"pub fn handler() {}\n";
    engine.index(
        &[syntaxmesh_language_sdk::SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[b"src/main.rs"]),
                normalized_path: "src/main.rs".to_owned(),
                content_hash: *blake3::hash(source).as_bytes(),
                size_bytes: u64::try_from(source.len())?,
            },
            content: String::from_utf8(source.to_vec())?,
        }],
        IndexRunId::derive(&[b"runtime-observation-turso-static-run"]),
        static_generation,
    )?;
    let static_records = engine.query(static_generation).export_records()?;
    let static_nodes = static_records
        .iter()
        .filter_map(|record| match record {
            GraphRecord::Node { node, .. } => Some(node.clone()),
            GraphRecord::Header { .. }
            | GraphRecord::Edge { .. }
            | GraphRecord::Provenance { .. }
            | GraphRecord::Footer { .. } => None,
        })
        .collect::<Vec<_>>();
    let static_edges = static_records
        .iter()
        .filter_map(|record| match record {
            GraphRecord::Edge { edge, .. } => Some(edge.clone()),
            GraphRecord::Header { .. }
            | GraphRecord::Node { .. }
            | GraphRecord::Provenance { .. }
            | GraphRecord::Footer { .. } => None,
        })
        .collect::<Vec<_>>();
    let receipt = engine.ingest_extension_facts(
        FactBatch {
            manifest: ExtensionManifest {
                schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
                namespace: observation.producer_namespace.clone(),
                producer_version: observation.producer_version.clone(),
                capabilities: vec![Capability::RuntimeObservations],
            },
            provenance: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            observations: vec![observation.clone()],
        },
        IndexRunId::derive(&[b"runtime-observation-turso-run"]),
        observation_generation,
    )?;
    if receipt.publication.generation.generation != observation_generation
        || receipt.verification != syntaxmesh_workflow::WorkflowStatus::Verified
    {
        return Err(std::io::Error::other(
            "Turso runtime observation did not publish through verified history",
        )
        .into());
    }
    let observation_records = engine.query(observation_generation).export_records()?;
    let observed_static_nodes = observation_records
        .iter()
        .filter_map(|record| match record {
            GraphRecord::Node { node, .. } if node.kind != NodeKind::RuntimeObservation => {
                Some(node.clone())
            }
            GraphRecord::Header { .. }
            | GraphRecord::Node { .. }
            | GraphRecord::Edge { .. }
            | GraphRecord::Provenance { .. }
            | GraphRecord::Footer { .. } => None,
        })
        .collect::<Vec<_>>();
    let observed_edges = observation_records
        .iter()
        .filter_map(|record| match record {
            GraphRecord::Edge { edge, .. } => Some(edge.clone()),
            GraphRecord::Header { .. }
            | GraphRecord::Node { .. }
            | GraphRecord::Provenance { .. }
            | GraphRecord::Footer { .. } => None,
        })
        .collect::<Vec<_>>();
    let observation_node = observation_records.iter().find_map(|record| match record {
        GraphRecord::Node { node, .. } if node.kind == NodeKind::RuntimeObservation => Some(node),
        GraphRecord::Header { .. }
        | GraphRecord::Node { .. }
        | GraphRecord::Edge { .. }
        | GraphRecord::Provenance { .. }
        | GraphRecord::Footer { .. } => None,
    });
    let observation_node = observation_node
        .ok_or_else(|| std::io::Error::other("Turso omitted the runtime observation node"))?;
    let observation_id = observation_node.id;
    let payload = observation_node
        .extension_payload
        .as_ref()
        .ok_or_else(|| std::io::Error::other("Turso observation has no protocol payload"))?;
    if payload.namespace != observation.producer_namespace
        || payload.schema_version != observation.schema_version
        || serde_json::from_slice::<Observation>(&payload.bytes)? != observation
        || observed_static_nodes != static_nodes
        || observed_edges != static_edges
        || !matches!(
            engine
                .query(observation_generation)
                .fact_history(syntaxmesh_core::FactRef::Node(observation_id))?
                .as_slice(),
            [syntaxmesh_api_model::TemporalRecord::FactVersion {
                observed_at: Some(syntaxmesh_core::ObservationTime(42)),
                accepted_at: Some(_),
                ..
            }]
        )
    {
        return Err(std::io::Error::other(
            "Turso mixed runtime and static facts or lost temporal observation data",
        )
        .into());
    }
    drop(engine);

    let reopened = TursoGraphStore::open(&database)?;
    let mut reopened_engine = SyntaxMeshEngine::new(reopened, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    let reopened_records = reopened_engine
        .query(observation_generation)
        .export_records()?;
    let reopened_observation = reopened_records.iter().find_map(|record| match record {
        GraphRecord::Node { node, .. } if node.id == observation_id => Some(node),
        GraphRecord::Header { .. }
        | GraphRecord::Node { .. }
        | GraphRecord::Edge { .. }
        | GraphRecord::Provenance { .. }
        | GraphRecord::Footer { .. } => None,
    });
    if reopened_observation
        .and_then(|node| node.extension_payload.as_ref())
        .is_none_or(|persisted_payload| {
            persisted_payload.namespace != observation.producer_namespace
                || serde_json::from_slice::<Observation>(&persisted_payload.bytes)
                    .ok()
                    .as_ref()
                    != Some(&observation)
        })
        || reopened_engine.verify_statechronicle_history()?.is_none()
    {
        return Err(std::io::Error::other(
            "Turso restart lost runtime evidence or its StateChronicle chain",
        )
        .into());
    }
    Ok(())
}

fn stale_workflow_record<S>(
    mut store: S,
    repository: RepositoryId,
    worktree: WorktreeId,
    prefix: &[u8],
) -> Result<(S, IndexRunId, GenerationId), Box<dyn Error>>
where
    S: GraphStore + DurableRecordStore,
{
    let accepted_generation = GenerationId::derive(&[prefix, b"accepted-generation"]);
    let base_run = IndexRunId::derive(&[prefix, b"accepted-run"]);
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: base_run,
        expected_base: None,
        next_generation: accepted_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let rejected_run = IndexRunId::derive(&[prefix, b"rejected-run"]);
    let stale_delta = GraphDelta {
        repository,
        worktree,
        run_id: rejected_run,
        expected_base: None,
        next_generation: GenerationId::derive(&[prefix, b"stale-generation"]),
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let mut workflow = PenelopeWorkflow::new(store);
    if !matches!(
        workflow.publish(stale_delta, AcceptanceTime(456)),
        Err(syntaxmesh_workflow::WorkflowError::Store(
            syntaxmesh_store::StoreError::StaleBase {
                expected: None,
                actual: Some(actual),
            }
        )) if actual == accepted_generation
    ) {
        return Err(
            std::io::Error::other("stale fixture did not produce a terminal rejection").into(),
        );
    }
    Ok((workflow.into_store(), rejected_run, accepted_generation))
}

#[test]
fn workflow_rejection_commands_read_durable_typed_details_for_file_and_turso()
-> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let repository = RepositoryId::derive(&[b"workflow-rejection-cli-repo"]);
    let worktree = WorktreeId::derive(&[b"workflow-rejection-cli-worktree"]);

    let snapshot = fixture.path.join("rejections.snapshot");
    let file_store = FileGraphStore::open(&snapshot)?;
    let (file_store, file_run, file_actual) =
        stale_workflow_record(file_store, repository, worktree, b"file-workflow-rejection")?;
    drop(file_store);
    let file_output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .args([
            "workflow-rejections",
            snapshot.to_str().ok_or("invalid path")?,
            "10",
        ])
        .output()?;
    let file_stdout = String::from_utf8(file_output.stdout)?;
    if !file_output.status.success()
        || !file_stdout.contains(&format!("run_id={}", file_run.0.to_hex()))
        || !file_stdout.contains(&format!("actual={}", file_actual.0.to_hex()))
        || !file_stdout.contains("reason=stale_base")
        || !file_stdout.contains("next_cursor=none")
    {
        return Err(std::io::Error::other(format!(
            "File rejection command omitted durable details: {file_stdout}; stderr={}",
            String::from_utf8_lossy(&file_output.stderr)
        ))
        .into());
    }

    let database = fixture.path.join("rejections.turso");
    let (turso_store, turso_run, turso_actual) = stale_workflow_record(
        open_turso(&database)?,
        repository,
        worktree,
        b"turso-workflow-rejection",
    )?;
    drop(turso_store);
    let turso_output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .args([
            "workflow-rejections-turso",
            database.to_str().ok_or("invalid path")?,
            "10",
        ])
        .output()?;
    let turso_stdout = String::from_utf8(turso_output.stdout)?;
    if !turso_output.status.success()
        || !turso_stdout.contains(&format!("run_id={}", turso_run.0.to_hex()))
        || !turso_stdout.contains(&format!("actual={}", turso_actual.0.to_hex()))
        || !turso_stdout.contains("reason=stale_base")
        || !turso_stdout.contains("next_cursor=none")
    {
        return Err(std::io::Error::other(format!(
            "Turso rejection command omitted durable details: {turso_stdout}; stderr={}",
            String::from_utf8_lossy(&turso_output.stderr)
        ))
        .into());
    }
    Ok(())
}

#[test]
fn node_module_resolution_is_explicit_and_changes_the_index_generation()
-> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    fs::create_dir_all(fixture.path.join("src"))?;
    fs::write(
        fixture.path.join("src/main.ts"),
        "import { publicValue as value } from './target'; export { value };\n",
    )?;
    fs::write(
        fixture.path.join("src/target.ts"),
        "const value = 1; export { value as publicValue };\n",
    )?;
    fs::write(
        fixture.path.join("src/lib.rs"),
        "use std::fmt::Debug;\npub fn render(_: impl Debug) {}\n",
    )?;
    fs::create_dir_all(fixture.path.join("src/pkg"))?;
    fs::create_dir_all(fixture.path.join("src/package"))?;
    fs::write(
        fixture.path.join("src/pkg/consumer.py"),
        "from .helpers import build as make\nimport package.module as local_module\n",
    )?;
    fs::write(fixture.path.join("src/pkg/__init__.py"), "")?;
    fs::write(
        fixture.path.join("src/pkg/helpers.py"),
        "def build():\n    pass\n",
    )?;
    fs::write(fixture.path.join("src/package/__init__.py"), "")?;
    fs::write(fixture.path.join("src/package/module.py"), "value = 1\n")?;
    fs::write(
        fixture.path.join("syntaxmesh.toml"),
        "[module_resolution]\nprofiles = [\"node\", \"python\"]\nsource_roots = [\"src\"]\n",
    )?;
    let snapshot = fixture.path.join("graph.snapshot");
    let indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&snapshot)
        .output()?;
    if !indexed.status.success() || !String::from_utf8_lossy(&indexed.stdout).contains("(Durable)")
    {
        return Err(std::io::Error::other(format!(
            "Node-profile CLI indexing failed or enabled verification: stdout={}, stderr={}",
            String::from_utf8_lossy(&indexed.stdout),
            String::from_utf8_lossy(&indexed.stderr)
        ))
        .into());
    }
    let file_store = FileGraphStore::open(&snapshot)?;
    let first = file_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("Node-profile index did not publish a generation"))?;
    let first_nodes = file_store.nodes(first.generation)?;
    let mut python_import_ids = first_nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                syntaxmesh_core::NodeKind::Import {
                    kind: syntaxmesh_core::ImportKind::PythonModule
                        | syntaxmesh_core::ImportKind::PythonFrom
                        | syntaxmesh_core::ImportKind::PythonStar,
                    ..
                }
            )
        })
        .map(|node| node.id)
        .collect::<Vec<_>>();
    python_import_ids.sort_unstable();
    let graph_edges = file_store.edges(first.generation)?;
    if python_import_ids.len() != 2
        || python_import_ids.iter().any(|node_id| {
            !graph_edges.iter().any(|edge| {
                edge.source == *node_id
                    && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
            }) || graph_edges.iter().any(|edge| {
                edge.source == *node_id
                    && edge.relation == syntaxmesh_core::RelationKind::HasResolutionDiagnostic
            })
        })
    {
        return Err(std::io::Error::other(
            "Python profile did not resolve its typed source imports in the mixed-language project",
        )
        .into());
    }
    let import = first_nodes
        .iter()
        .find(|node| {
            matches!(
                &node.kind,
                syntaxmesh_core::NodeKind::Import {
                    kind: syntaxmesh_core::ImportKind::Named,
                    ..
                }
            )
        })
        .ok_or_else(|| std::io::Error::other("indexed typed import occurrence is missing"))?;
    let import_id = import.id;
    if !graph_edges.iter().any(|edge| {
        edge.source == import_id && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
    }) {
        return Err(std::io::Error::other(
            "explicit Node profile did not resolve the indexed module",
        )
        .into());
    }
    let rust_import_ids = first_nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                syntaxmesh_core::NodeKind::Import {
                    kind: syntaxmesh_core::ImportKind::RustUse
                        | syntaxmesh_core::ImportKind::RustGlob,
                    ..
                }
            )
        })
        .map(|node| node.id)
        .collect::<Vec<_>>();
    if rust_import_ids.len() != 1
        || rust_import_ids.iter().any(|node_id| {
            graph_edges.iter().any(|edge| {
                edge.source == *node_id
                    && matches!(
                        edge.relation,
                        syntaxmesh_core::RelationKind::ResolvesTo
                            | syntaxmesh_core::RelationKind::HasResolutionDiagnostic
                    )
            })
        })
    {
        return Err(std::io::Error::other(
            "Node/Python module providers claimed or diagnosed a Rust use path",
        )
        .into());
    }
    drop(file_store);

    fs::write(fixture.path.join("syntaxmesh.toml"), "")?;
    let updated = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&snapshot)
        .output()?;
    if !updated.status.success() {
        return Err(std::io::Error::other(format!(
            "indexing after disabling module resolution failed: {}",
            String::from_utf8_lossy(&updated.stderr)
        ))
        .into());
    }
    let updated_store = FileGraphStore::open(&snapshot)?;
    let second = updated_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("second index lost its generation"))?;
    let second_nodes = updated_store.nodes(second.generation)?;
    let mut second_python_import_ids = second_nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                syntaxmesh_core::NodeKind::Import {
                    kind: syntaxmesh_core::ImportKind::PythonModule
                        | syntaxmesh_core::ImportKind::PythonFrom
                        | syntaxmesh_core::ImportKind::PythonStar,
                    ..
                }
            )
        })
        .map(|node| node.id)
        .collect::<Vec<_>>();
    second_python_import_ids.sort_unstable();
    let second_edges = updated_store.edges(second.generation)?;
    if second.generation == first.generation
        || python_import_ids != second_python_import_ids
        || python_import_ids.iter().any(|node_id| {
            second_edges.iter().any(|edge| {
                edge.source == *node_id
                    && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
            })
        })
        || second_edges.iter().any(|edge| {
            edge.source == import_id && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
        })
    {
        return Err(std::io::Error::other(
            "disabling the resolver did not publish a distinct generation and retract its edge",
        )
        .into());
    }
    Ok(())
}

#[test]
fn node_module_resolution_survives_turso_restart_and_graph_at() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    fs::write(
        fixture.path.join("src/main.ts"),
        "import { publicValue as value } from './barrel'; import * as barrelNamespace from './barrel'; export { value }; const namespaceValue = barrelNamespace?.['publicValue'];\n",
    )?;
    fs::write(
        fixture.path.join("src/barrel.ts"),
        "export * from './middle';\n",
    )?;
    fs::write(
        fixture.path.join("src/middle.ts"),
        "export * from './target';\n",
    )?;
    fs::write(
        fixture.path.join("src/target.ts"),
        "const value = 1; export { value as publicValue };\n",
    )?;
    fs::write(
        fixture.path.join("syntaxmesh.toml"),
        "[module_resolution]\nprofile = \"node\"\n",
    )?;
    let database = fixture.path.join("graph.turso");
    migrate_turso_cli(&database)?;

    let first_index = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&fixture.path)
        .arg(&database)
        .output()?;
    let first_output = String::from_utf8_lossy(&first_index.stdout);
    if !first_index.status.success() || !first_output.contains("(Durable)") {
        return Err(std::io::Error::other(format!(
            "Turso Node-profile indexing failed or enabled verification: stdout={first_output}, stderr={}",
            String::from_utf8_lossy(&first_index.stderr)
        ))
        .into());
    }

    let first_store = TursoGraphStore::open(&database)?;
    let first_generation = first_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("Turso Node-profile index has no generation"))?
        .generation;
    let first_nodes = first_store.nodes(first_generation)?;
    let import_id = first_nodes
        .iter()
        .find_map(|node| {
            if let syntaxmesh_core::NodeKind::Import { specifier, .. } = &node.kind {
                (specifier == "./barrel").then_some(node.id)
            } else {
                None
            }
        })
        .ok_or_else(|| std::io::Error::other("Turso graph omitted the typed import occurrence"))?;
    let target_id = first_nodes
        .iter()
        .find(|node| node.kind == syntaxmesh_core::NodeKind::Module && node.name == "src/barrel.ts")
        .map(|node| node.id)
        .ok_or_else(|| std::io::Error::other("Turso graph omitted the barrel module"))?;
    let first_edges = first_store.edges(first_generation)?;
    let resolution_edge = first_edges
        .iter()
        .find(|edge| {
            edge.source == import_id
                && edge.target == target_id
                && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
        })
        .cloned()
        .ok_or_else(|| std::io::Error::other("Turso Node profile did not resolve the import"))?;
    let binding_edge = first_edges
        .iter()
        .find(|edge| {
            edge.source == import_id
                && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
                && first_nodes.iter().any(|node| {
                    node.id == edge.target
                        && matches!(
                            &node.kind,
                            syntaxmesh_core::NodeKind::Export {
                                exported_name: Some(name),
                                ..
                            } if name == "publicValue"
                        )
                })
        })
        .cloned()
        .ok_or_else(|| {
            std::io::Error::other(
                "Turso Node profile did not bind the aliased import to its export",
            )
        })?;
    let namespace_member_id = first_nodes
        .iter()
        .find_map(|node| {
            if let syntaxmesh_core::NodeKind::Import {
                specifier,
                kind: syntaxmesh_core::ImportKind::NamespaceMember,
                imported_name: Some(imported_name),
                local_name: Some(local_name),
                ..
            } = &node.kind
                && specifier == "./barrel"
                && imported_name == "publicValue"
                && local_name == "barrelNamespace.publicValue"
            {
                Some(node.id)
            } else {
                None
            }
        })
        .ok_or_else(|| std::io::Error::other("Turso graph omitted namespace member usage"))?;
    let namespace_binding_edge = first_edges
        .iter()
        .find(|edge| {
            edge.source == namespace_member_id
                && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
                && first_nodes.iter().any(|node| {
                    node.id == edge.target
                        && matches!(
                            &node.kind,
                            syntaxmesh_core::NodeKind::Export {
                                exported_name: Some(name),
                                ..
                            } if name == "publicValue"
                        )
                })
        })
        .cloned()
        .ok_or_else(|| {
            std::io::Error::other("Turso Node profile did not bind namespace member to export")
        })?;
    drop(first_store);

    fs::remove_file(fixture.path.join("src/barrel.ts"))?;
    let second_index = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&fixture.path)
        .arg(&database)
        .output()?;
    if !second_index.status.success() {
        return Err(std::io::Error::other(format!(
            "Turso reindex after target removal failed: {}",
            String::from_utf8_lossy(&second_index.stderr)
        ))
        .into());
    }

    let reopened = TursoGraphStore::open(&database)?;
    let second_generation = reopened
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("Turso reindex lost its generation"))?
        .generation;
    if second_generation == first_generation
        || reopened
            .nodes(second_generation)?
            .iter()
            .any(|node| node.id == target_id)
        || reopened.edges(second_generation)?.iter().any(|edge| {
            edge.source == import_id && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
        })
    {
        return Err(std::io::Error::other(
            "reopened current Turso generation retained the deleted barrel or stale resolution edges",
        )
        .into());
    }
    let second_nodes = reopened.nodes(second_generation)?;
    let second_edges = reopened.edges(second_generation)?;
    if !second_nodes
        .iter()
        .any(|node| node.id == namespace_member_id)
        || second_edges.iter().any(|edge| {
            edge.source == namespace_member_id
                && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
        })
    {
        return Err(std::io::Error::other(
            "Turso target removal did not retain namespace source evidence while retracting stale resolution",
        )
        .into());
    }
    let historical = reopened.historical_snapshot(first_generation)?;
    if !historical.nodes.iter().any(|node| node.id == target_id)
        || !historical.edges.iter().any(|edge| edge == &resolution_edge)
        || !historical.edges.iter().any(|edge| edge == &binding_edge)
        || !historical
            .nodes
            .iter()
            .any(|node| node.id == namespace_member_id)
        || !historical
            .edges
            .iter()
            .any(|edge| edge == &namespace_binding_edge)
    {
        return Err(std::io::Error::other(
            "reopened Turso GraphAt did not preserve the original module resolution",
        )
        .into());
    }
    drop(reopened);

    let historical_node = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("node-at-turso")
        .arg(&database)
        .arg(first_generation.0.to_hex())
        .arg(target_id.0.to_hex())
        .output()?;
    let historical_node_output = String::from_utf8_lossy(&historical_node.stdout);
    if !historical_node.status.success()
        || !historical_node_output.contains("src/barrel.ts")
        || !historical_node_output.contains(&first_generation.0.to_hex())
    {
        return Err(std::io::Error::other(format!(
            "historical node point query failed: stdout={historical_node_output}, stderr={}",
            String::from_utf8_lossy(&historical_node.stderr)
        ))
        .into());
    }

    let diagnostics = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("resolution-diagnostics-turso")
        .arg(&database)
        .output()?;
    if !diagnostics.status.success() {
        return Err(std::io::Error::other(format!(
            "Turso resolution diagnostics command failed: {}",
            String::from_utf8_lossy(&diagnostics.stderr)
        ))
        .into());
    }
    let diagnostic_records = String::from_utf8(diagnostics.stdout)?
        .lines()
        .map(serde_json::from_str::<GraphRecord>)
        .collect::<Result<Vec<_>, _>>()?;
    if !diagnostic_records.iter().any(|record| {
        matches!(record, GraphRecord::Node { node, .. }
            if matches!(node.kind,
                syntaxmesh_core::NodeKind::ModuleResolutionDiagnostic {
                    occurrence,
                    status: syntaxmesh_core::ModuleResolutionDiagnosticStatus::Unresolved,
                    ..
                } if occurrence == import_id))
    }) {
        return Err(std::io::Error::other(
            "reopened Turso diagnostic query omitted the target-removal outcome",
        )
        .into());
    }

    let graph_at = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("graph-at-turso")
        .arg(&database)
        .arg(first_generation.0.to_hex())
        .output()?;
    if !graph_at.status.success() {
        return Err(std::io::Error::other(format!(
            "Turso GraphAt command failed: {}",
            String::from_utf8_lossy(&graph_at.stderr)
        ))
        .into());
    }
    let records = String::from_utf8(graph_at.stdout)?
        .lines()
        .map(serde_json::from_str::<GraphRecord>)
        .collect::<Result<Vec<_>, _>>()?;
    if !records.iter().any(
        |record| matches!(record, GraphRecord::Edge { edge, .. } if edge.id == resolution_edge.id),
    ) || !records.iter().any(
        |record| matches!(record, GraphRecord::Edge { edge, .. } if edge.id == binding_edge.id),
    ) || !records.iter().any(
        |record| matches!(record, GraphRecord::Edge { edge, .. } if edge.id == namespace_binding_edge.id),
    ) {
        return Err(std::io::Error::other(
            "Turso GraphAt export omitted a historical barrel or export-binding edge",
        )
        .into());
    }
    Ok(())
}

#[test]
fn git_context_reports_branch_head_dirty_and_detached_states() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let run_git = |arguments: &[&str]| -> Result<(), Box<dyn Error>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&fixture.path)
            .args(arguments)
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(String::from_utf8_lossy(&output.stderr)).into());
        }
        Ok(())
    };
    run_git(&["init", "--initial-branch=feature/context"])?;
    run_git(&["config", "user.name", "SyntaxMesh Test"])?;
    run_git(&["config", "user.email", "syntaxmesh-test@example.invalid"])?;
    run_git(&["add", "."])?;
    run_git(&["commit", "-m", "fixture"])?;

    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("git-context")
            .arg(&fixture.path)
            .output()
    };
    let clean = invoke()?;
    let clean_text = String::from_utf8(clean.stdout)?;
    if !clean.status.success()
        || !clean_text.contains("repository=true")
        || !clean_text.contains("branch=feature/context")
        || !clean_text.contains("head=")
        || !clean_text.contains("dirty=false")
    {
        return Err(
            std::io::Error::other(format!("unexpected clean Git context: {clean_text}")).into(),
        );
    }
    fs::write(fixture.path.join("src/main.rs"), "pub fn changed() {}\n")?;
    let dirty = invoke()?;
    if !String::from_utf8(dirty.stdout)?.contains("dirty=true") {
        return Err(std::io::Error::other("Git context missed a dirty worktree").into());
    }
    run_git(&["checkout", "--detach"])?;
    let detached = invoke()?;
    if !String::from_utf8(detached.stdout)?.contains("branch=detached") {
        return Err(std::io::Error::other("Git context missed detached HEAD").into());
    }

    let outside = FixtureDirectory::new()?;
    // TMPDIR may be inside a repository. This fixture deliberately tests a
    // non-repository directory, so prevent discovery of an unrelated parent.
    let discovery_boundary = outside.path.parent().ok_or("fixture has no parent")?;
    let not_repository = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .env("GIT_CEILING_DIRECTORIES", discovery_boundary)
        .arg("git-context")
        .arg(&outside.path)
        .output()?;
    if !not_repository.status.success()
        || !String::from_utf8(not_repository.stdout)?.contains("repository=false")
    {
        return Err(
            std::io::Error::other("non-repository context was not reported cleanly").into(),
        );
    }
    Ok(())
}

#[test]
fn cli_init_creates_requested_project_policies_without_overwriting() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let config = fixture.path.join("syntaxmesh.toml");
    let initialized = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("init")
        .arg("--verified-history")
        .arg("--node-module-resolution")
        .current_dir(&fixture.path)
        .output()?;
    let contents = fs::read_to_string(&config)?;
    let output_text = String::from_utf8(initialized.stdout)?;
    if !initialized.status.success()
        || !output_text.contains("verified_history=true")
        || !output_text.contains("node_module_resolution=true")
        || contents != "[history]\nverified = true\n\n[module_resolution]\nprofile = \"node\"\n"
    {
        return Err(std::io::Error::other(format!(
            "init did not create the requested project policy: contents={contents:?}, stderr={}",
            String::from_utf8_lossy(&initialized.stderr)
        ))
        .into());
    }
    let repeated = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("init")
        .current_dir(&fixture.path)
        .output()?;
    if repeated.status.success() || fs::read_to_string(config)? != contents {
        return Err(std::io::Error::other(
            "repeated init overwrote an existing project configuration",
        )
        .into());
    }
    let mixed = FixtureDirectory::new()?;
    let initialized_mixed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("init")
        .arg("--node-module-resolution")
        .arg("--python-module-resolution")
        .current_dir(&mixed.path)
        .output()?;
    let mixed_config = fs::read_to_string(mixed.path.join("syntaxmesh.toml"))?;
    if !initialized_mixed.status.success()
        || mixed_config
            != "[history]\nverified = false\n\n[module_resolution]\nprofiles = [\"node\", \"python\"]\n"
    {
        return Err(std::io::Error::other(format!(
            "init did not configure independent mixed-language profiles: {mixed_config:?}"
        ))
        .into());
    }
    Ok(())
}

#[test]
fn cli_indexes_bash_markdown_and_text_through_the_rust_engine() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    fs::create_dir_all(fixture.path.join("scripts"))?;
    fs::create_dir_all(fixture.path.join("docs"))?;
    fs::write(
        fixture.path.join("scripts/release.sh"),
        "release() { cargo build; }\nrelease\n",
    )?;
    fs::write(
        fixture.path.join("docs/architecture.md"),
        "# Architecture\n\n## Runtime neutral\n\n`bash` is text here, not a command. See [notes](notes.txt) and [missing](missing.md).\n",
    )?;
    fs::write(fixture.path.join("docs/notes.txt"), "Plain-text notes.\n")?;
    fs::write(
        fixture.path.join("docs/overview.rst"),
        "Overview\n========\n",
    )?;
    let snapshot = fixture.path.join("graph.snapshot");
    let indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&snapshot)
        .output()?;
    if !indexed.status.success() {
        return Err(std::io::Error::other(format!(
            "Bash/document CLI indexing failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&indexed.stdout),
            String::from_utf8_lossy(&indexed.stderr)
        ))
        .into());
    }
    let store = FileGraphStore::open(&snapshot)?;
    let generation = store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("Bash/document inputs were not published"))?;
    let nodes = store.nodes(generation.generation)?;
    let scripts = nodes
        .iter()
        .filter(|node| node.kind == syntaxmesh_core::NodeKind::Script)
        .count();
    let documents = nodes
        .iter()
        .filter(|node| node.kind == syntaxmesh_core::NodeKind::Document)
        .count();
    let sections = nodes
        .iter()
        .filter(|node| node.kind == syntaxmesh_core::NodeKind::Section)
        .count();
    let functions = nodes
        .iter()
        .filter(|node| node.kind == syntaxmesh_core::NodeKind::Function)
        .count();
    let document_chunks = nodes
        .iter()
        .filter(|node| node.kind == syntaxmesh_core::NodeKind::DocumentChunk)
        .count();
    let heading_anchors = nodes
        .iter()
        .filter(|node| syntaxmesh_language_sdk::is_source_anchor(&node.kind))
        .count();
    let contains_edges = store
        .edges(generation.generation)?
        .into_iter()
        .filter(|edge| edge.relation == syntaxmesh_core::RelationKind::Contains)
        .count();
    let resolved_doc_link = nodes.iter().find(|node| {
        node.name == "docs/notes.txt"
            && node.kind
                == (syntaxmesh_core::NodeKind::Reference {
                    relation: syntaxmesh_core::RelationKind::References,
                })
    });
    let unresolved_doc_link = nodes.iter().any(|node| {
        node.name == "docs/missing.md"
            && node.kind
                == (syntaxmesh_core::NodeKind::UnresolvedReference {
                    relation: syntaxmesh_core::RelationKind::References,
                })
    });
    let links_to_notes = store.edges(generation.generation)?.into_iter().any(|edge| {
        resolved_doc_link.is_some_and(|link| {
            edge.source == link.id
                && edge.relation == syntaxmesh_core::RelationKind::ResolvesTo
                && nodes.iter().any(|node| {
                    node.id == edge.target
                        && node.kind == syntaxmesh_core::NodeKind::Document
                        && node.name == "docs/notes.txt"
                })
        })
    });
    if (
        scripts,
        documents,
        sections,
        functions,
        document_chunks,
        heading_anchors,
        contains_edges,
    ) != (1, 3, 2, 3, 3, 2, 8)
        || !links_to_notes
        || !unresolved_doc_link
    {
        return Err(std::io::Error::other(format!(
            "unexpected Bash/document graph facts: scripts={scripts}, documents={documents}, sections={sections}, functions={functions}, contains={contains_edges}"
        ))
        .into());
    }
    Ok(())
}

#[test]
fn project_policy_and_cli_overrides_drive_statechronicle_on_both_hosts()
-> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let config = fixture.path.join("syntaxmesh.toml");
    fs::write(&config, "[history]\nverified = true\n")?;

    let verified_snapshot = fixture.path.join("configured-verified.snapshot");
    let verified_file = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&verified_snapshot)
        .output()?;
    if !verified_file.status.success()
        || !String::from_utf8(verified_file.stdout)?.contains("(Verified)")
    {
        return Err(std::io::Error::other(format!(
            "project verification policy did not enable File history: {}",
            String::from_utf8_lossy(&verified_file.stderr)
        ))
        .into());
    }
    let file_audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("statechronicle-verify")
        .arg(&verified_snapshot)
        .output()?;
    if !file_audit.status.success()
        || !String::from_utf8(file_audit.stdout)?.contains("statechronicle_history=verified")
    {
        return Err(std::io::Error::other("configured File history failed full audit").into());
    }

    fs::write(&config, "[history]\nverified = false\n")?;
    let durable_database = fixture.path.join("configured-durable.db");
    migrate_turso_cli(&durable_database)?;
    let durable_turso = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&fixture.path)
        .arg(&durable_database)
        .output()?;
    if !durable_turso.status.success()
        || !String::from_utf8(durable_turso.stdout)?.contains("(Durable)")
    {
        return Err(std::io::Error::other(
            "disabled project policy did not preserve durable-only Turso publication",
        )
        .into());
    }

    let forced_database = fixture.path.join("configured-forced-verified.db");
    migrate_turso_cli(&forced_database)?;
    let forced_turso = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&fixture.path)
        .arg(&forced_database)
        .arg("--verify")
        .output()?;
    if !forced_turso.status.success()
        || !String::from_utf8(forced_turso.stdout)?.contains("(Verified)")
    {
        return Err(
            std::io::Error::other("--verify did not override the disabled project policy").into(),
        );
    }
    let turso_audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("statechronicle-verify-turso")
        .arg(&forced_database)
        .output()?;
    if !turso_audit.status.success()
        || !String::from_utf8(turso_audit.stdout)?.contains("statechronicle_history=verified")
    {
        return Err(std::io::Error::other("configured Turso history failed full audit").into());
    }

    fs::write(&config, "[history\nverified = true")?;
    let malformed_snapshot = fixture.path.join("malformed-config.snapshot");
    let malformed_index = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&malformed_snapshot)
        .arg("--verify")
        .output()?;
    if malformed_index.status.success()
        || !String::from_utf8(malformed_index.stderr)?.contains("syntaxmesh.toml")
        || malformed_snapshot.exists()
    {
        return Err(std::io::Error::other(
            "malformed project config was ignored or indexing opened the destination first",
        )
        .into());
    }
    Ok(())
}

#[test]
fn cli_indexes_and_resolves_python_class_calls() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let snapshot = fixture.path.join("python.snapshot");
    fs::write(
        fixture.path.join("models.py"),
        "class Widget:\n    pass\n\nclass Service:\n    def run(self):\n        self.build()\n    def build(self):\n        pass\n\nclass Other:\n    def build(self):\n        pass\n",
    )?;
    fs::write(
        fixture.path.join("factory.py"),
        "def make_widget():\n    return Widget()\n\ndef use_client(client):\n    return client.build()\n",
    )?;

    let indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&snapshot)
        .output()?;
    let indexed_text = String::from_utf8(indexed.stdout)?;
    if !indexed.status.success()
        || !indexed_text.contains("scan files=4")
        || !indexed_text.contains("indexed 4 files")
    {
        return Err(std::io::Error::other(format!(
            "CLI did not index both Python files: status={}, stdout={indexed_text}, stderr={}",
            indexed.status,
            String::from_utf8_lossy(&indexed.stderr)
        ))
        .into());
    }

    let exported = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("export")
        .arg(&snapshot)
        .output()?;
    let export_text = String::from_utf8(exported.stdout)?;
    if !exported.status.success()
        || !export_text.contains("\"kind\":\"Class\"")
        || !export_text.contains("\"name\":\"Widget\"")
        || !export_text.contains("\"relation\":\"ResolvesTo\"")
        || !export_text.contains("\"name\":\"client.build\"")
        || !export_text.contains("UnresolvedReference")
    {
        return Err(std::io::Error::other(format!(
            "Python class/call facts were not present in the accepted graph: status={}, stdout={export_text}, stderr={}",
            exported.status,
            String::from_utf8_lossy(&exported.stderr)
        ))
        .into());
    }

    let status = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("status")
        .arg(&snapshot)
        .arg(&fixture.path)
        .output()?;
    let status_text = String::from_utf8(status.stdout)?;
    if !status.status.success() || !status_text.contains("index_freshness=current") {
        return Err(std::io::Error::other(format!(
            "Python-aware source freshness check failed: status={}, stdout={status_text}, stderr={}",
            status.status,
            String::from_utf8_lossy(&status.stderr)
        ))
        .into());
    }
    Ok(())
}

#[test]
fn embedded_ecmascript_pack_indexes_typescript_and_javascript() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let snapshot = fixture.path.join("ecmascript.snapshot");
    fs::write(
        fixture.path.join("service.ts"),
        "import { client as transport } from './client'; export { transport as publicTransport }; export class Service { run() { this.build(); client.send(); } build() {} }\n",
    )?;
    fs::write(
        fixture.path.join("app.js"),
        "function start() { service.run(); }\n",
    )?;
    let mut extractors = CompositeExtractor::new();
    extractors.register(RustExtractor, ["rs"])?;
    extractors.register(PythonExtractor, ["py"])?;
    extractors.register(TypeScriptExtractor, ["ts", "tsx"])?;
    extractors.register(JavaScriptExtractor, ["js", "jsx", "mjs", "cjs"])?;
    extractors.register(BashExtractor, ["sh", "bash"])?;
    extractors.register(
        DocumentationExtractor,
        ["md", "markdown", "txt", "text", "rst", "adoc", "asciidoc"],
    )?;
    let files = scan(
        &fixture.path,
        syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
    )?
    .files;
    let repository = RepositoryId::derive(&[b"ecmascript-test-repository"]);
    let worktree = WorktreeId::derive(&[b"ecmascript-test-worktree"]);
    let generation = GenerationId::derive(&[b"ecmascript-test-generation"]);
    let run = IndexRunId::derive(&[b"ecmascript-test-run"]);
    let receipt = SyntaxMeshEngine::new(
        FileGraphStore::open(&snapshot)?,
        extractors,
        repository,
        worktree,
    )
    .index(&files, run, generation)?;
    if receipt.publication.generation.generation != generation {
        return Err(
            std::io::Error::other("ECMAScript fixture published an unexpected generation").into(),
        );
    }
    let exported = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("export")
        .arg(snapshot)
        .output()?;
    let output = String::from_utf8(exported.stdout)?;
    if !exported.status.success()
        || !output.contains("Service::build")
        || !output.contains("client.send")
        || !output.contains("service.run")
        || !output.contains("publicTransport")
        || !output.contains("./client")
        || !output.contains("\"Exports\"")
    {
        return Err(std::io::Error::other(format!(
            "TypeScript/JavaScript facts were missing: {output}"
        ))
        .into());
    }

    let cli_snapshot = fixture.path.join("ecmascript-cli.snapshot");
    let indexed = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&cli_snapshot)
        .output()?;
    let indexed_text = String::from_utf8(indexed.stdout)?;
    if !indexed.status.success() || !indexed_text.contains("scan files=4") {
        return Err(std::io::Error::other(format!(
            "CLI did not index TypeScript and JavaScript inputs: status={}, stdout={indexed_text}, stderr={}",
            indexed.status,
            String::from_utf8_lossy(&indexed.stderr)
        ))
        .into());
    }
    let cli_export = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("export")
        .arg(&cli_snapshot)
        .output()?;
    let cli_output = String::from_utf8(cli_export.stdout)?;
    if !cli_export.status.success()
        || !cli_output.contains("Service::build")
        || !cli_output.contains("client.send")
        || !cli_output.contains("service.run")
        || !cli_output.contains("publicTransport")
        || !cli_output.contains("./client")
        || !cli_output.contains("\"Exports\"")
    {
        return Err(std::io::Error::other(format!(
            "CLI export lost TypeScript/JavaScript facts: {cli_output}"
        ))
        .into());
    }
    Ok(())
}

#[test]
fn changeset_cli_queries_use_pinned_history_for_file_and_turso() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let snapshot = fixture.path.join("changes.snapshot");
    let database = fixture.path.join("changes.db");
    let change_set = ChangeSetId(StableId::derive("cli-change-set", &[b"group"]));
    let generation = GenerationId(StableId::derive("cli-generation", &[b"one"]));
    publish_changeset_fixture(FileGraphStore::open(&snapshot)?, generation, change_set).map_err(
        |error| std::io::Error::other(format!("file ChangeSet fixture failed: {error}")),
    )?;
    publish_changeset_fixture(open_turso(&database)?, generation, change_set).map_err(|error| {
        std::io::Error::other(format!("Turso ChangeSet fixture failed: {error}"))
    })?;

    let mut declaration_results = Vec::new();
    let mut event_results = Vec::new();
    for (store_path, suffix) in [(snapshot.as_path(), ""), (database.as_path(), "-turso")] {
        let declaration = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(format!("change-set-at{suffix}"))
            .arg(store_path)
            .arg(change_set.0.to_hex())
            .arg(generation.0.to_hex())
            .output()?;
        let declaration_text = String::from_utf8(declaration.stdout)?;
        if !declaration.status.success()
            || !declaration_text.contains("\"type\":\"change_set_version\"")
            || !declaration_text.contains("CLI query fixture")
        {
            return Err(std::io::Error::other(format!(
                "ChangeSet declaration lookup failed: status={}, stdout={declaration_text}, stderr={}",
                declaration.status,
                String::from_utf8_lossy(&declaration.stderr)
            ))
            .into());
        }
        declaration_results.push(declaration_text);

        let events = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(format!("change-set-events{suffix}"))
            .arg(store_path)
            .arg(change_set.0.to_hex())
            .arg(generation.0.to_hex())
            .arg("1")
            .output()?;
        let event_text = String::from_utf8(events.stdout)?;
        if !events.status.success()
            || !event_text.contains("\"type\":\"change_set_event\"")
            || !event_text.contains("\"type\":\"change_set_events_footer\"")
            || !event_text.contains("\"returned\":1")
        {
            return Err(std::io::Error::other(format!(
                "ChangeSet event query failed: status={}, stdout={event_text}, stderr={}",
                events.status,
                String::from_utf8_lossy(&events.stderr)
            ))
            .into());
        }
        event_results.push(event_text);

        let after_last = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(format!("change-set-events{suffix}"))
            .arg(store_path)
            .arg(change_set.0.to_hex())
            .arg(generation.0.to_hex())
            .arg("1")
            .arg(generation.0.to_hex())
            .output()?;
        let after_text = String::from_utf8(after_last.stdout)?;
        if !after_last.status.success()
            || after_text.contains("\"type\":\"change_set_event\"")
            || !after_text.contains("\"returned\":0")
        {
            return Err(std::io::Error::other(format!(
                "ChangeSet cursor did not continue from the pinned snapshot: {after_text}"
            ))
            .into());
        }
    }
    if declaration_results.first() != declaration_results.get(1)
        || event_results.first() != event_results.get(1)
    {
        return Err(std::io::Error::other(format!(
            "File and Turso ChangeSet outputs differ: declarations={declaration_results:?}, events={event_results:?}"
        ))
        .into());
    }
    Ok(())
}

fn publish_changeset_fixture<S: GraphStore + DurableRecordStore>(
    store: S,
    generation: GenerationId,
    change_set: ChangeSetId,
) -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"cli-changeset-repository"]);
    let worktree = WorktreeId::derive(&[b"cli-changeset-worktree"]);
    let run_id = IndexRunId::derive(&[b"cli-changeset-run"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"cli-changeset-provenance"]),
        producer_namespace: "test.cli-changeset".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let graph = GraphDelta {
        repository,
        worktree,
        run_id,
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance.clone()],
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let event = InMemoryGraphStore::new().change_event_for_delta(&graph)?;
    let mut engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    engine.publish_prepared_with_lineage(GraphDeltaWithLineage {
        graph,
        lineage: ChangeSetDelta {
            upsert_sets: vec![ChangeSet {
                id: change_set,
                kind: ChangeSetKind::ManualGroup,
                title: Some("CLI query fixture".to_owned()),
                originating_intent: None,
                parent_changes: Vec::new(),
                git_commits: Vec::new(),
                pull_requests: Vec::new(),
                issues: Vec::new(),
                adrs: Vec::new(),
                repositories: vec![repository],
                first_generation: generation,
                last_generation: Some(generation),
                provenance: provenance.id,
            }],
            assign_events: vec![ChangeSetMembership {
                change_set,
                event: event.id,
                provenance: provenance.id,
            }],
            unassign_events: Vec::new(),
        },
    })?;
    Ok(())
}

#[test]
fn sqlite_migration_cli_reports_read_only_status_and_explicit_upgrade() -> Result<(), Box<dyn Error>>
{
    let fixture = FixtureDirectory::new()?;
    let database = fixture.path.join("syntaxmesh.db");
    let before = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("sqlite-migration-status")
        .arg(&database)
        .output()?;
    let before_stdout = String::from_utf8(before.stdout)?;
    if !before.status.success()
        || database.exists()
        || !before_stdout.contains("sqlite_schema_version=uninitialized")
        || !before_stdout.contains("sqlite_migration=pending")
    {
        return Err(std::io::Error::other(format!(
            "SQLite status command was not read-only/pending: status={}, stdout={before_stdout}, stderr={}",
            before.status,
            String::from_utf8_lossy(&before.stderr)
        ))
        .into());
    }

    let migrated = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("sqlite-migrate")
        .arg(&database)
        .output()?;
    let migrated_stdout = String::from_utf8(migrated.stdout)?;
    if !migrated.status.success()
        || !migrated_stdout.contains("sqlite_schema_version=21")
        || !migrated_stdout.contains("sqlite_migration_ledger_validated=true")
        || !migrated_stdout.contains("sqlite_migration=applied")
    {
        return Err(std::io::Error::other(format!(
            "SQLite migrate command did not report the upgraded schema: status={}, stdout={migrated_stdout}, stderr={}",
            migrated.status,
            String::from_utf8_lossy(&migrated.stderr)
        ))
        .into());
    }
    Ok(())
}

#[test]
fn file_and_turso_cli_export_same_identity_fact_lineage() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let snapshot = fixture.path.join("lineage.snapshot");
    let database = fixture.path.join("lineage.db");
    let mut initial_ids = Vec::new();
    for (index_command, _history_command, store_path) in [
        ("index", "fact-lineage", snapshot.as_path()),
        ("index-turso", "fact-lineage-turso", database.as_path()),
    ] {
        if index_command == "index-turso" {
            migrate_turso_cli(store_path)?;
        }
        let initial = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(index_command)
            .arg(&fixture.path)
            .arg(store_path)
            .output()?;
        if !initial.status.success() {
            return Err(std::io::Error::other(format!(
                "{index_command} initial generation failed: {}",
                String::from_utf8_lossy(&initial.stderr)
            ))
            .into());
        }
        initial_ids.push(cli_search_node_id(
            if index_command == "index" {
                "search"
            } else {
                "search-turso"
            },
            store_path,
            "helper",
        )?);
    }

    fs::write(
        fixture.path.join("src/support.rs"),
        "pub fn helper() { let answer = 42; let _ = answer; }\n",
    )?;
    let mut lineages = Vec::new();
    for (index_command, history_command, store_path) in [
        ("index", "fact-lineage", snapshot.as_path()),
        ("index-turso", "fact-lineage-turso", database.as_path()),
    ] {
        let update = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(index_command)
            .arg(&fixture.path)
            .arg(store_path)
            .output()?;
        if !update.status.success() {
            return Err(std::io::Error::other(format!(
                "{index_command} update generation failed: {}",
                String::from_utf8_lossy(&update.stderr)
            ))
            .into());
        }
        let node_id = cli_search_node_id(
            if index_command == "index" {
                "search"
            } else {
                "search-turso"
            },
            store_path,
            "helper",
        )?;
        if initial_ids.get(lineages.len()) != Some(&node_id) {
            return Err(std::io::Error::other(format!(
                "Rust extractor changed helper identity across a body edit: before={initial_ids:?}, after={node_id}"
            ))
            .into());
        }
        let lineage = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(history_command)
            .arg(store_path)
            .arg("node")
            .arg(node_id)
            .output()?;
        if !lineage.status.success() {
            return Err(std::io::Error::other(format!(
                "{history_command} query failed: {}",
                String::from_utf8_lossy(&lineage.stderr)
            ))
            .into());
        }
        let records = String::from_utf8(lineage.stdout)?;
        if !records.contains("\"type\":\"fact_supersedes\"") {
            let fact_history_command = if index_command == "index" {
                "fact-history"
            } else {
                "fact-history-turso"
            };
            let fact_history = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(fact_history_command)
                .arg(store_path)
                .arg("node")
                .arg(cli_search_node_id(
                    if index_command == "index" {
                        "search"
                    } else {
                        "search-turso"
                    },
                    store_path,
                    "helper",
                )?)
                .output()?;
            return Err(std::io::Error::other(format!(
                "{history_command} did not expose the indexed same-identity version link: lineage={records}; history={}",
                String::from_utf8_lossy(&fact_history.stdout)
            ))
            .into());
        }
        lineages.push(
            records
                .split(",\"accepted_at\":")
                .next()
                .unwrap_or(&records)
                .to_owned(),
        );
    }
    if lineages.first() != lineages.get(1) {
        return Err(std::io::Error::other(format!(
            "File and Turso fact lineage differ: {lineages:?}"
        ))
        .into());
    }
    Ok(())
}

struct FixtureDirectory {
    path: PathBuf,
}

impl FixtureDirectory {
    fn new() -> Result<Self, std::io::Error> {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "syntaxmesh-host-equivalence-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        fs::create_dir(path.join("src"))?;
        fs::write(
            path.join("src/main.rs"),
            // Exercise supported unique unqualified matching, not Rust module
            // resolution: separate-file crate paths are not implemented yet.
            "pub fn caller() { helper(); }\n",
        )?;
        fs::write(path.join("src/support.rs"), "pub fn helper() {}\n")?;
        Ok(Self { path })
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.path));
    }
}

fn embedded_generation(
    root: &Path,
    scan_report: &syntaxmesh_scanner::ScanReport,
    extractor: &CompositeExtractor,
) -> Result<(RepositoryId, WorktreeId, GenerationId, IndexRunId), Box<dyn Error>> {
    let canonical_root = fs::canonicalize(root)?;
    let root_text = canonical_root.to_string_lossy();
    let repository = RepositoryId::derive(&[root_text.as_bytes()]);
    let worktree = WorktreeId::derive(&[root_text.as_bytes(), b"working-tree"]);
    let mut fingerprint = syntaxmesh_engine::source_inventory_fingerprint(&scan_report.files)?;
    let resolvers = syntaxmesh_source_host::project_resolvers(
        &canonical_root,
        &syntaxmesh_source_host::ProjectConfig::default(),
    )?;
    fingerprint.extend_from_slice(&resolvers.fingerprint);
    let extractor_fingerprint = extractor.configuration_fingerprint();
    let generation = GenerationId::derive(&[
        b"source-index-v1",
        extractor_fingerprint.as_slice(),
        fingerprint.as_slice(),
    ]);
    let run = IndexRunId::derive(&[b"syntaxmesh-cli", generation.0.0.as_slice()]);
    Ok((repository, worktree, generation, run))
}

#[test]
fn cli_and_embedded_engine_export_the_same_fixture_generation() {
    let result = compare_cli_and_embedded_exports();
    assert!(result.is_ok(), "host equivalence failed: {result:?}");
}

fn compare_cli_and_embedded_exports() -> Result<(), Box<dyn Error>> {
    let fixture = FixtureDirectory::new()?;
    let snapshot = fixture.path.join("snapshot.bin");
    let index_output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&snapshot)
        .output()?;
    if !index_output.status.success() {
        return Err(std::io::Error::other(format!(
            "CLI index failed: {}",
            String::from_utf8_lossy(&index_output.stderr)
        ))
        .into());
    }
    if !String::from_utf8(index_output.stdout)?.contains("(Durable)") {
        return Err(std::io::Error::other(
            "default CLI indexing did not report the durable-only status",
        )
        .into());
    }
    let file_integrity = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("integrity")
        .arg(&snapshot)
        .output()?;
    if !file_integrity.status.success()
        || !String::from_utf8(file_integrity.stdout)?.contains("backend_integrity=ok")
    {
        return Err(std::io::Error::other(format!(
            "FileGraphStore integrity check failed: {}",
            String::from_utf8_lossy(&file_integrity.stderr)
        ))
        .into());
    }

    let database = fixture.path.join("syntaxmesh.db");
    migrate_turso_cli(&database)?;
    let turso_index = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&fixture.path)
        .arg(&database)
        .output()?;
    if !turso_index.status.success() {
        return Err(std::io::Error::other(format!(
            "Turso CLI index failed: {}",
            String::from_utf8_lossy(&turso_index.stderr)
        ))
        .into());
    }
    if !String::from_utf8(turso_index.stdout)?.contains("(Durable)") {
        return Err(std::io::Error::other(
            "default Turso indexing did not report the durable-only status",
        )
        .into());
    }
    let turso_integrity = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("integrity-turso")
        .arg(&database)
        .output()?;
    if !turso_integrity.status.success()
        || !String::from_utf8(turso_integrity.stdout)?.contains("backend_integrity=ok")
    {
        return Err(std::io::Error::other(format!(
            "Turso database integrity check failed: {}",
            String::from_utf8_lossy(&turso_integrity.stderr)
        ))
        .into());
    }

    let verified_database = fixture.path.join("verified.db");
    migrate_turso_cli(&verified_database)?;
    let verified_index = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index-turso")
        .arg(&fixture.path)
        .arg(&verified_database)
        .arg("--verify")
        .output()?;
    if !verified_index.status.success()
        || !String::from_utf8(verified_index.stdout)?.contains("(Verified)")
    {
        return Err(std::io::Error::other(format!(
            "opt-in StateChronicle CLI indexing did not report verified history: {}",
            String::from_utf8_lossy(&verified_index.stderr)
        ))
        .into());
    }
    let verified_status = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("status-turso")
        .arg(&verified_database)
        .output()?;
    if !verified_status.status.success()
        || !String::from_utf8(verified_status.stdout)?.contains("status=Verified")
    {
        return Err(std::io::Error::other(format!(
            "verified status did not survive reopening the Turso store: {}",
            String::from_utf8_lossy(&verified_status.stderr)
        ))
        .into());
    }
    let statechronicle_audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("statechronicle-verify-turso")
        .arg(&verified_database)
        .output()?;
    let audit_stdout = String::from_utf8(statechronicle_audit.stdout)?;
    if !statechronicle_audit.status.success()
        || !audit_stdout.contains("statechronicle_history=verified")
        || !audit_stdout.contains("head_digest=")
    {
        return Err(std::io::Error::other(format!(
            "full StateChronicle history audit failed: stdout={audit_stdout}, stderr={}",
            String::from_utf8_lossy(&statechronicle_audit.stderr)
        ))
        .into());
    }

    let verified_snapshot = fixture.path.join("verified.snapshot");
    let verified_file_index = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(&fixture.path)
        .arg(&verified_snapshot)
        .arg("--verify")
        .output()?;
    if !verified_file_index.status.success()
        || !String::from_utf8(verified_file_index.stdout)?.contains("(Verified)")
    {
        return Err(std::io::Error::other(format!(
            "opt-in StateChronicle File indexing failed: {}",
            String::from_utf8_lossy(&verified_file_index.stderr)
        ))
        .into());
    }
    let file_statechronicle_audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("statechronicle-verify")
        .arg(&verified_snapshot)
        .output()?;
    let file_audit_stdout = String::from_utf8(file_statechronicle_audit.stdout)?;
    if !file_statechronicle_audit.status.success()
        || !file_audit_stdout.contains("statechronicle_history=verified")
        || !file_audit_stdout.contains("head_digest=")
    {
        return Err(std::io::Error::other(format!(
            "full File StateChronicle history audit failed: stdout={file_audit_stdout}, stderr={}",
            String::from_utf8_lossy(&file_statechronicle_audit.stderr)
        ))
        .into());
    }

    let cli_export = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("export")
        .arg(&snapshot)
        .output()?;
    if !cli_export.status.success() {
        return Err(std::io::Error::other(format!(
            "CLI export failed: {}",
            String::from_utf8_lossy(&cli_export.stderr)
        ))
        .into());
    }
    let cli_status = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("status")
        .arg(&snapshot)
        .arg(&fixture.path)
        .output()?;
    if !cli_status.status.success() {
        return Err(std::io::Error::other(format!(
            "CLI status failed: {}",
            String::from_utf8_lossy(&cli_status.stderr)
        ))
        .into());
    }
    let unscanned_status = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("status")
        .arg(&snapshot)
        .output()?;
    if !unscanned_status.status.success()
        || !String::from_utf8(unscanned_status.stdout)?.contains("index_freshness=not_checked")
    {
        return Err(std::io::Error::other(
            "CLI status without a source root did not report freshness as not checked",
        )
        .into());
    }

    for (search_command, path_command, impact_command, store_path) in [
        ("search", "path", "impact", snapshot.as_path()),
        (
            "search-turso",
            "path-turso",
            "impact-turso",
            database.as_path(),
        ),
    ] {
        let caller_id = cli_search_node_id(search_command, store_path, "caller")?;
        let helper_id = cli_search_node_id(search_command, store_path, "helper")?;
        let path_output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(path_command)
            .arg(store_path)
            .arg(&caller_id)
            .arg(&helper_id)
            .arg("4")
            .output()?;
        let path_text = String::from_utf8(path_output.stdout)?;
        if !path_output.status.success()
            || !path_text.contains("caller\t")
            || !path_text.contains("helper\t")
        {
            return Err(std::io::Error::other(format!(
                "{path_command} did not return the cross-file call path: {path_text}"
            ))
            .into());
        }
        let impact_output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(impact_command)
            .arg(store_path)
            .arg("helper")
            .output()?;
        let impact_text = String::from_utf8(impact_output.stdout)?;
        if !impact_output.status.success()
            || !impact_text.contains("target=helper\t")
            || !impact_text.contains("caller\t")
        {
            return Err(std::io::Error::other(format!(
                "{impact_command} did not return the cross-file caller: {impact_text}"
            ))
            .into());
        }
    }
    let status_text = String::from_utf8(cli_status.stdout)?;
    for expected in [
        "logical_integrity=ok",
        "graph_root_matches=true",
        "references_valid=true",
        "index_freshness=current",
        "index_unindexed_files=0",
        "index_changed_files=0",
        "index_removed_files=0",
        "workflow_prepared=0",
        "workflow_completed=1",
        "workflow_rejected=0",
    ] {
        if !status_text.contains(expected) {
            return Err(std::io::Error::other(format!(
                "CLI status omitted {expected}: {status_text}"
            ))
            .into());
        }
    }
    let mismatched_root = fixture.path.join("src");
    let mismatched_scope_status = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("status")
        .arg(&snapshot)
        .arg(&mismatched_root)
        .output()?;
    if mismatched_scope_status.status.success()
        || !String::from_utf8(mismatched_scope_status.stderr)?
            .contains("does not match the repository/worktree")
    {
        return Err(std::io::Error::other(
            "CLI status compared a source inventory from a different scope",
        )
        .into());
    }

    let scan_report = scan(
        &fixture.path,
        syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
    )?;

    let mut sqlite_extractor = CompositeExtractor::new();
    sqlite_extractor.register(RustExtractor, ["rs"])?;
    sqlite_extractor.register(PythonExtractor, ["py"])?;
    sqlite_extractor.register(TypeScriptExtractor, ["ts", "tsx"])?;
    sqlite_extractor.register(JavaScriptExtractor, ["js", "jsx", "mjs", "cjs"])?;
    sqlite_extractor.register(BashExtractor, ["sh", "bash"])?;
    sqlite_extractor.register(
        DocumentationExtractor,
        ["md", "markdown", "txt", "text", "rst", "adoc", "asciidoc"],
    )?;
    let (repository, worktree, generation, run) =
        embedded_generation(&fixture.path, &scan_report, &sqlite_extractor)?;
    let sqlite_path = fixture.path.join("embedded-engine.sqlite");
    SqliteGraphStore::migrate(&sqlite_path)?;
    let mut sqlite_engine = SyntaxMeshEngine::new(
        SqliteGraphStore::open(&sqlite_path)?,
        sqlite_extractor,
        repository,
        worktree,
    );
    let sqlite_receipt = sqlite_engine.index(&scan_report.files, run, generation)?;
    if sqlite_receipt.publication.generation.generation != generation
        || !sqlite_engine
            .status()?
            .ok_or("SQLite-backed embedded engine did not publish a status")?
            .integrity
            .is_valid()
    {
        return Err(std::io::Error::other(
            "SQLite-backed embedded engine did not publish a valid expected generation",
        )
        .into());
    }
    let sqlite_records = sqlite_engine
        .query(generation)
        .export_records()?
        .iter()
        .map(|record| record.to_json_line())
        .collect::<Result<Vec<_>, _>>()?;

    let mut extractor = CompositeExtractor::new();
    extractor.register(RustExtractor, ["rs"])?;
    extractor.register(PythonExtractor, ["py"])?;
    extractor.register(TypeScriptExtractor, ["ts", "tsx"])?;
    extractor.register(JavaScriptExtractor, ["js", "jsx", "mjs", "cjs"])?;
    extractor.register(BashExtractor, ["sh", "bash"])?;
    extractor.register(
        DocumentationExtractor,
        ["md", "markdown", "txt", "text", "rst", "adoc", "asciidoc"],
    )?;
    let mut engine =
        SyntaxMeshEngine::new(InMemoryGraphStore::new(), extractor, repository, worktree);
    engine.index(&scan_report.files, run, generation)?;
    let expected = engine
        .query(generation)
        .export_records()?
        .iter()
        .map(|record| record.to_json_line())
        .collect::<Result<Vec<_>, _>>()?;
    let actual = String::from_utf8(cli_export.stdout)?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    if actual != expected || sqlite_records != expected {
        return Err(std::io::Error::other(format!(
            "host/store output differed from the embedded reference:\nCLI: {actual:#?}\nSQLite embedded engine: {sqlite_records:#?}\nInMemory embedded engine: {expected:#?}"
        ))
        .into());
    }

    fs::write(
        fixture.path.join("src/support.rs"),
        "pub fn helper() { 1 + 1; }\n",
    )?;
    fs::write(fixture.path.join("src/new.rs"), "pub fn added() {}\n")?;
    fs::remove_file(fixture.path.join("src/main.rs"))?;
    let stale_status = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("status")
        .arg(&snapshot)
        .arg(&fixture.path)
        .output()?;
    if stale_status.status.success() {
        return Err(std::io::Error::other("CLI status accepted a stale source index").into());
    }
    let stale_stdout = String::from_utf8(stale_status.stdout)?;
    let stale_stderr = String::from_utf8(stale_status.stderr)?;
    for stale_field in [
        "index_freshness=stale",
        "index_unindexed_files=1",
        "index_changed_files=1",
        "index_removed_files=1",
    ] {
        if !stale_stdout.contains(stale_field) {
            return Err(std::io::Error::other(format!(
                "CLI stale status omitted {stale_field}: {stale_stdout}"
            ))
            .into());
        }
    }
    if !stale_stderr.contains("indexed source is stale") {
        return Err(std::io::Error::other(format!(
            "CLI stale status omitted its failure diagnostic: {stale_stderr}"
        ))
        .into());
    }
    Ok(())
}

fn cli_search_node_id(
    command: &str,
    store_path: &Path,
    name: &str,
) -> Result<String, Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg(command)
        .arg(store_path)
        .arg(name)
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "{command} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let stdout = String::from_utf8(output.stdout)?;
    stdout
        .lines()
        .find(|line| {
            line.split_once('\t')
                .is_some_and(|(found, _)| found == name)
        })
        .and_then(|line| line.rsplit('\t').next())
        .map(str::to_owned)
        .ok_or_else(|| std::io::Error::other(format!("{command} found no {name} node")).into())
}
