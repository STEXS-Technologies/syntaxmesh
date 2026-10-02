use std::error::Error;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};
use syntaxmesh_core::{FileId, FileVersion, NodeKind};
use syntaxmesh_integration_penelope::PenelopeSemanticEnricher;
use syntaxmesh_lang_docs::DocumentationExtractor;
use syntaxmesh_language_sdk::{LanguageExtractor, SourceFile};
use syntaxmesh_semantic::{SemanticDocumentChunk, SemanticProviderIdentity, SemanticRequest};
use syntaxmesh_store::InMemoryGraphStore;

use super::OpenAiCompatibleProvider;

fn server(responses: Vec<Value>) -> std::io::Result<(String, JoinHandle<std::io::Result<()>>)> {
    transport_server(
        responses
            .into_iter()
            .map(|body| ("GET /api/tags ", body))
            .collect(),
    )
}

fn transport_server(
    responses: Vec<(&'static str, Value)>,
) -> std::io::Result<(String, JoinHandle<std::io::Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let handle = std::thread::spawn(move || -> std::io::Result<()> {
        for (expected_path, response) in responses {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut request = Vec::new();
            loop {
                let mut buffer = [0_u8; 4096];
                let count = stream.read(&mut buffer)?;
                if count == 0 {
                    return Err(std::io::Error::other("incomplete fixture HTTP request"));
                }
                request.extend_from_slice(
                    buffer
                        .get(..count)
                        .ok_or_else(|| std::io::Error::other("invalid request length"))?,
                );
                if let Some(boundary) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(
                        request
                            .get(..boundary)
                            .ok_or_else(|| std::io::Error::other("invalid headers"))?,
                    )
                    .map_err(std::io::Error::other)?;
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>())
                        })
                        .transpose()
                        .map_err(std::io::Error::other)?
                        .unwrap_or(0);
                    if request.len() >= boundary.saturating_add(4).saturating_add(length) {
                        break;
                    }
                }
            }
            if !request.starts_with(expected_path.as_bytes()) {
                return Err(std::io::Error::other("wrong model metadata endpoint"));
            }
            let body = response.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )?;
        }
        Ok(())
    });
    Ok((endpoint, handle))
}

#[test]
fn changed_model_output_is_not_completed_in_penelope_cache() -> Result<(), Box<dyn Error>> {
    let initial = json!({"models":[{"name":"fixture:latest", "digest":"a".repeat(64)}]});
    let changed = json!({"models":[{"name":"fixture:latest", "digest":"b".repeat(64)}]});
    let completion =
        json!({"choices":[{"finish_reason":"stop", "message":{"content":"{\"claims\":[]}"}}]});
    let (endpoint, handle) = transport_server(vec![
        ("GET /api/tags ", initial),
        ("POST /v1/chat/completions ", completion),
        ("GET /api/tags ", changed),
    ])?;
    let mut provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
    provider.resolve_model_revision(None)?;
    let request = request(provider.identity.clone())?;
    let mut store = InMemoryGraphStore::new();
    let first = PenelopeSemanticEnricher::new(&mut store).enrich(request.clone(), &provider);
    finish(handle)?;
    let retry = PenelopeSemanticEnricher::new(&mut store).enrich(request, &provider);
    if !first.is_err_and(|error| error.to_string().contains("revision changed"))
        || retry.is_ok()
        || provider.requests_made() != 4
    {
        return Err(std::io::Error::other(
            "changed-model output was cached instead of remaining retryable",
        )
        .into());
    }
    Ok(())
}

fn finish(handle: JoinHandle<std::io::Result<()>>) -> Result<(), Box<dyn Error>> {
    handle
        .join()
        .map_err(|_panic| std::io::Error::other("model metadata fixture panicked"))??;
    Ok(())
}

