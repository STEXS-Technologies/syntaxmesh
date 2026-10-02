//! Shared retained semantic evaluation; transports use the normal CLI host.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::json;
use syntaxmesh_core::NodeKind;
use syntaxmesh_store::{FileGraphStore, GraphStore};

use super::semantic_eval_config::EvaluationConfig;
use super::semantic_review;

pub(super) const ARCHITECTURE: &str = "# Architecture\n\nSyntaxMesh uses Penelope for durable indexing workflows so interrupted work can resume.\n\nSyntaxMesh supports optional StateChronicle verification for accepted graph generations.\n";
pub(super) const BOUNDARIES: &str = "# Ownership\n\nThe Change Engine owns repository mutation workflows. SyntaxMesh supplies read-only engineering evidence.\n\nCore model crates exclude database and transport dependencies so the engine remains runtime agnostic.\n";
pub(super) const OWNERSHIP_EDIT: &str =
    "\nRepository mutation never runs inside the SyntaxMesh evidence engine.\n";

pub(super) fn require_success(output: &Output) -> Result<(), Box<dyn Error>> {
    if !output.status.success() {
        return Err(std::io::Error::other(
            "live semantic evaluation failed; inspect retained CLI logs",
        )
        .into());
    }
    Ok(())
}

fn invoke(root: &Path, arguments: &[String], label: &str) -> Result<Output, Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .current_dir(root)
        .args(arguments)
        .output()?;
    fs::write(root.join(format!("{label}.stdout.txt")), &output.stdout)?;
    fs::write(root.join(format!("{label}.stderr.txt")), &output.stderr)?;
    require_success(&output)?;
    Ok(output)
}

