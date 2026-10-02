use super::*;
use oxc_resolver::{FileMetadata, FileSystemOs};
use std::collections::BTreeSet;
use std::io;
use std::sync::Arc;

#[test]
fn package_and_missing_config_inputs_are_shared_between_resolution_modes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let package = fixture.path().join("node_modules/sample");
    std::fs::create_dir_all(&package)?;
    std::fs::write(fixture.path().join("caller.js"), "import 'sample';")?;
    let manifest = package.join("package.json");
    std::fs::write(
        &manifest,
        r#"{"exports":{"import":"./import.js","require":"./require.js"}}"#,
    )?;
    std::fs::write(package.join("import.js"), "export {};")?;
    std::fs::write(package.join("require.js"), "module.exports = {};")?;
    let resolver = OxcModuleResolver::for_node_project(fixture.path(), FileSystemOs)?;
    if resolver.resolve("caller.js", "sample")? != package.join("import.js")
        || resolver.resolve_with_mode("caller.js", "sample", ModuleResolutionMode::CommonJs)?
            != package.join("require.js")
    {
        return Err("tracking changed package conditions".into());
    }
    let paths = resolver.observed_paths()?;
    for expected in [
        &manifest,
        &fixture.path().join("tsconfig.json"),
        &package.join("import.js"),
        &package.join("require.js"),
    ] {
        if !paths.contains(expected) {
            return Err(format!("missing resolver input {}", expected.display()).into());
        }
    }
    if paths
        .iter()
        .zip(paths.iter().skip(1))
        .any(|(left, right)| left >= right)
    {
        return Err("observed paths are not sorted and deduplicated".into());
    }
    std::fs::write(
        &manifest,
        r#"{"exports":{"import":"./require.js","require":"./import.js"}}"#,
    )?;
    ModuleResolutionProvider::refresh(&resolver)?;
    if resolver.resolve("caller.js", "sample")? != package.join("require.js")
        || resolver.resolve_with_mode("caller.js", "sample", ModuleResolutionMode::CommonJs)?
            != package.join("import.js")
    {
        return Err("refresh did not observe package-only condition changes".into());
    }
    Ok(())
}

#[derive(Default)]
struct ObservedFileSystem {
    reads: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl FileSystem for ObservedFileSystem {
    fn new() -> Self {
        Self::default()
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.reads
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?
            .insert(path.to_owned());
        FileSystemOs.read(path)
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.reads
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?
            .insert(path.to_owned());
        FileSystemOs.read_to_string(path)
    }
    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        FileSystemOs.metadata(path)
    }
    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        FileSystemOs.symlink_metadata(path)
    }
    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        FileSystemOs.read_link(path)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        FileSystemOs.canonicalize(path)
    }
}

#[test]
fn filesystem_observation_preserves_auto_discovery_and_extended_config_reads()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    std::fs::create_dir(fixture.path().join("src"))?;
    let config = fixture.path().join("tsconfig.json");
    let extended = fixture.path().join("base.json");
    std::fs::write(&config, r#"{"extends":"./base.json"}"#)?;
    std::fs::write(
        &extended,
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
    )?;
    std::fs::write(fixture.path().join("src/caller.ts"), "import '@/target';")?;
    std::fs::write(
        fixture.path().join("src/target.ts"),
        "export const target = 1;",
    )?;
    let filesystem = ObservedFileSystem::default();
    let reads = Arc::clone(&filesystem.reads);
    let resolver = OxcModuleResolver::for_node_project(fixture.path(), filesystem)?;
    if resolver.resolve("src/caller.ts", "@/target")? != fixture.path().join("src/target.ts") {
        return Err("observed filesystem changed automatic resolution".into());
    }
    let observed = reads
        .lock()
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !observed.contains(&config) || !observed.contains(&extended) {
        return Err(format!("missing automatically discovered config reads: {observed:?}").into());
    }
    drop(observed);
    let tracked = resolver.observed_paths()?;
    if !tracked.contains(&config) || !tracked.contains(&extended) {
        return Err("adapter did not expose extended config observations".into());
    }
    if resolver.resolve("src/caller.ts", "./missing").is_ok() {
        return Err("missing target unexpectedly resolved".into());
    }
    let before_refresh = resolver.observed_paths()?;
    if !before_refresh.contains(&fixture.path().join("src/missing.ts")) {
        return Err("missing target probe was not tracked".into());
    }
    ModuleResolutionProvider::refresh(&resolver)?;
    if resolver.observed_paths()? != before_refresh {
        return Err("refresh discarded dependency coverage".into());
    }
    Ok(())
}
