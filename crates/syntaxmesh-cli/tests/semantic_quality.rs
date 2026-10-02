use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::json;
use syntaxmesh_store::FileGraphStore;

#[path = "support/semantic_eval_config.rs"]
mod semantic_eval_config;
#[path = "support/semantic_evaluation.rs"]
mod semantic_evaluation;
use semantic_evaluation::{ARCHITECTURE, BOUNDARIES, OWNERSHIP_EDIT, require_success};

#[path = "support/semantic_http.rs"]
mod semantic_http;
#[path = "support/semantic_review.rs"]
mod semantic_review;

#[test]
fn evaluator_transport_selection_is_explicit_and_models_are_selectable()
-> Result<(), Box<dyn Error>> {
    use semantic_eval_config::EvaluationConfig;
    for (endpoint, command, expected_model, expected_transport) in [
        (
            Some("http://127.0.0.1:11434/v1".to_owned()),
            None,
            "qwen3:latest",
            "http",
        ),
        (
            None,
            Some("[\"codex\",\"exec\",\"{model}\"]".to_owned()),
            "gpt-6-luna",
            "local-command",
        ),
    ] {
        let default =
            EvaluationConfig::new(endpoint.clone(), command.clone(), None, None, false, false)?;
        let overridden = EvaluationConfig::new(
            endpoint,
            command,
            Some("selected-model".to_owned()),
            Some("selected-revision".to_owned()),
            true,
            false,
        )?;
        if default.model != expected_model
            || default.transport != expected_transport
            || overridden.model != "selected-model"
            || !overridden.cross_document
            || !overridden
                .arguments
                .iter()
                .any(|arg| arg == "selected-model")
            || !overridden
                .arguments
                .iter()
                .any(|arg| arg == "selected-revision")
        {
            return Err("evaluation model/transport selection mismatch".into());
        }
    }
    for (endpoint, command, allow_network) in [
        (None, None, false),
        (
            Some("endpoint".to_owned()),
            Some("command".to_owned()),
            false,
        ),
        (None, Some("command".to_owned()), true),
        (None, Some(String::new()), false),
    ] {
        if EvaluationConfig::new(endpoint, command, None, None, false, allow_network).is_ok() {
            return Err("invalid evaluation transport accepted".into());
        }
    }
    for (model, revision) in [(Some(String::new()), None), (None, Some(String::new()))] {
        if EvaluationConfig::new(
            Some("endpoint".to_owned()),
            None,
            model,
            revision,
            false,
            false,
        )
        .is_ok()
        {
            return Err("empty evaluation model/revision accepted".into());
        }
    }
    Ok(())
}

#[test]
fn evaluation_plumbing_reports_empty_output_without_claiming_quality() -> Result<(), Box<dyn Error>>
{
    let report = mock_evaluation(false, false)?;
    if report
        .get("cache_stable")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || !report
            .get("claims")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
        || !report
            .get("claim_review")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
        || !report
            .get("label_presence")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|labels| {
                labels.len() == 3 && labels.values().all(|value| value.as_bool() == Some(false))
            })
    {
        return Err(std::io::Error::other(
            "empty inference was misreported as useful semantic output",
        )
        .into());
    }
    Ok(())
}

