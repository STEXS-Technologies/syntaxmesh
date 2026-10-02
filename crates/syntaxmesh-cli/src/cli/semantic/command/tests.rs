use super::validate;

#[test]
fn selected_model_reaches_command_and_cache_identity() -> Result<(), Box<dyn std::error::Error>> {
    let argv = vec![
        "codex".to_owned(),
        "exec".to_owned(),
        "--model".to_owned(),
        "{model}".to_owned(),
    ];
    let first =
        super::super::OpenAiCompatibleProvider::for_command(argv.clone(), "gpt-6-luna", None)?;
    let second =
        super::super::OpenAiCompatibleProvider::for_command(argv, "user-selected-model", None)?;
    if first
        .command
        .as_ref()
        .and_then(|args| args.last())
        .map(String::as_str)
        != Some("gpt-6-luna")
        || second
            .command
            .as_ref()
            .and_then(|args| args.last())
            .map(String::as_str)
            != Some("user-selected-model")
        || first.identity.configuration_hash == second.identity.configuration_hash
    {
        return Err("model selection did not reach command/cache identity".into());
    }
    Ok(())
}

#[test]
fn command_config_is_loaded_without_shell_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("argv.json");
    std::fs::write(&path, r#"["codex", "exec", "literal; not shell"]"#)?;
    let argv =
        super::super::OpenAiCompatibleProvider::command_argv(&format!("@{}", path.display()))?;
    if argv != ["codex", "exec", "literal; not shell"] {
        return Err("argv file was reinterpreted".into());
    }
    for input in ["[]", "{}", r#"["", "exec"]"#, r#"["codex", 1]"#] {
        if super::super::OpenAiCompatibleProvider::command_argv(input).is_ok() {
            return Err("invalid command configuration accepted".into());
        }
    }
    std::fs::write(&path, "x".repeat(65537))?;
    if super::super::OpenAiCompatibleProvider::command_argv(&format!("@{}", path.display())).is_ok()
    {
        return Err("oversized argv file accepted".into());
    }
    Ok(())
}

#[test]
fn command_argv_is_explicit_bounded_and_not_shell_parsed() {
    assert!(validate(&[]).is_err());
    assert!(validate(&[String::new()]).is_err());
    assert!(validate(&["codex\0other".to_owned()]).is_err());
    assert!(validate(&vec!["codex".to_owned(); 129]).is_err());
    assert!(validate(&["codex".to_owned(), "x".repeat(65537)]).is_err());
    assert!(
        validate(&[
            "codex".to_owned(),
            "exec".to_owned(),
            "literal; not shell".to_owned()
        ])
        .is_ok()
    );
}