pub(super) fn run(config: EvaluationConfig) -> Result<serde_json::Value, Box<dyn Error>> {
    let EvaluationConfig {
        model,
        transport,
        cross_document,
        mut arguments,
    } = config;
    let evidence_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/semantic-provider-results");
    fs::create_dir_all(&evidence_root)?;
    let root = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&evidence_root)?
        .keep();
    println!("live semantic evidence: {}", root.display());
    let source = root.join("source");
    fs::create_dir_all(source.join("docs"))?;
    fs::write(source.join("docs/architecture.md"), ARCHITECTURE)?;
    fs::write(source.join("docs/ownership.md"), BOUNDARIES)?;
    let snapshot = root.join("index.snapshot");
    arguments.insert(1, snapshot.to_string_lossy().into_owned());
    arguments.insert(1, source.to_string_lossy().into_owned());
    let started = std::time::Instant::now();
    let first = invoke(&root, &arguments, "initial")?;
    let inference_elapsed = started.elapsed();
    let store = FileGraphStore::open(&snapshot)?;
    let generation = store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("live evaluation did not publish a generation"))?
        .generation;
    let history_len = store.generation_history()?.len();
    let claim_review = semantic_review::accepted_claims(&store, generation, &source)?;
    let initial_source = root.join("initial-source");
    fs::create_dir_all(initial_source.join("docs"))?;
    fs::write(initial_source.join("docs/architecture.md"), ARCHITECTURE)?;
    fs::write(initial_source.join("docs/ownership.md"), BOUNDARIES)?;
    let claims = store.nodes(generation)?.into_iter()
        .filter(|node| matches!(&node.kind, NodeKind::External { namespace, kind } if namespace == "syntaxmesh.semantic" && kind == "claim"))
        .map(|node| node.name).collect::<Vec<_>>();
    let provenance = store
        .historical_snapshot(generation)?
        .provenance
        .into_iter()
        .filter(|record| record.producer_namespace.starts_with("syntaxmesh.semantic"))
        .map(|record| record.producer_version)
        .collect::<Vec<_>>();
    let cached_started = std::time::Instant::now();
    let repeated = invoke(&root, &arguments, "cached")?;
    let cached_elapsed = cached_started.elapsed();
    let repeated_store = FileGraphStore::open(&snapshot)?;
    let cached_stdout = String::from_utf8_lossy(&repeated.stdout);
    let expected_cache_units = if cross_document { 3 } else { 2 };
    let cache_stable = cached_stdout
        .split_whitespace()
        .any(|field| field == "provider_requests=0")
        && cached_stdout
            .split_whitespace()
            .any(|field| field == format!("cache_reused={expected_cache_units}"))
        && repeated_store
            .latest_generation()
            .map(|manifest| manifest.generation)
            == Some(generation)
        && repeated_store.generation_history()?.len() == history_len;
    let edited_ownership = format!("{BOUNDARIES}{OWNERSHIP_EDIT}");
    fs::write(source.join("docs/ownership.md"), &edited_ownership)?;
    let edited_started = std::time::Instant::now();
    let edited = invoke(&root, &arguments, "edited")?;
    let edited_elapsed = edited_started.elapsed();
    let edited_store = FileGraphStore::open(&snapshot)?;
    let edited_generation = edited_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("edited evaluation lacks a generation"))?
        .generation;
    let edited_history_len = edited_store.generation_history()?.len();
    let edited_claim_review =
        semantic_review::accepted_claims(&edited_store, edited_generation, &source)?;
    let edited_stdout = String::from_utf8_lossy(&edited.stdout);
    let edit_cache_selective = edited_stdout
        .split_whitespace()
        .any(|field| field == "cache_reused=1")
        && edited_stdout.split_whitespace().any(|field| {
            field
                .strip_prefix("provider_requests=")
                .and_then(|value| value.parse::<usize>().ok())
                .is_some_and(|value| value > 0)
        })
        && edited_generation != generation
        && edited_history_len > history_len
        && semantic_review::accepted_claims(&edited_store, generation, &initial_source)?
            == claim_review;
    let edited_cached_started = std::time::Instant::now();
    let edited_cached = invoke(&root, &arguments, "edited-cached")?;
    let edited_cached_elapsed = edited_cached_started.elapsed();
    let edited_cached_store = FileGraphStore::open(&snapshot)?;
    let edited_cached_stdout = String::from_utf8_lossy(&edited_cached.stdout);
    let edited_cache_stable = edited_cached_stdout
        .split_whitespace()
        .any(|field| field == "provider_requests=0")
        && edited_cached_stdout
            .split_whitespace()
            .any(|field| field == format!("cache_reused={expected_cache_units}"))
        && edited_cached_store
            .latest_generation()
            .map(|manifest| manifest.generation)
            == Some(edited_generation)
        && edited_cached_store.generation_history()?.len() == edited_history_len;
    let markers = ["penelope", "statechronicle", "change engine"];
    let joint_source_claims = claim_review
        .iter()
        .filter(|review| {
            review
                .get("evidence")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|evidence| {
                    evidence
                        .iter()
                        .filter_map(|item| {
                            item.get("source_path").and_then(serde_json::Value::as_str)
                        })
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        > 1
                })
        })
        .count();
    let label_presence = markers
        .into_iter()
        .map(|marker| {
            (
                marker,
                claims
                    .iter()
                    .any(|claim| claim.to_lowercase().contains(marker)),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let report = json!({
        "schema_version":5, "model":model, "transport":transport,
        "usage_scope":if transport == "local-command" { "unavailable: final-JSON-only command transport" } else { "provider-reported usage in retained logs; may be missing" },
        "cross_document":cross_document, "expected_cache_units":expected_cache_units,
        "joint_source_claims":joint_source_claims,
        "architecture_hash":blake3::hash(ARCHITECTURE.as_bytes()).to_hex().to_string(),
        "ownership_hash":blake3::hash(BOUNDARIES.as_bytes()).to_hex().to_string(),
        "inference_elapsed_us":inference_elapsed.as_micros().to_string(),
        "cached_elapsed_us":cached_elapsed.as_micros().to_string(),
        "edited_elapsed_us":edited_elapsed.as_micros().to_string(),
        "edited_cached_elapsed_us":edited_cached_elapsed.as_micros().to_string(),
        "edited_ownership_hash":blake3::hash(edited_ownership.as_bytes()).to_hex().to_string(),
        "initial_source_directory":"initial-source", "edited_source_directory":"source",
        "edit_cache_selective":edit_cache_selective, "edited_cache_stable":edited_cache_stable,
        "edited_claim_review":edited_claim_review,
        "initial_stdout_bytes":first.stdout.len(), "cache_stable":cache_stable,
        "claims":claims, "producer_versions":provenance, "label_presence":label_presence,
        "claim_review":claim_review,
        "quality_scope":"controlled-fixture label presence and joint-source support counts only; timings include indexing/publication; review stored evidence and relations"
    });
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string(&report)?);
    if !cache_stable || !edit_cache_selective || !edited_cache_stable {
        return Err(
            std::io::Error::other("live provider cache/generation stability failed").into(),
        );
    }
    Ok(report)
}