#[test]
fn evaluation_reports_grounded_triples_quotes_and_shared_concepts() -> Result<(), Box<dyn Error>> {
    let report = mock_evaluation(true, false)?;
    let reviews = report
        .get("claim_review")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| std::io::Error::other("missing claim review"))?;
    let expected = [
        (
            "syntaxmesh",
            "uses",
            "penelope",
            "SyntaxMesh uses Penelope for durable indexing workflows so interrupted work can resume.",
            "docs/architecture.md",
        ),
        (
            "syntaxmesh",
            "supports",
            "statechronicle",
            "SyntaxMesh supports optional StateChronicle verification for accepted graph generations.",
            "docs/architecture.md",
        ),
        (
            "change engine",
            "owns",
            "repository mutation workflows",
            "The Change Engine owns repository mutation workflows. SyntaxMesh supplies read-only engineering evidence.",
            "docs/ownership.md",
        ),
        (
            "syntaxmesh",
            "supplies",
            "read-only engineering evidence",
            "The Change Engine owns repository mutation workflows. SyntaxMesh supplies read-only engineering evidence.",
            "docs/ownership.md",
        ),
    ];
    let mut syntaxmesh_id = None;
    for (subject, relation, object, quote, path) in expected {
        let review = reviews
            .iter()
            .find(|review| {
                review.get("subject").and_then(serde_json::Value::as_str) == Some(subject)
                    && review.get("relation").and_then(serde_json::Value::as_str) == Some(relation)
                    && review.get("object").and_then(serde_json::Value::as_str) == Some(object)
            })
            .ok_or_else(|| std::io::Error::other("grounded fixture triple missing"))?;
        let evidence = review
            .get("evidence")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| std::io::Error::other("missing fixture evidence"))?;
        let [evidence] = evidence.as_slice() else {
            return Err(std::io::Error::other("fixture evidence count differs").into());
        };
        if evidence.get("quote").and_then(serde_json::Value::as_str) != Some(quote)
            || evidence
                .get("source_path")
                .and_then(serde_json::Value::as_str)
                != Some(path)
            || evidence
                .get("evidence_class")
                .and_then(serde_json::Value::as_str)
                != Some("SemanticInference")
        {
            return Err(
                std::io::Error::other("review evidence differs from exact fixture source").into(),
            );
        }
        if subject == "syntaxmesh" {
            let id = review
                .get("subject_id")
                .ok_or_else(|| std::io::Error::other("missing concept identity"))?;
            if syntaxmesh_id.is_some_and(|previous| previous != id) {
                return Err(std::io::Error::other(
                    "shared concept identity differs between claims",
                )
                .into());
            }
            syntaxmesh_id = Some(id);
        }
    }
    if reviews.len() != 4
        || report
            .get("cache_stable")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
    {
        return Err(
            std::io::Error::other("positive evaluation cache or claim count mismatch").into(),
        );
    }
    Ok(())
}

#[test]
fn evaluation_reports_joint_source_claims_and_two_layer_cache() -> Result<(), Box<dyn Error>> {
    let report = mock_evaluation(true, true)?;
    if report
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(5)
        || report
            .get("cross_document")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || report
            .get("joint_source_claims")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        || report
            .get("expected_cache_units")
            .and_then(serde_json::Value::as_u64)
            != Some(3)
        || report
            .get("cache_stable")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || !report
            .get("cached_elapsed_us")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.parse::<u128>().is_ok())
    {
        return Err(std::io::Error::other("cross-document evaluation evidence differs").into());
    }
    let reviews = report
        .get("claim_review")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| std::io::Error::other("missing joint evidence review"))?;
    let joint = reviews
        .iter()
        .find(|review| {
            review.get("relation").and_then(serde_json::Value::as_str) == Some("separates")
        })
        .ok_or_else(|| std::io::Error::other("missing joint fixture claim"))?;
    if joint
        .get("evidence")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        != Some(2)
    {
        return Err(std::io::Error::other("joint fixture lost source evidence").into());
    }
    Ok(())
}

#[test]
fn empty_cross_document_evaluation_does_not_claim_joint_reasoning() -> Result<(), Box<dyn Error>> {
    let report = mock_evaluation(false, true)?;
    if report
        .get("joint_source_claims")
        .and_then(serde_json::Value::as_u64)
        != Some(0)
        || report
            .get("cache_stable")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || !report
            .get("claim_review")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return Err(
            std::io::Error::other("empty compound inference was reported as reasoning").into(),
        );
    }
    Ok(())
}

