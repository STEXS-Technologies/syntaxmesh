use std::error::Error;
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use syntaxmesh_core::NodeKind;

#[path = "support/semantic_http.rs"]
mod semantic_http;
#[path = "support/semantic_review.rs"]
mod semantic_review;
#[path = "support/semantic_store.rs"]
mod semantic_store;
use semantic_store::StoreHost;

const QUOTE: &str = "SyntaxMesh uses Penelope for durable indexing.";
const REVISION: &[&str] = &["--semantic-model-revision=fixture-lifecycle"];
const OFFLINE: &[&str] = &[
    "--semantic-model-revision=fixture-lifecycle",
    "--semantic-offline",
];

fn require_success(output: Output, expected: &[&str]) -> Result<(), Box<dyn Error>> {
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success() || expected.iter().any(|part| !stdout.contains(part)) {
        return Err(std::io::Error::other(format!(
            "semantic lifecycle failed: {stdout}; {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}

#[test]
fn file_semantic_cache_rebinds_shared_evidence_after_delete_and_restore()
-> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::File)
}

#[test]
fn verified_turso_semantic_cache_rebinds_shared_evidence_after_delete_and_restore()
-> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::TursoVerified)
}

fn lifecycle(host: StoreHost) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    host.prepare(root)?;
    fs::create_dir(root.join("docs"))?;
    fs::write(
        root.join("docs/a.md"),
        format!("# First evidence\n\n{QUOTE}\n"),
    )?;
    fs::write(
        root.join("docs/b.md"),
        format!("# Second evidence\n\n{QUOTE}\n"),
    )?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let started = Instant::now();
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && started.elapsed() < Duration::from_secs(5) =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => return Err(error),
            }
        };
        let (path, request) = semantic_http::read_request(&mut stream)?;
        if path != "POST /v1/chat/completions HTTP/1.1" {
            return Err(std::io::Error::other("unexpected lifecycle request"));
        }
        let prompt = request
            .get("messages")
            .and_then(|messages| messages.get(1))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| std::io::Error::other("missing lifecycle prompt"))?;
        let input: Value = serde_json::from_str(prompt).map_err(std::io::Error::other)?;
        let groups = input
            .get("requests")
            .and_then(Value::as_array)
            .ok_or_else(|| std::io::Error::other("missing lifecycle request groups"))?;
        if groups.len() != 2 {
            return Err(std::io::Error::other(
                "lifecycle must cache two independent documents",
            ));
        }
        let mut claims = Vec::new();
        for group in groups {
            let chunks = group
                .get("chunks")
                .and_then(Value::as_array)
                .ok_or_else(|| std::io::Error::other("missing lifecycle chunks"))?;
            let [chunk] = chunks.as_slice() else {
                return Err(std::io::Error::other("unexpected lifecycle chunk count"));
            };
            let hash = chunk
                .get("content_hash")
                .and_then(Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing lifecycle chunk hash"))?;
            if !chunk
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| text.contains(QUOTE))
            {
                return Err(std::io::Error::other(
                    "lifecycle quote missing from request",
                ));
            }
            claims.push(json!({"subject":"SyntaxMesh","relation":"uses","object":"Penelope", "evidence":[{"chunk_content_hash":hash,"quote":QUOTE}]}));
        }
        let body = json!({"choices":[{"finish_reason":"stop","message":{"content":json!({"claims":claims}).to_string()}}]}).to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )?;
        Ok(())
    });
    require_success(
        host.run(root, Some(&endpoint), REVISION)?,
        &[
            "batches=2",
            "provider_requests=1",
            "metadata_requests=0",
            "claims=1",
        ],
    )?;
    server
        .join()
        .map_err(|_panic| std::io::Error::other("lifecycle fixture panicked"))??;
    // The listener is now closed. Every subsequent CLI process must use only
    // durable cached output, including after source deletion and reintroduction.
    let original_store = host.open(root)?;
    let original_generation = host.latest(original_store.as_ref())?.generation;
    let original = original_store.historical_snapshot(original_generation)?;
    let original_review =
        semantic_review::accepted_claims(original_store.as_ref(), original_generation, root)?;
    let [original_claim] = original_review.as_slice() else {
        return Err("duplicate document assertion did not coalesce".into());
    };
    if original_claim
        .get("evidence")
        .and_then(Value::as_array)
        .map(Vec::len)
        != Some(2)
    {
        return Err("coalesced claim did not retain both document supports".into());
    }
    let concept = original.nodes.iter().find(|node| node.name == "syntaxmesh" && matches!(&node.kind, NodeKind::External { namespace, kind } if namespace == "syntaxmesh.semantic" && kind == "concept"))
        .ok_or("shared concept missing")?;
    let primary_file = concept
        .owner_file
        .ok_or("shared concept lacks its primary source")?;
    let primary = original
        .files
        .iter()
        .find(|file| file.file_id == primary_file)
        .ok_or("primary evidence file missing")?;
    let survivor = original
        .files
        .iter()
        .find(|file| file.file_id != primary_file)
        .ok_or("surviving evidence file missing")?;
    if !matches!(primary.normalized_path.as_str(), "docs/a.md" | "docs/b.md")
        || !matches!(survivor.normalized_path.as_str(), "docs/a.md" | "docs/b.md")
    {
        return Err("lifecycle deletion target is not a controlled fixture".into());
    }
    let survivor_text = fs::read_to_string(root.join(&survivor.normalized_path))?;
    drop(original_store);
    fs::remove_file(root.join(&primary.normalized_path))?;
    require_success(
        host.run(root, Some(&endpoint), OFFLINE)?,
        &[
            "batches=1",
            "provider_requests=0",
            "metadata_requests=0",
            "cache_reused=1",
            "claims=1",
        ],
    )?;
    let surviving_store = host.open(root)?;
    let surviving_generation = host.latest(surviving_store.as_ref())?.generation;
    let surviving = surviving_store.historical_snapshot(surviving_generation)?;
    let surviving_review =
        semantic_review::accepted_claims(surviving_store.as_ref(), surviving_generation, root)?;
    let [surviving_claim] = surviving_review.as_slice() else {
        return Err("deletion lost the surviving claim".into());
    };
    let supports = surviving_claim
        .get("evidence")
        .and_then(Value::as_array)
        .ok_or("surviving claim lacks evidence")?;
    let [support] = supports.as_slice() else {
        return Err("deletion retained stale support".into());
    };
    if support.get("source_path").and_then(Value::as_str) != Some(survivor.normalized_path.as_str())
        || support.get("quote").and_then(Value::as_str) != Some(QUOTE)
        || surviving
            .nodes
            .iter()
            .find(|node| node.id == concept.id)
            .and_then(|node| node.owner_file)
            != Some(survivor.file_id)
        || surviving.files.len() != 1
        || surviving_store.historical_snapshot(original_generation)? != original
    {
        return Err("shared semantic identity/evidence did not rebind or history changed".into());
    }
    let surviving_history = surviving_store.generation_history()?.len();
    drop(surviving_store);
    require_success(
        host.run(root, Some(&endpoint), OFFLINE)?,
        &["provider_requests=0", "already current"],
    )?;
    let repeated_store = host.open(root)?;
    if host.latest(repeated_store.as_ref())?.generation != surviving_generation
        || repeated_store.generation_history()?.len() != surviving_history
    {
        return Err("unchanged survivor advanced history".into());
    }
    drop(repeated_store);
    fs::remove_file(root.join(&survivor.normalized_path))?;
    require_success(
        host.run(root, Some(&endpoint), OFFLINE)?,
        &["batches=0", "provider_requests=0", "claims=0"],
    )?;
    let empty_store = host.open(root)?;
    let empty_generation = host.latest(empty_store.as_ref())?.generation;
    let empty = empty_store.historical_snapshot(empty_generation)?;
    if !empty.files.is_empty() || !semantic_review::accepted_claims(empty_store.as_ref(), empty_generation, root)?.is_empty()
        || empty.nodes.iter().any(|node| matches!(&node.kind, NodeKind::External { namespace, .. } if namespace == "syntaxmesh.semantic")) {
        return Err("last document deletion left current semantic facts".into());
    }
    let empty_history = empty_store.generation_history()?.len();
    drop(empty_store);
    require_success(
        host.run(root, Some(&endpoint), OFFLINE)?,
        &["provider_requests=0", "already current"],
    )?;
    let repeated_empty_store = host.open(root)?;
    if host.latest(repeated_empty_store.as_ref())?.generation != empty_generation
        || repeated_empty_store.generation_history()?.len() != empty_history
    {
        return Err("repeated empty indexing advanced history".into());
    }
    drop(repeated_empty_store);
    fs::write(root.join(&survivor.normalized_path), survivor_text)?;
    require_success(
        host.run(root, Some(&endpoint), OFFLINE)?,
        &["provider_requests=0", "cache_reused=1", "claims=1"],
    )?;
    let restored_store = host.open(root)?;
    let restored_generation = host.latest(restored_store.as_ref())?.generation;
    if semantic_review::accepted_claims(restored_store.as_ref(), restored_generation, root)?
        != surviving_review
        || restored_store.historical_snapshot(original_generation)? != original
    {
        return Err("restored cached evidence or original history differs".into());
    }
    drop(restored_store);
    if matches!(host, StoreHost::TursoVerified) {
        require_success(
            Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg("statechronicle-verify-turso")
                .arg(host.path(root))
                .output()?,
            &["statechronicle_history=verified"],
        )?;
    }
    Ok(())
}
