use std::error::Error;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use super::OpenAiCompatibleProvider;

#[test]
fn standard_local_ollama_scheduling_is_serial() -> Result<(), Box<dyn Error>> {
    for (endpoint, expected) in [
        (None, 1),
        (Some("http://127.0.0.1:11434/v1"), 1),
        (Some("http://localhost:11434/v1"), 1),
        (Some("http://[::1]:11434/v1"), 1),
        (Some("http://127.0.0.1:12345/v1"), 4),
        (Some("https://example.com:11434/v1"), 4),
    ] {
        let provider = OpenAiCompatibleProvider::new(endpoint, "fixture", None, true)?;
        if provider.parallel_limit != expected {
            return Err(std::io::Error::other("unexpected inference parallel limit").into());
        }
    }
    Ok(())
}

#[test]
fn cross_document_option_is_explicit_and_requires_semantic_mode() {
    for args in [
        vec!["--semantic-cross-document"],
        vec![
            "--semantic",
            "--semantic-cross-document",
            "--semantic-cross-document",
        ],
        vec!["--semantic", "--semantic-cross-document=true"],
    ] {
        assert!(crate::cli::indexing::index_options(args.into_iter().map(str::to_owned)).is_err());
    }
    for args in [
        vec!["--semantic", "--semantic-cross-document"],
        vec!["--semantic-cross-document", "--semantic"],
        vec![
            "--semantic",
            "--semantic-cross-document",
            "--semantic-offline",
            "--semantic-model-revision=r",
        ],
    ] {
        assert!(crate::cli::indexing::index_options(args.into_iter().map(str::to_owned)).is_ok());
    }
}

#[test]
fn offline_guards_both_http_paths_without_sending_requests() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let mut provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
    provider.set_offline(true);
    if provider.resolve_model_revision(None).is_ok()
        || provider.send_request(&serde_json::json!({})).is_ok()
        || provider.requests_made() != 0
        || provider.metadata_requests_made() != 0
        || !matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    {
        return Err(std::io::Error::other("offline mode sent or permitted HTTP requests").into());
    }
    provider.resolve_model_revision(Some("fixture-revision"))?;
    provider.verify_model_revision()?;
    Ok(())
}

#[test]
fn offline_option_requires_revision_and_rejects_network_permission() {
    for args in [
        vec!["--semantic-offline"],
        vec!["--semantic", "--semantic-offline"],
        vec![
            "--semantic",
            "--semantic-offline",
            "--semantic-model-revision=r",
            "--allow-network",
        ],
        vec![
            "--semantic",
            "--semantic-offline",
            "--semantic-model-revision=r",
            "--semantic-offline",
        ],
    ] {
        assert!(crate::cli::indexing::index_options(args.into_iter().map(str::to_owned)).is_err());
    }
    assert!(
        crate::cli::indexing::index_options(
            [
                "--semantic",
                "--semantic-offline",
                "--semantic-model-revision=r",
                "--semantic-endpoint=https://example.invalid/v1"
            ]
            .into_iter()
            .map(str::to_owned)
        )
        .is_ok()
    );
}

#[test]
fn endpoint_policy_recognizes_ipv4_ipv6_and_localhost() -> Result<(), Box<dyn Error>> {
    for endpoint in [
        "http://127.0.0.1:11434/v1",
        "http://127.5.6.7:11434/v1",
        "http://[::1]:11434/v1",
        "http://LOCALHOST:11434/v1",
    ] {
        let mut provider = OpenAiCompatibleProvider::new(Some(endpoint), "fixture", None, false)?;
        provider.resolve_model_revision(Some("fixture-revision"))?;
    }
    for endpoint in [
        "http://[::2]/v1",
        "http://192.0.2.1/v1",
        "https://example.invalid/v1",
    ] {
        if OpenAiCompatibleProvider::new(Some(endpoint), "fixture", None, false).is_ok() {
            return Err(std::io::Error::other(
                "non-loopback endpoint bypassed network authorization",
            )
            .into());
        }
    }
    Ok(())
}

#[test]
fn loopback_provider_rejects_redirects_without_following_them() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let destination = TcpListener::bind("127.0.0.1:0")?;
    let location = format!("http://{}/v1/chat/completions", destination.local_addr()?);
    // Following this redirect would produce a connection failure, rather than
    // the original 302. The destination is closed before the request starts.
    drop(destination);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request)?;
        write!(
            stream,
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )?;
        Ok(())
    });
    let provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
    let response = provider.send_request(&serde_json::json!({}));
    server
        .join()
        .map_err(|_panic| std::io::Error::other("semantic fixture server panicked"))??;
    if !response.is_err_and(|error| error.contains("HTTP 302")) || provider.requests_made() != 1 {
        return Err(std::io::Error::other(
            "redirect must fail at the configured endpoint after exactly one request",
        )
        .into());
    }
    Ok(())
}
