#[test]
fn command_default_and_model_override_are_order_independent()
-> Result<(), Box<dyn std::error::Error>> {
    for (arguments, expected) in [
        (
            vec!["--semantic", "--semantic-command", "[\"codex\"]"],
            "gpt-6-luna",
        ),
        (
            vec!["--semantic-command", "[\"codex\"]", "--semantic"],
            "gpt-6-luna",
        ),
        (
            vec![
                "--semantic",
                "custom-model",
                "--semantic-command",
                "[\"codex\"]",
            ],
            "custom-model",
        ),
        (
            vec![
                "--semantic-command",
                "[\"codex\"]",
                "--semantic=custom-model",
            ],
            "custom-model",
        ),
        (vec!["--semantic"], "qwen3:latest"),
    ] {
        let options = super::index_options(arguments.into_iter().map(str::to_owned))?;
        if options
            .semantic
            .as_ref()
            .map(|semantic| semantic.model.as_str())
            != Some(expected)
        {
            return Err("wrong semantic model selection".into());
        }
    }
    Ok(())
}
#[test]
fn syntax_failure_policy_is_explicit_and_unique() -> Result<(), Box<dyn std::error::Error>> {
    let strict = super::index_options(Vec::<String>::new())?;
    let partial = super::index_options(["--record-syntax-failures".to_owned()])?;
    if strict.syntax_policy != syntaxmesh_engine::SourceSyntaxPolicy::Strict
        || partial.syntax_policy != syntaxmesh_engine::SourceSyntaxPolicy::RecordFailures
        || super::index_options([
            "--record-syntax-failures".to_owned(),
            "--record-syntax-failures".to_owned(),
        ])
        .is_ok()
    {
        return Err("syntax failure policy must be explicit and unique".into());
    }
    Ok(())
}