#[test]
fn evaluator_rejects_invalid_cross_document_mode_before_inference() -> Result<(), Box<dyn Error>> {
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "live_provider_retains_claims_and_reuses_unchanged_documents",
            "--ignored",
            "--nocapture",
        ])
        .env_remove("SYNTAXMESH_SEMANTIC_EVAL_COMMAND")
        .env("SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT", "http://127.0.0.1:0/v1")
        .env("SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT", "invalid")
        .env_remove("SYNTAXMESH_SEMANTIC_API_KEY")
        .output()?;
    if output.status.success()
        || !String::from_utf8_lossy(&output.stderr)
            .contains("evaluator cross-document mode must be true or false")
        || String::from_utf8_lossy(&output.stdout).contains("live semantic evidence:")
    {
        return Err(std::io::Error::other("invalid evaluation mode reached workload setup").into());
    }
    Ok(())
}

fn mock_evaluation(
    grounded: bool,
    cross_document: bool,
) -> Result<serde_json::Value, Box<dyn Error>> {
    use std::io::Write;
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        for attempt in 0..if cross_document { 4 } else { 2 } {
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
                return Err(std::io::Error::other(
                    "evaluation made an unexpected request",
                ));
            }
            let claims = if grounded {
                if cross_document && attempt % 2 == 1 {
                    joint_response(&request)?
                } else {
                    grounded_response(&request, attempt == 0)?
                }
            } else {
                json!({"claims":[]})
            };
            let body =
            json!({"choices":[{"finish_reason":"stop","message":{"content":claims.to_string()}}]})
                .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
        }
        Ok(())
    });
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "live_provider_retains_claims_and_reuses_unchanged_documents",
            "--ignored",
            "--nocapture",
        ])
        .env("SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT", endpoint)
        .env("SYNTAXMESH_SEMANTIC_EVAL_MODEL", "fixture")
        .env("SYNTAXMESH_SEMANTIC_EVAL_REVISION", "fixture-revision")
        .env("SYNTAXMESH_SEMANTIC_EVAL_ALLOW_NETWORK", "false")
        .env(
            "SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT",
            cross_document.to_string(),
        )
        .env_remove("SYNTAXMESH_SEMANTIC_API_KEY")
        .output()?;
    server
        .join()
        .map_err(|_panic| std::io::Error::other("evaluation fixture panicked"))??;
    require_success(&output)?;
    let stdout = String::from_utf8(output.stdout)?;
    let root = stdout
        .lines()
        .find_map(|line| line.strip_prefix("live semantic evidence: "))
        .ok_or_else(|| std::io::Error::other("evaluation omitted its evidence path"))?;
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(Path::new(root).join("report.json"))?)?;
    for field in ["edit_cache_selective", "edited_cache_stable"] {
        if report.get(field).and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(std::io::Error::other("sparse edit cache check failed").into());
        }
    }
    for field in ["edited_elapsed_us", "edited_cached_elapsed_us"] {
        if !report
            .get(field)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.parse::<u128>().is_ok())
        {
            return Err(std::io::Error::other("sparse edit timing missing").into());
        }
    }
    if fs::read_to_string(Path::new(root).join("initial-source/docs/ownership.md"))? != BOUNDARIES
        || fs::read_to_string(Path::new(root).join("source/docs/ownership.md"))?
            != format!("{BOUNDARIES}{OWNERSHIP_EDIT}")
    {
        return Err(std::io::Error::other("sparse edit lost retained source versions").into());
    }
    for label in ["edited", "edited-cached"] {
        for stream in ["stdout", "stderr"] {
            if !Path::new(root)
                .join(format!("{label}.{stream}.txt"))
                .is_file()
            {
                return Err(std::io::Error::other("sparse edit logs missing").into());
            }
        }
    }
    if grounded {
        let root = Path::new(root);
        let store = FileGraphStore::open(root.join("index.snapshot"))?;
        let generation = store
            .latest_generation()
            .ok_or_else(|| std::io::Error::other("missing fixture generation"))?
            .generation;
        fs::write(
            root.join("source/docs/architecture.md"),
            "changed retained fixture",
        )?;
        let rejected = semantic_review::accepted_claims(&store, generation, &root.join("source"));
        fs::write(root.join("source/docs/architecture.md"), ARCHITECTURE)?;
        if !rejected.is_err_and(|error| {
            error
                .to_string()
                .contains("retained source disagrees with accepted file hash")
        }) {
            return Err(std::io::Error::other("review accepted changed source bytes").into());
        }
    }
    Ok(report)
}

