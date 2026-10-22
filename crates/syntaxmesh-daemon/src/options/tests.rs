use super::*;

#[test]
fn indexed_mcp_policies_require_mcp_and_compose_without_changing_defaults() -> Result<(), String> {
    let base = ["source", "graph.db", "127.0.0.1:0"];
    let defaults = parse(base.into_iter().map(OsString::from))?.ok_or("missing defaults")?;
    if defaults.source_content_context || defaults.lexical_first_context {
        return Err("indexed context became a default".to_owned());
    }
    for flag in ["--source-content-context", "--lexical-first-context"] {
        if parse(base.into_iter().chain([flag]).map(OsString::from)).is_ok()
            || parse(
                base.into_iter()
                    .chain(["--mcp", "--context-tokenizer", "cl100k_base", flag, flag])
                    .map(OsString::from),
            )
            .is_ok()
        {
            return Err("invalid indexed MCP policy options accepted".to_owned());
        }
    }
    let enabled = parse(
        base.into_iter()
            .chain([
                "--mcp",
                "--context-tokenizer",
                "cl100k_base",
                "--source-content-context",
                "--lexical-first-context",
            ])
            .map(OsString::from),
    )?
    .ok_or("missing configured options")?;
    if !enabled.mcp || !enabled.source_content_context || !enabled.lexical_first_context {
        return Err("indexed MCP policies failed to compose".to_owned());
    }
    Ok(())
}

#[test]
fn partial_processing_is_explicit_and_duplicate_flags_reject() -> Result<(), String> {
    let base = ["source", "graph.db", "127.0.0.1:0"];
    let strict = parse(base.into_iter().map(OsString::from))?.ok_or("missing options")?;
    let partial = parse(
        base.into_iter()
            .chain(["--verify", "--record-syntax-failures"])
            .map(OsString::from),
    )?
    .ok_or("missing options")?;
    if strict.syntax_policy != SourceSyntaxPolicy::Strict
        || partial.syntax_policy != SourceSyntaxPolicy::RecordFailures
        || !partial.verify
        || parse(
            base.into_iter()
                .chain(["--record-syntax-failures", "--record-syntax-failures"])
                .map(OsString::from),
        )
        .is_ok()
    {
        return Err("daemon source policy parsing mismatch".to_owned());
    }
    Ok(())
}

#[cfg(feature = "inference-sqlite")]
#[test]
fn inference_read_config_is_explicit_and_unique() -> Result<(), String> {
    let base = ["source", "graph.db", "127.0.0.1:0"];
    let options = parse(
        base.into_iter()
            .chain(["--inference-read-config", "inference-read.json"])
            .map(OsString::from),
    )?
    .ok_or("missing inference read options")?;
    if options.inference_read_config != Some("inference-read.json".into())
        || parse(
            base.into_iter()
                .chain([
                    "--inference-read-config",
                    "first.json",
                    "--inference-read-config",
                    "second.json",
                ])
                .map(OsString::from),
        )
        .is_ok()
        || parse(
            base.into_iter()
                .chain(["--inference-read-config"])
                .map(OsString::from),
        )
        .is_ok()
    {
        return Err("inference read config parsing accepted an invalid option sequence".to_owned());
    }
    Ok(())
}

#[cfg(not(feature = "inference-sqlite"))]
#[test]
fn inference_read_config_requires_opt_in_build_feature() {
    let base = ["source", "graph.db", "127.0.0.1:0"];
    let error = parse(
        base.into_iter()
            .chain(["--inference-read-config"])
            .map(OsString::from),
    )
    .err();
    assert!(error.is_some_and(|message| message.contains("inference-sqlite")));
}
