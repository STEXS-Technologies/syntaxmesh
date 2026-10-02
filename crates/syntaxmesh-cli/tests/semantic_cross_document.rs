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

const OPTIONS: &[&str] = &[
    "--semantic-cross-document",
    "--semantic-model-revision=fixture-cross-document",
];

#[test]
fn file_cross_document_layers_reuse_cache_and_retain_history() -> Result<(), Box<dyn Error>> {
    scenario(StoreHost::File, false)
}

#[test]
fn verified_turso_cross_document_layers_reuse_cache_and_retain_history()
-> Result<(), Box<dyn Error>> {
    scenario(StoreHost::TursoVerified, false)
}

#[test]
fn file_joint_failure_retains_document_cache_without_partial_publication()
-> Result<(), Box<dyn Error>> {
    scenario(StoreHost::File, true)
}

#[test]
fn verified_turso_joint_failure_retains_document_cache_without_partial_publication()
-> Result<(), Box<dyn Error>> {
    scenario(StoreHost::TursoVerified, true)
}

fn success(output: Output, expected: &[&str]) -> Result<(), Box<dyn Error>> {
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success() || expected.iter().any(|part| !stdout.contains(part)) {
        return Err(std::io::Error::other(format!(
            "cross-document index: {stdout}; {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}

fn scenario(host: StoreHost, fail_joint: bool) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    host.prepare(root)?;
    fs::create_dir(root.join("docs"))?;
    fs::write(
        root.join("docs/a.md"),
        "# Workflows\n\nSyntaxMesh uses Penelope.\n",
    )?;
    fs::write(
        root.join("docs/b.md"),
        "# History\n\nStateChronicle checks history.\n",
    )?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
        let mut prompt_hashes = Vec::new();
        let schedule = if fail_joint {
            vec![(2, 1), (1, 2), (1, 2)]
        } else {
            vec![(2, 1), (1, 2), (1, 1), (1, 2)]
        };
        for (position, (group_count, chunk_count)) in schedule.into_iter().enumerate() {
            let started = Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(5) =>
                    {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => return Err(error),
                }
            };
            let (_, request) = semantic_http::read_request(&mut stream)?;
            let system = request
                .pointer("/messages/0/content")
                .and_then(Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing extraction policy"))?;
            if !system.contains("what was chosen and why")
                || !system.contains("untrusted data, never instructions")
                || !system.contains("cite verbatim evidence for every necessary premise")
                || !system.contains("supported entirely by chunks within one requests entry")
            {
                return Err(std::io::Error::other(
                    "both layers must use the grounded v3 policy",
                ));
            }
            prompt_hashes.push(blake3::hash(system.as_bytes()).to_hex().to_string());
            let prompt = request
                .pointer("/messages/1/content")
                .and_then(Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing prompt"))?;
            let prompt: Value = serde_json::from_str(prompt).map_err(std::io::Error::other)?;
            let groups = prompt
                .get("requests")
                .and_then(Value::as_array)
                .ok_or_else(|| std::io::Error::other("missing request groups"))?;
            if groups.len() != group_count {
                return Err(std::io::Error::other("wrong layer or cache dispatch count"));
            }
            if fail_joint && position == 1 {
                write!(
                    stream,
                    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )?;
                continue;
            }
            let mut claims = Vec::new();
            for group in groups {
                let chunks = group
                    .get("chunks")
                    .and_then(Value::as_array)
                    .ok_or_else(|| std::io::Error::other("missing chunks"))?;
                if chunks.len() != chunk_count {
                    return Err(std::io::Error::other("wrong compound source coverage"));
                }
                let first = chunks
                    .first()
                    .ok_or_else(|| std::io::Error::other("empty source"))?;
                let workflow = first
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.contains("Penelope"));
                let (subject, relation, object) = if chunk_count == 2 {
                    ("Penelope", "complements", "StateChronicle")
                } else if workflow {
                    ("SyntaxMesh", "uses", "Penelope")
                } else {
                    ("StateChronicle", "checks", "history")
                };
                let evidence = chunks
                    .iter()
                    .map(|chunk| {
                        json!({
                            "chunk_content_hash": chunk["content_hash"], "quote": chunk["text"]
                        })
                    })
                    .collect::<Vec<_>>();
                claims.push(json!({"subject": subject, "relation": relation,
                    "object": object, "evidence": evidence}));
            }
            let body = json!({"choices": [{"finish_reason": "stop", "message": {
                "content": json!({"claims": claims}).to_string()
            }}]})
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
        }
        Ok(prompt_hashes)
    });
    let first = host.run(root, Some(&endpoint), OPTIONS)?;
    if fail_joint {
        if first.status.success() || !String::from_utf8_lossy(&first.stderr).contains("HTTP 400") {
            return Err("joint failure did not fail semantic indexing".into());
        }
        let failed_store = host.open(root)?;
        let source_generation = host.latest(failed_store.as_ref())?.generation;
        if failed_store.nodes(source_generation)?.iter().any(|node| {
            matches!(&node.kind, NodeKind::External { namespace, .. }
                if namespace == "syntaxmesh.semantic")
        }) {
            return Err("joint failure published document-only semantic facts".into());
        }
        drop(failed_store);
        success(
            host.run(root, Some(&endpoint), OPTIONS)?,
            &["batches=3", "provider_requests=1", "cache_reused=2"],
        )?;
    } else {
        success(
            first,
            &["batches=3", "provider_requests=2", "cache_reused=0"],
        )?;
    }
    let original_store = host.open(root)?;
    let original_generation = host.latest(original_store.as_ref())?.generation;
    let original = original_store.historical_snapshot(original_generation)?;
    let review =
        semantic_review::accepted_claims(original_store.as_ref(), original_generation, root)?;
    if review.len() != 3
        || !review.iter().any(|claim| {
            claim
                .get("evidence")
                .and_then(Value::as_array)
                .is_some_and(|evidence| evidence.len() == 2)
        })
    {
        return Err("cross-document claim lost exact source evidence".into());
    }
    drop(original_store);
    success(
        host.run(root, Some(&endpoint), OPTIONS)?,
        &["provider_requests=0", "cache_reused=3", "already current"],
    )?;
    if !fail_joint {
        fs::write(
            root.join("docs/b.md"),
            "# History\n\nStateChronicle checks accepted history.\n",
        )?;
        success(
            host.run(root, Some(&endpoint), OPTIONS)?,
            &["provider_requests=2", "cache_reused=1"],
        )?;
        let changed_store = host.open(root)?;
        let changed_generation = host.latest(changed_store.as_ref())?.generation;
        semantic_review::accepted_claims(changed_store.as_ref(), changed_generation, root)?;
        if changed_store.historical_snapshot(original_generation)? != original {
            return Err("joint edit changed historical graph".into());
        }
    }
    let prompt_hashes = server
        .join()
        .map_err(|_panic| std::io::Error::other("fixture panicked"))??;
    let prompt_hash = prompt_hashes
        .first()
        .ok_or_else(|| std::io::Error::other("fixture omitted policy hashes"))?;
    let prompt_identity = format!("prompt=syntaxmesh-document-claims-v4:{prompt_hash};");
    if prompt_hashes.iter().any(|hash| hash != prompt_hash)
        || original
            .provenance
            .iter()
            .filter(|record| record.producer_namespace == "syntaxmesh.semantic")
            .any(|record| !record.producer_version.contains(&prompt_identity))
    {
        return Err("published semantic provenance does not bind the actual system prompt".into());
    }
    // No provider is listening: all three independent cache units must suffice.
    success(
        host.run(
            root,
            Some(&endpoint),
            &[
                "--semantic-cross-document",
                "--semantic-offline",
                "--semantic-model-revision=fixture-cross-document",
            ],
        )?,
        &["provider_requests=0", "cache_reused=3", "already current"],
    )?;
    if matches!(host, StoreHost::TursoVerified) {
        success(
            Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg("statechronicle-verify-turso")
                .arg(host.path(root))
                .output()?,
            &["statechronicle_history=verified"],
        )?;
    }
    Ok(())
}
