use std::error::Error;
use std::path::Path;

use reqwest::blocking::Client;
use serde_json::json;
use syntaxmesh_api_model::{ContextItemKind, ContextPack, ContextRequest};
use syntaxmesh_context_host::{ExactTiktokenCounter, RepositorySourceReader};
use syntaxmesh_core::{GenerationId, NodeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_query::ContextTokenCounter;
use syntaxmesh_store_turso::TursoGraphStore;

pub(super) fn verify(
    client: &Client,
    base: &str,
    engine: &SyntaxMeshEngine<TursoGraphStore, RustExtractor>,
    source: &Path,
    generations: [GenerationId; 2],
    old_node: NodeId,
    tokenizer: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let endpoint = format!("{base}/api/v1/context");
    let [old, current] = generations;
    let reader = RepositorySourceReader::open(source)?;
    let counter = ExactTiktokenCounter::open(tokenizer)?;
    let document = source.join("architecture.md");
    let current_bytes = std::fs::read(&document)?;
    let old_bytes = "# History policy\n\nRetain historical evidence because architectural decisions must remain auditable.\n";
    for (generation, bytes, expected_text, warning) in [
        (
            current,
            Some(current_bytes.as_slice()),
            Some("current source bytes"),
            None,
        ),
        (
            old,
            Some(current_bytes.as_slice()),
            None,
            Some("stale_source"),
        ),
        (
            old,
            Some(old_bytes.as_bytes()),
            Some("architectural decisions must remain auditable"),
            None,
        ),
        (old, None, None, Some("source_unavailable")),
    ] {
        if let Some(bytes) = bytes {
            std::fs::write(&document, bytes)?;
        } else {
            std::fs::remove_file(&document)?;
        }
        let response = client
            .post(&endpoint)
            .json(&json!({
                "query":"History policy", "generation":generation.0.to_hex(),
                "token_budget":8192,
            }))
            .send()?;
        if response.status().as_u16() != 200 {
            return Err("documentation context request failed".into());
        }
        let body = response.text()?;
        let pack: ContextPack = serde_json::from_str(&body)?;
        let evidence = pack
            .items
            .iter()
            .filter(|item| {
                item.kind == ContextItemKind::SourceEvidence
                    && item.source_path.as_deref() == Some("architecture.md")
            })
            .collect::<Vec<_>>();
        if pack.generation != generation
            || counter.count_tokens(&body)? != pack.token_count
            || pack.token_count > pack.token_budget
            || expected_text.is_some_and(|text| {
                !evidence.iter().any(|item| {
                    item.text.contains(text) && item.line_start.is_some() && item.line_end.is_some()
                })
            })
            || warning.is_some_and(|code| {
                !evidence.is_empty() || !pack.warnings.iter().any(|item| item.code == code)
            })
        {
            return Err(format!(
                "documentation mismatch: expected={expected_text:?}, warning={warning:?}"
            )
            .into());
        }
    }
    std::fs::write(&document, &current_bytes)?;
    for generation in [None, Some(current), Some(old)] {
        let request = ContextRequest {
            query: if generation == Some(old) {
                "old_symbol"
            } else {
                "current_symbol"
            }
            .to_owned(),
            seed_nodes: if generation == Some(old) {
                vec![old_node]
            } else {
                Vec::new()
            },
            token_budget: 8192,
            max_hops: 1,
            max_candidates: 64,
        };
        let selected = engine.query(generation.unwrap_or(current));
        let expected = if generation.is_some() {
            selected.historical_context(&request, &reader, &counter.for_request())?
        } else {
            selected.context(&request, &reader, &counter.for_request())?
        };
        let response = client
            .post(&endpoint)
            .json(&json!({
                "query": request.query, "token_budget": request.token_budget,
                "seed_nodes": request.seed_nodes.iter().map(|id| id.0.to_hex()).collect::<Vec<_>>(),
                "generation": generation.map(|id| id.0.to_hex()),
            }))
            .send()?;
        if response.status().as_u16() != 200 {
            return Err("context request failed".into());
        }
        let body = response.text()?;
        let pack: ContextPack = serde_json::from_str(&body)?;
        if serde_json::to_value(&pack)? != serde_json::to_value(&expected)?
            || counter.count_tokens(&body)? != pack.token_count
            || pack.token_count > request.token_budget
            || serde_json::to_string(&pack)? != body
        {
            return Err("HTTP context differs from Engine or complete-body token count".into());
        }
    }
    for (field, value, status) in [
        ("token_budget", json!(0), 400),
        ("token_budget", json!(32769), 400),
        ("token_budget", json!(1), 413),
        ("query", json!("x".repeat(4097)), 400),
        ("seed_nodes", json!(vec!["0".repeat(64); 65]), 400),
        ("seed_nodes", json!(["bad"]), 400),
        ("generation", json!("bad"), 400),
        (
            "generation",
            json!(GenerationId::derive(&[b"absent-http-context"]).0.to_hex()),
            404,
        ),
        (
            "seed_nodes",
            json!([NodeId::derive(&[b"absent-http-context"]).0.to_hex()]),
            404,
        ),
        ("max_hops", json!(9), 400),
        ("max_candidates", json!(0), 400),
        ("max_candidates", json!(257), 400),
        ("unknown", json!(true), 400),
    ] {
        let mut body = json!({"query":"current_symbol","token_budget":8192});
        body.as_object_mut()
            .ok_or("invalid request fixture")?
            .insert(field.to_owned(), value);
        super::require_status(client.post(&endpoint).json(&body).send()?, status)?;
    }
    super::require_status(
        client
            .post(&endpoint)
            .header("content-type", "application/json")
            .body("x".repeat(65537))
            .send()?,
        413,
    )?;
    super::require_status(client.post(&endpoint).body("{}").send()?, 415)?;
    super::require_status(
        client
            .post(&endpoint)
            .header("content-type", "application/json")
            .body("{")
            .send()?,
        400,
    )?;
    super::require_status(
        client
            .post(&endpoint)
            .header("origin", "http://attacker.invalid")
            .json(&json!({"query":"x","token_budget":8192}))
            .send()?,
        403,
    )?;
    Ok(())
}
