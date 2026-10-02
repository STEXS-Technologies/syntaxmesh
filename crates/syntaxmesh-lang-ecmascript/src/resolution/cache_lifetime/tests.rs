use super::*;
use oxc_resolver::FileSystemOs;

#[test]
fn upstream_dependency_context_does_not_replace_auto_tsconfig_file_resolution()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    std::fs::create_dir(fixture.path().join("src"))?;
    std::fs::write(
        fixture.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
    )?;
    std::fs::write(fixture.path().join("src/caller.ts"), "import '@/target';")?;
    std::fs::write(
        fixture.path().join("src/target.ts"),
        "export const target = 1;",
    )?;
    let resolver = OxcModuleResolver::for_node_project(fixture.path(), FileSystemOs)?;
    let expected = fixture.path().join("src/target.ts");
    if resolver.resolve("src/caller.ts", "@/target")? != expected {
        return Err("automatic tsconfig discovery fixture did not resolve".into());
    }
    let mut context = oxc_resolver::ResolveContext::default();
    if resolver
        .import_resolver
        .resolve_with_context(fixture.path().join("src"), "@/target", None, &mut context)
        .is_ok()
    {
        return Err("dependency-context API changed; revisit auto-discovery limitation".into());
    }
    Ok(())
}

#[test]
fn cached_missing_file_requires_explicit_refresh_for_long_lived_hosts()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    std::fs::write(fixture.path().join("caller.ts"), "import './added';")?;
    let resolver = OxcModuleResolver::for_node_project(fixture.path(), FileSystemOs)?;
    if resolver.resolve("caller.ts", "./added").is_ok() {
        return Err("missing fixture target unexpectedly resolved".into());
    }
    std::fs::write(fixture.path().join("added.ts"), "export const added = 1;")?;
    if resolver.resolve("caller.ts", "./added").is_ok() {
        return Err("Oxc cache lifetime changed; revisit generation refresh design".into());
    }
    ModuleResolutionProvider::refresh(&resolver)?;
    if resolver.resolve("caller.ts", "./added")? != fixture.path().join("added.ts") {
        return Err("explicit refresh did not observe added target".into());
    }
    let refreshed = OxcModuleResolver::for_node_project(fixture.path(), FileSystemOs)?;
    if refreshed.resolve("caller.ts", "./added")? != fixture.path().join("added.ts") {
        return Err("fresh resolver failed to observe added target".into());
    }
    Ok(())
}