fn grounded_response(
    request: &serde_json::Value,
    initial: bool,
) -> std::io::Result<serde_json::Value> {
    let prompt = request
        .get("messages")
        .and_then(|messages| messages.get(1))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| std::io::Error::other("missing evaluation prompt"))?;
    let input: serde_json::Value = serde_json::from_str(prompt).map_err(std::io::Error::other)?;
    let mut claims = Vec::new();
    for group in input
        .get("requests")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| std::io::Error::other("missing evaluation request groups"))?
    {
        for chunk in group
            .get("chunks")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| std::io::Error::other("missing evaluation chunks"))?
        {
            let text = chunk
                .get("text")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing evaluation text"))?;
            let hash = chunk
                .get("content_hash")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing evaluation hash"))?;
            let triple = if text.starts_with("SyntaxMesh uses Penelope") {
                Some(("SyntaxMesh", "uses", "Penelope"))
            } else if text.starts_with("SyntaxMesh supports optional StateChronicle") {
                Some(("SyntaxMesh", "supports", "StateChronicle"))
            } else if text.starts_with("The Change Engine owns") {
                Some(("Change Engine", "owns", "repository mutation workflows"))
            } else {
                None
            };
            if let Some((subject, relation, object)) = triple {
                claims.push(json!({"subject":subject,"relation":relation,"object":object,"evidence":[{"chunk_content_hash":hash,"quote":text.trim()}]}));
            }
            if text.starts_with("The Change Engine owns") {
                claims.push(json!({"subject":"SyntaxMesh","relation":"supplies","object":"read-only engineering evidence","evidence":[{"chunk_content_hash":hash,"quote":text.trim()}]}));
            }
        }
    }
    if claims.len() != if initial { 4 } else { 2 } {
        return Err(std::io::Error::other(
            "evaluation prompt omitted controlled claims",
        ));
    }
    Ok(json!({"claims":claims}))
}

fn joint_response(request: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let prompt = request
        .get("messages")
        .and_then(|messages| messages.get(1))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| std::io::Error::other("missing compound prompt"))?;
    let input: serde_json::Value = serde_json::from_str(prompt).map_err(std::io::Error::other)?;
    let groups = input
        .get("requests")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| std::io::Error::other("missing compound request"))?;
    let [group] = groups.as_slice() else {
        return Err(std::io::Error::other(
            "joint fixture expected one compound request",
        ));
    };
    let chunks = group
        .get("chunks")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| std::io::Error::other("missing compound chunks"))?;
    let mut evidence = Vec::new();
    for prefix in ["SyntaxMesh uses Penelope", "The Change Engine owns"] {
        let chunk = chunks
            .iter()
            .find(|chunk| {
                chunk
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| text.starts_with(prefix))
            })
            .ok_or_else(|| std::io::Error::other("joint fixture omitted a source"))?;
        let quote = chunk
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| std::io::Error::other("joint fixture lacks source text"))?;
        evidence.push(json!({"chunk_content_hash":chunk["content_hash"],"quote":quote.trim()}));
    }
    Ok(
        json!({"claims":[{"subject":"SyntaxMesh","relation":"separates",
        "object":"durable indexing from repository mutation","evidence":evidence}]}),
    )
}

#[test]
#[ignore = "opt-in live inference; use cargo make evaluate-semantic-provider"]
fn live_provider_retains_claims_and_reuses_unchanged_documents() -> Result<(), Box<dyn Error>> {
    semantic_evaluation::run(semantic_eval_config::EvaluationConfig::from_environment()?)?;
    Ok(())
}