fn request(identity: SemanticProviderIdentity) -> Result<SemanticRequest, Box<dyn Error>> {
    let content = "# Architecture\n\nDurable indexing uses Penelope.\n".to_owned();
    let source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"architecture.md"]),
            normalized_path: "architecture.md".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content,
    };
    let chunks = DocumentationExtractor
        .extract(&source)?
        .nodes
        .into_iter()
        .filter(|node| node.kind == NodeKind::DocumentChunk)
        .map(|node| SemanticDocumentChunk {
            text: node.name.clone(),
            node,
            context: Vec::new(),
        })
        .collect();
    Ok(SemanticRequest::new(identity, chunks)?)
}

#[test]
fn local_digest_changes_identity_and_rejects_changed_revision() -> Result<(), Box<dyn Error>> {
    let first = json!({"models":[{"name":"fixture:latest", "digest":"a".repeat(64)}]});
    let changed = json!({"models":[{"name":"fixture:latest", "digest":"b".repeat(64)}]});
    let (endpoint, handle) = server(vec![first.clone(), first, changed.clone(), changed])?;
    let mut provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
    provider.resolve_model_revision(None)?;
    let first_identity = provider.identity.clone();
    let first_cache_key = request(first_identity.clone())?.cache_key();
    provider
        .verify_model_revision()
        .map_err(std::io::Error::other)?;
    let changed_check = provider.verify_model_revision();
    provider.resolve_model_revision(None)?;
    finish(handle)?;
    if first_identity.model_revision != format!("ollama:sha256:{}", "a".repeat(64))
        || provider.identity.model_revision != format!("ollama:sha256:{}", "b".repeat(64))
        || !changed_check.is_err_and(|error| error.contains("revision changed"))
        || first_identity == provider.identity
        || first_cache_key == request(provider.identity.clone())?.cache_key()
    {
        return Err(
            std::io::Error::other("model revision identity/change detection failed").into(),
        );
    }
    Ok(())
}

#[test]
fn discovery_rejects_missing_duplicate_and_malformed_metadata() -> Result<(), Box<dyn Error>> {
    let valid = json!({"name":"fixture:latest", "digest":"a".repeat(64)});
    let responses = vec![
        json!({"models":[]}),
        json!({"models":[valid.clone(), valid]}),
        json!({"models":[{"name":"fixture:latest", "digest":"not-a-digest"}]}),
        json!({"models":"malformed"}),
    ];
    let count = responses.len();
    let (endpoint, handle) = server(responses)?;
    let mut provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
    for _ in 0..count {
        if provider.resolve_model_revision(None).is_ok() {
            return Err(std::io::Error::other("invalid metadata accepted").into());
        }
    }
    finish(handle)?;
    Ok(())
}

#[test]
fn explicit_revision_is_offline_and_remote_discovery_requires_assertion()
-> Result<(), Box<dyn Error>> {
    let mut local =
        OpenAiCompatibleProvider::new(Some("http://127.0.0.1:1/v1"), "fixture", None, false)?;
    local.resolve_model_revision(Some("immutable-1"))?;
    local
        .verify_model_revision()
        .map_err(std::io::Error::other)?;
    let mut remote =
        OpenAiCompatibleProvider::new(Some("https://example.invalid/v1"), "fixture", None, true)?;
    if remote.resolve_model_revision(None).is_ok()
        || local.identity.model_revision != "asserted:immutable-1"
        || local.resolve_model_revision(Some(" ")).is_ok()
    {
        return Err(std::io::Error::other("explicit revision policy failed").into());
    }
    remote.resolve_model_revision(Some("immutable-1"))?;
    Ok(())
}

#[test]
fn revision_option_rejects_missing_semantic_empty_and_duplicate_values()
-> Result<(), Box<dyn Error>> {
    for arguments in [
        vec!["--semantic-model-revision=rev-1"],
        vec!["--semantic", "--semantic-model-revision="],
        vec!["--semantic", "--semantic-model-revision"],
        vec![
            "--semantic",
            "--semantic-model-revision=rev-1",
            "--semantic-model-revision=rev-2",
        ],
    ] {
        if crate::cli::indexing::index_options(arguments.into_iter().map(str::to_owned)).is_ok() {
            return Err(std::io::Error::other("invalid model revision option accepted").into());
        }
    }
    Ok(())
}
