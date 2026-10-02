use std::error::Error;
use std::io::Write;
use std::net::TcpListener;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::tempdir;

#[path = "support/semantic_http.rs"]
mod semantic_http;
use semantic_http::read_request;
#[path = "support/semantic_store.rs"]
mod semantic_store;
use semantic_store::StoreHost;

const QUOTE: &str = "SyntaxMesh uses Penelope for durable indexing.";

enum Reply {
    Metadata(char),
    Completion,
}

fn require_success(output: Output, expected: &[&str]) -> Result<(), Box<dyn Error>> {
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success() || expected.iter().any(|part| !stdout.contains(part)) {
        return Err(std::io::Error::other(format!(
            "semantic CLI failed: {stdout}; {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}

#[test]
fn discovered_revisions_control_cli_cache_history_and_publication() -> Result<(), Box<dyn Error>> {
    revision_scenario(StoreHost::File)
}

#[test]
fn discovered_revisions_preserve_verified_turso_history_and_cache() -> Result<(), Box<dyn Error>> {
    revision_scenario(StoreHost::TursoVerified)
}

fn revision_scenario(host: StoreHost) -> Result<(), Box<dyn Error>> {
    let fixture = tempdir()?;
    host.prepare(fixture.path())?;
    std::fs::write(
        fixture.path().join("architecture.md"),
        format!("# Architecture\n\n{QUOTE}\n"),
    )?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let replies = [
            Reply::Metadata('a'),
            Reply::Completion,
            Reply::Metadata('a'),
            Reply::Metadata('a'),
            Reply::Metadata('a'),
            Reply::Metadata('a'),
            Reply::Metadata('b'),
            Reply::Completion,
            Reply::Metadata('b'),
            Reply::Metadata('b'),
            Reply::Metadata('b'),
            Reply::Metadata('b'),
            Reply::Metadata('c'),
            Reply::Completion,
            Reply::Metadata('c'),
            Reply::Metadata('d'),
        ];
        for reply in replies {
            let deadline = Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if deadline.elapsed() > Duration::from_secs(5) {
                            return Err(std::io::Error::other("semantic CLI fixture timed out"));
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error),
                }
            };
            let (path, request) = read_request(&mut stream)?;
            let response = match reply {
                Reply::Metadata(digest) => {
                    if path != "GET /api/tags HTTP/1.1" {
                        return Err(std::io::Error::other("unexpected metadata request"));
                    }
                    json!({"models":[{"name":"qwen3:latest", "digest":digest.to_string().repeat(64)}]})
                }
                Reply::Completion => {
                    if path != "POST /v1/chat/completions HTTP/1.1" {
                        return Err(std::io::Error::other("unexpected inference request"));
                    }
                    let message = request.get("messages").and_then(|messages| messages.get(1))
                        .and_then(|message| message.get("content")).and_then(Value::as_str)
                        .ok_or_else(|| std::io::Error::other("missing prompt"))?;
                    let input: Value = serde_json::from_str(message).map_err(std::io::Error::other)?;
                    let hash = input.get("requests").and_then(|requests| requests.get(0))
                        .and_then(|group| group.get("chunks")).and_then(|chunks| chunks.get(0))
                        .and_then(|chunk| chunk.get("content_hash")).and_then(Value::as_str)
                        .ok_or_else(|| std::io::Error::other("missing source chunk hash"))?;
                    let claims = json!({"claims":[{"subject":"SyntaxMesh", "relation":"uses", "object":"Penelope", "evidence":[{"chunk_content_hash":hash,"quote":QUOTE}]}]});
                    json!({"choices":[{"finish_reason":"stop", "message":{"content":claims.to_string()}}], "usage":{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120}})
                }
            }.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )?;
        }
        Ok(())
    });
    require_success(
        host.run(fixture.path(), Some(&endpoint), &[])?,
        &[
            "provider_requests=1",
            "metadata_requests=3",
            "claims=1",
            "semantic_usage reports=1 missing=0 invalid=0",
            "reported_prompt_tokens=100",
            "reported_completion_tokens=20",
            "reported_total_tokens=120",
        ],
    )?;
    let first = host.open(fixture.path())?;
    let first_generation = host.latest(first.as_ref())?.generation;
    let first_history = first.generation_history()?.len();
    drop(first);
    require_success(
        host.run(fixture.path(), Some(&endpoint), &[])?,
        &[
            "provider_requests=0",
            "metadata_requests=2",
            "cache_reused=1",
            "already current",
            "semantic_usage reports=0 missing=0 invalid=0",
            "reported_total_tokens=0",
        ],
    )?;
    let repeated = host.open(fixture.path())?;
    if host.latest(repeated.as_ref())?.generation != first_generation
        || repeated.generation_history()?.len() != first_history
    {
        return Err(std::io::Error::other("unchanged digest changed graph history").into());
    }
    drop(repeated);
    require_success(
        host.run(fixture.path(), Some(&endpoint), &[])?,
        &[
            "provider_requests=1",
            "metadata_requests=3",
            "cache_reused=0",
            "claims=1",
            "semantic_usage reports=1 missing=0 invalid=0",
            "reported_total_tokens=120",
        ],
    )?;
    let changed = host.open(fixture.path())?;
    let changed_generation = host.latest(changed.as_ref())?.generation;
    let changed_history = changed.generation_history()?.len();
    let old = changed.historical_snapshot(first_generation)?;
    let new = changed.historical_snapshot(changed_generation)?;
    if changed_generation == first_generation
        || changed_history != first_history.saturating_add(1)
        || !old.provenance.iter().any(|record| {
            record
                .producer_version
                .contains(&format!("ollama:sha256:{}", "a".repeat(64)))
        })
        || !new.provenance.iter().any(|record| {
            record
                .producer_version
                .contains(&format!("ollama:sha256:{}", "b".repeat(64)))
        })
    {
        return Err(std::io::Error::other(
            "changed digest did not update semantic provenance/history",
        )
        .into());
    }
    drop(changed);
    require_success(
        host.run(fixture.path(), Some(&endpoint), &[])?,
        &["provider_requests=0", "cache_reused=1", "already current"],
    )?;
    let after_repeat = host.open(fixture.path())?;
    if host.latest(after_repeat.as_ref())?.generation != changed_generation
        || after_repeat.generation_history()?.len() != changed_history
    {
        return Err(std::io::Error::other("cached model revision advanced graph history").into());
    }
    drop(after_repeat);
    let failed = host.run(fixture.path(), Some(&endpoint), &[])?;
    server
        .join()
        .map_err(|_panic| std::io::Error::other("semantic revision fixture panicked"))??;
    let after_failure = host.open(fixture.path())?;
    if failed.status.success()
        || !String::from_utf8_lossy(&failed.stderr).contains("revision changed")
        || !String::from_utf8_lossy(&failed.stdout)
            .contains("semantic_usage reports=1 missing=0 invalid=0")
        || !String::from_utf8_lossy(&failed.stdout).contains("reported_total_tokens=120")
        || host.latest(after_failure.as_ref())?.generation != changed_generation
        || after_failure.generation_history()?.len() != changed_history
    {
        return Err(std::io::Error::other(
            "late revision change published an invalid semantic generation",
        )
        .into());
    }
    drop(after_failure);
    if matches!(host, StoreHost::TursoVerified) {
        let audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("statechronicle-verify-turso")
            .arg(host.path(fixture.path()))
            .output()?;
        require_success(audit, &["statechronicle_history=verified"])?;
    }
    Ok(())
}
