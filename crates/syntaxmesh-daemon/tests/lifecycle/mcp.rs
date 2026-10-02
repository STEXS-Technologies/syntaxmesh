use super::*;
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ContentBlock},
    transport::StreamableHttpClientTransport,
};

fn tool_json(result: rmcp::model::CallToolResult) -> Result<Value, Box<dyn Error>> {
    let Some(ContentBlock::Text(content)) = result.content.into_iter().next() else {
        return Err("MCP tool returned no text content".into());
    };
    Ok(serde_json::from_str(&content.text)?)
}

#[test]
fn daemon_mcp_transport_refreshes_with_watch_and_preserves_http_boundary()
-> Result<(), Box<dyn Error>> {
    exercise_daemon_mcp(true)
}

#[test]
fn daemon_mcp_transport_refreshes_with_native_watch() -> Result<(), Box<dyn Error>> {
    exercise_daemon_mcp(false)
}

fn exercise_daemon_mcp(polling: bool) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    let file = root.join("lib.rs");
    std::fs::write(&file, "pub fn protocol_before() {}")?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let (mut process, base) = start_with_mcp(&root, &database, polling, true)?;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()?;
    for (header, value) in [
        ("origin", "https://example.com"),
        ("host", "evil.example"),
        ("sec-fetch-site", "cross-site"),
    ] {
        if client
            .post(format!("{base}/mcp"))
            .header(header, value)
            .body("{}")
            .send()?
            .status()
            .as_u16()
            != 403
        {
            return Err("MCP route bypassed the shared HTTP boundary".into());
        }
    }
    if client
        .post(format!("{base}/mcp"))
        .header("x-syntaxmesh-owner-instance", "invalid-instance")
        .body("{}")
        .send()?
        .status()
        .as_u16()
        != 412
    {
        return Err("MCP bypassed the owner-instance boundary".into());
    }
    if client
        .post(format!("{base}/mcp"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(" ".repeat(65537))
        .send()?
        .status()
        .as_u16()
        != 413
    {
        return Err("MCP accepted an oversized request body".into());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let protocol = runtime.block_on(async {
        let service = tokio::time::timeout(
            Duration::from_secs(3),
            ().serve(StreamableHttpClientTransport::from_uri(format!(
                "{base}/mcp"
            ))),
        )
        .await??;
        let initial = tool_json(
            service
                .peer()
                .call_tool(CallToolRequestParams::new("search").with_arguments(
                    serde_json::from_value(serde_json::json!({"text":"protocol_before"}))?,
                ))
                .await?,
        )?;
        if !initial
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(|nodes| !nodes.is_empty())
        {
            return Err::<(), Box<dyn Error>>("MCP protocol initial search missing".into());
        }
        let old = generation(&initial)?.to_owned();
        std::fs::write(&file, "pub fn protocol_after() {}")?;
        let waiting = Instant::now();
        loop {
            let after = tool_json(
                service
                    .peer()
                    .call_tool(CallToolRequestParams::new("search").with_arguments(
                        serde_json::from_value(serde_json::json!({"text":"protocol_after"}))?,
                    ))
                    .await?,
            )?;
            if generation(&after)? != old
                && after
                    .get("nodes")
                    .and_then(Value::as_array)
                    .is_some_and(|nodes| !nodes.is_empty())
            {
                break;
            }
            if waiting.elapsed() > Duration::from_secs(3) {
                return Err("MCP protocol failed to refresh after save".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Keep the SDK session open across termination: shutdown must not wait
        // for the client to close its event stream first.
        #[cfg(unix)]
        {
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(i32::try_from(process.0.id())?),
                nix::sys::signal::Signal::SIGTERM,
            )?;
            wait_exit(&mut process)?;
        }
        service.cancel().await?;
        Ok(())
    });
    drop(runtime);
    protocol?;
    #[cfg(not(unix))]
    wait_exit(&mut process)?;
    audit_history(&root, &database)?;
    Ok(())
}

#[test]
fn mcp_requires_tokenizer_before_storage_access() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let database = fixture.path().join("missing.db");
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"))
        .arg(fixture.path())
        .arg(&database)
        .args(["127.0.0.1:0", "--mcp"])
        .output()?;
    if output.status.success()
        || database.exists()
        || !String::from_utf8_lossy(&output.stderr).contains("--mcp requires --context-tokenizer")
    {
        return Err("MCP tokenizer requirement was not validated before startup".into());
    }
    Ok(())
}
