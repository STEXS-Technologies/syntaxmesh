use std::error::Error;
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[path = "support/semantic_http.rs"]
mod semantic_http;
#[path = "support/semantic_store.rs"]
mod semantic_store;
use semantic_store::StoreHost;

#[test]
fn file_recovers_truncated_prompt_and_caches_both_documents() -> Result<(), Box<dyn Error>> {
    recover(StoreHost::File, false)
}

#[test]
fn verified_turso_recovers_truncated_prompt_and_caches_both_documents() -> Result<(), Box<dyn Error>>
{
    recover(StoreHost::TursoVerified, false)
}

#[test]
fn file_recovers_null_content_truncation() -> Result<(), Box<dyn Error>> {
    recover(StoreHost::File, true)
}

#[test]
fn verified_turso_recovers_null_content_truncation() -> Result<(), Box<dyn Error>> {
    recover(StoreHost::TursoVerified, true)
}

fn recover(host: StoreHost, null_content: bool) -> Result<(), Box<dyn Error>> {
    recovery_scenario(host, null_content, false)
}

#[test]
fn file_reuses_successful_leaf_after_sibling_failure() -> Result<(), Box<dyn Error>> {
    recovery_scenario(StoreHost::File, true, true)
}

#[test]
fn verified_turso_reuses_successful_leaf_after_sibling_failure() -> Result<(), Box<dyn Error>> {
    recovery_scenario(StoreHost::TursoVerified, true, true)
}

fn recovery_scenario(
    host: StoreHost,
    null_content: bool,
    partial_failure: bool,
) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    host.prepare(root)?;
    fs::create_dir(root.join("docs"))?;
    for file in ["a.md", "b.md"] {
        fs::write(
            root.join("docs").join(file),
            format!("# {file}\n\nSyntaxMesh uses Penelope.\n"),
        )?;
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let sizes = if partial_failure {
            vec![2, 1, 1, 1]
        } else {
            vec![2, 1, 1]
        };
        let mut failed_hash = None;
        for (request_number, expected_groups) in sizes.into_iter().enumerate() {
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
            let prompt = request
                .pointer("/messages/1/content")
                .and_then(Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing prompt"))?;
            let prompt: Value = serde_json::from_str(prompt).map_err(std::io::Error::other)?;
            let groups = prompt
                .get("requests")
                .and_then(Value::as_array)
                .ok_or_else(|| std::io::Error::other("missing request groups"))?;
            if groups.len() != expected_groups {
                return Err(std::io::Error::other("unexpected adaptive prompt size"));
            }
            let chunk = prompt
                .pointer("/requests/0/chunks/0")
                .ok_or_else(|| std::io::Error::other("missing evidence"))?;
            if partial_failure && request_number == 2 {
                failed_hash = Some(chunk["content_hash"].clone());
                write!(
                    stream,
                    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )?;
                continue;
            }
            if partial_failure
                && request_number == 3
                && failed_hash.as_ref() != Some(&chunk["content_hash"])
            {
                return Err(std::io::Error::other("retry dispatched the wrong document"));
            }
            let content = if expected_groups == 2 {
                if null_content {
                    Value::Null
                } else {
                    json!("{truncated")
                }
            } else {
                json!({"claims": [{
                    "subject": "SyntaxMesh", "relation": "uses", "object": "Penelope",
                    "evidence": [{"chunk_content_hash": chunk["content_hash"],
                        "quote": "SyntaxMesh uses Penelope."}]
                }]})
                .to_string()
                .into()
            };
            let body = json!({"choices": [{
                "finish_reason": if expected_groups == 2 { "length" } else { "stop" },
                "message": {"content": content}
            }], "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
        }
        Ok(())
    });
    let options = ["--semantic-model-revision=fixture-truncation"];
    let first = host.run(root, Some(&endpoint), &options)?;
    if !String::from_utf8_lossy(&first.stderr)
        .contains("semantic_progress pending_requests=2 prompt_groups=1 parallel_limit=4")
        || !String::from_utf8_lossy(&first.stderr).contains(if partial_failure {
            "group_requests=2 validated_requests=1 failed_requests=1"
        } else {
            "group_requests=2 validated_requests=2 failed_requests=0"
        })
    {
        return Err(std::io::Error::other("uncached work omitted progress plan").into());
    }
    let completed_output = if partial_failure {
        if first.status.success() || !String::from_utf8_lossy(&first.stderr).contains("HTTP 400") {
            return Err(std::io::Error::other("failed sibling did not fail semantic step").into());
        }
        let failed_store = host.open(root)?;
        let deterministic = host.latest(failed_store.as_ref())?;
        if failed_store
            .nodes(deterministic.generation)?
            .iter()
            .any(|node| {
                matches!(&node.kind, syntaxmesh_core::NodeKind::External { namespace, .. }
                if namespace == "syntaxmesh.semantic")
            })
        {
            return Err(std::io::Error::other("failed run published partial semantics").into());
        }
        drop(failed_store);
        host.run(root, Some(&endpoint), &options)?
    } else {
        first
    };
    let server_result = server
        .join()
        .map_err(|_panic| std::io::Error::other("fixture panicked"))?;
    let completion_stdout = String::from_utf8_lossy(&completed_output.stdout);
    if !completed_output.status.success()
        || !completion_stdout.contains(if partial_failure {
            "provider_requests=1"
        } else {
            "provider_requests=3"
        })
        || (partial_failure && !completion_stdout.contains("cache_reused=1"))
    {
        return Err(
            std::io::Error::other(String::from_utf8_lossy(&completed_output.stderr)).into(),
        );
    }
    server_result?;
    let store = host.open(root)?;
    let manifest = host.latest(store.as_ref())?;
    let claims = store
        .nodes(manifest.generation)?
        .into_iter()
        .filter(|node| {
            matches!(&node.kind,
            syntaxmesh_core::NodeKind::External { namespace, kind }
                if namespace == "syntaxmesh.semantic" && kind == "claim")
        })
        .count();
    let supports = store
        .edges(manifest.generation)?
        .into_iter()
        .filter(|edge| {
            matches!(&edge.relation,
            syntaxmesh_core::RelationKind::External { namespace, relation }
                if namespace == "syntaxmesh.semantic" && relation == "supports")
        })
        .count();
    if claims != 1 || supports != 2 {
        return Err(std::io::Error::other("recovery lost independent source evidence").into());
    }
    drop(store);
    // The provider is now closed: unchanged output must be reused without HTTP.
    let second = host.run(root, Some(&endpoint), &options)?;
    let cached_stdout = String::from_utf8_lossy(&second.stdout);
    if !second.status.success()
        || !cached_stdout.contains("provider_requests=0")
        || !cached_stdout.contains("cache_reused=2")
        || String::from_utf8_lossy(&second.stderr).contains("semantic_progress")
    {
        return Err(std::io::Error::other(String::from_utf8_lossy(&second.stderr)).into());
    }
    let reopened = host.open(root)?;
    if host.latest(reopened.as_ref())? != manifest {
        return Err(std::io::Error::other("cache reuse changed the generation").into());
    }
    Ok(())
}
