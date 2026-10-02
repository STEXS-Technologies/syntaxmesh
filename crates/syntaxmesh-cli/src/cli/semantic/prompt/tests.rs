use std::error::Error;
use std::io::Write;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use syntaxmesh_core::{FileId, Node, NodeId, NodeKind, ProvenanceId, SourceLocation, SourceSpan};
use syntaxmesh_semantic::{SemanticDocumentChunk, SemanticProvider, SemanticRequest};

use super::{ENVELOPE_BYTES, encode, encode_request};
use crate::cli::semantic::{MAX_PROMPT_BYTES, OpenAiCompatibleProvider};

#[path = "../../../../tests/support/semantic_http.rs"]
mod semantic_http;

macro_rules! ensure {
    ($condition:expr, $message:literal) => {
        if !$condition {
            return Err(std::io::Error::other($message).into());
        }
    };
}

fn request(
    provider: &OpenAiCompatibleProvider,
    text: &str,
    context: Vec<String>,
) -> Result<SemanticRequest, Box<dyn Error>> {
    let file_id = FileId::derive(&[text.as_bytes()]);
    Ok(SemanticRequest::new(
        provider.identity(),
        vec![SemanticDocumentChunk {
            node: Node {
                id: NodeId::derive(&[text.as_bytes()]),
                kind: NodeKind::DocumentChunk,
                name: text.to_owned(),
                owner_file: Some(file_id),
                source: Some(SourceLocation {
                    file_id,
                    content_hash: *blake3::hash(text.as_bytes()).as_bytes(),
                    span: SourceSpan::new(0, u64::try_from(text.len())?)?,
                }),
                provenance: ProvenanceId::derive(&[b"prompt-fixture"]),
                extension_payload: None,
            },
            text: text.to_owned(),
            context,
        }],
    )?)
}

#[test]
fn exact_encoding_matches_existing_json_and_counts_envelope_separators()
-> Result<(), Box<dyn Error>> {
    let provider = OpenAiCompatibleProvider::new(None, "fixture", None, false)?;
    let requests = vec![
        request(
            &provider,
            "Unicode résumé, quotes \" and slash \\.",
            vec!["Heading \"quoted\"".to_owned()],
        )?,
        request(&provider, "Controls \u{0000}\n\t", vec!["Other".to_owned()])?,
    ];
    let previous = requests
        .iter()
        .map(|request| {
            let chunks = request
                .prompt_chunks()
                .into_iter()
                .map(|chunk| {
                    json!({
                        "content_hash": hex::encode(chunk.content_hash),
                        "section_context": chunk.context, "text": chunk.text,
                    })
                })
                .collect::<Vec<_>>();
            json!({"chunks": chunks})
        })
        .collect::<Vec<_>>();
    let encoded = encode(&requests)?;
    ensure!(
        encoded == serde_json::to_string(&json!({"requests": previous}))?,
        "wire encoding changed"
    );
    let fragments = requests
        .iter()
        .map(encode_request)
        .collect::<Result<Vec<_>, _>>()?;
    ensure!(
        encoded.len()
            == fragments
                .iter()
                .map(|(text, _)| text.len())
                .sum::<usize>()
                .saturating_add(ENVELOPE_BYTES)
                .saturating_add(1),
        "incorrect envelope or separator accounting"
    );
    ensure!(
        encode(&[])?.len() == ENVELOPE_BYTES,
        "incorrect empty envelope length"
    );
    Ok(())
}

#[test]
fn escaped_single_request_is_rejected_before_either_http_entry_point() -> Result<(), Box<dyn Error>>
{
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
    let text = "\"".repeat(26 * 1024);
    let request = request(&provider, &text, Vec::new())?;
    ensure!(text.len() < MAX_PROMPT_BYTES, "fixture source is oversized");
    ensure!(
        encode(std::slice::from_ref(&request))?.len() > MAX_PROMPT_BYTES,
        "escaping did not exceed bound"
    );
    ensure!(
        provider.extract(&request).is_err(),
        "single entry bypassed cap"
    );
    ensure!(
        provider.extract_many(&[request]).iter().all(Result::is_err),
        "batch entry bypassed cap"
    );
    ensure!(
        provider.requests_made() == 0,
        "oversized input attempted HTTP"
    );
    ensure!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "oversized input connected"
    );
    Ok(())
}

#[test]
fn exact_packing_splits_escaped_sources_before_sending_them() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
    let requests = vec![
        request(&provider, &"\"".repeat(15 * 1024), Vec::new())?,
        request(&provider, &"\\".repeat(15 * 1024), Vec::new())?,
    ];
    ensure!(
        encode(&requests)?.len() > MAX_PROMPT_BYTES,
        "fixture does not exceed packed limit"
    );
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        for _ in 0..2 {
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
            let (_, body) = semantic_http::read_request(&mut stream)?;
            let prompt = body
                .pointer("/messages/1/content")
                .and_then(Value::as_str)
                .ok_or_else(|| std::io::Error::other("missing prompt"))?;
            let parsed: Value = serde_json::from_str(prompt).map_err(std::io::Error::other)?;
            if prompt.len() > MAX_PROMPT_BYTES
                || parsed
                    .get("requests")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    != Some(1)
            {
                return Err(std::io::Error::other("packed prompt exceeded exact bounds"));
            }
            let response = json!({"choices": [{"finish_reason": "stop",
                "message": {"content": "{\"claims\":[]}"}}]})
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            )?;
        }
        Ok(())
    });
    let outputs = provider.extract_many(&requests);
    server
        .join()
        .map_err(|_panic| std::io::Error::other("prompt fixture panicked"))??;
    ensure!(outputs.len() == 2, "packing dropped a request");
    ensure!(outputs.iter().all(Result::is_ok), "packed inference failed");
    ensure!(provider.requests_made() == 2, "incorrect packed HTTP count");
    Ok(())
}
