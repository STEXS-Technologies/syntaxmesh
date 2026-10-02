use super::*;

#[test]
fn project_config_defaults_verification_to_disabled() -> Result<(), String> {
    let empty: ProjectConfig = toml::from_str("").map_err(|error| error.to_string())?;
    let history: ProjectConfig =
        toml::from_str("[history]\n").map_err(|error| error.to_string())?;
    let disabled: ProjectConfig =
        toml::from_str("[history]\nverified = false\n").map_err(|error| error.to_string())?;
    let enabled: ProjectConfig =
        toml::from_str("[history]\nverified = true\n").map_err(|error| error.to_string())?;
    if empty.verified_history()
        || history.verified_history()
        || disabled.verified_history()
        || !enabled.verified_history()
    {
        return Err(
            "project history verification defaults or explicit values were incorrect".to_owned(),
        );
    }
    Ok(())
}

#[test]
fn project_config_ignores_unknown_keys_but_rejects_invalid_known_values() -> Result<(), String> {
    if toml::from_str::<ProjectConfig>("[history]\nverified = true\nfuture_policy = \"ignored\"\n")
        .is_err()
        || toml::from_str::<ProjectConfig>("[history]\nverified = \"yes\"\n").is_ok()
        || toml::from_str::<ProjectConfig>("[history\nverified = true").is_ok()
    {
        return Err(
            "project config did not enforce the documented forward-compatible schema".to_owned(),
        );
    }
    Ok(())
}

#[test]
fn module_resolution_requires_an_explicit_supported_profile() -> Result<(), String> {
    let disabled: ProjectConfig = toml::from_str("").map_err(|error| error.to_string())?;
    let enabled: ProjectConfig = toml::from_str("[module_resolution]\nprofile = \"node\"\n")
        .map_err(|error| error.to_string())?;
    if !disabled.module_resolution_profiles().is_empty()
        || enabled.module_resolution_profiles() != vec![ModuleResolutionProfile::Node]
        || toml::from_str::<ProjectConfig>("[module_resolution]\nprofile = \"guess\"\n").is_ok()
    {
        return Err("module resolution profile default or validation is incorrect".to_owned());
    }
    let both: ProjectConfig = toml::from_str(
        "[module_resolution]\nprofiles = [\"node\", \"python\"]\nsource_roots = [\"src\"]\n",
    )
    .map_err(|error| error.to_string())?;
    if both.module_resolution_profiles()
        != vec![
            ModuleResolutionProfile::Node,
            ModuleResolutionProfile::Python,
        ]
        || both.python_source_roots() != ["src"]
    {
        return Err("multi-profile Python configuration was not preserved".to_owned());
    }
    Ok(())
}
