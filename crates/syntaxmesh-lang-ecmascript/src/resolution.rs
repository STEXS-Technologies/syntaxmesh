//! Oxc-backed ECMAScript module resolution, separate from source extraction.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

#[path = "resolution/observed_filesystem.rs"]
mod observed_filesystem;
use observed_filesystem::{ObservedFileSystem, ObservedPaths};

use oxc_resolver::{FileSystem, ResolveError, ResolveOptions, ResolverGeneric};
use syntaxmesh_resolver::{
    ModuleResolutionMode, ModuleResolutionOutcome, ModuleResolutionProvider,
    ModuleResolutionRequest, ModuleResolverIdentity,
};

#[cfg(test)]
#[path = "resolution/cache_lifetime/tests.rs"]
mod cache_lifetime_tests;
#[cfg(test)]
#[path = "resolution/input_tracking/tests.rs"]
mod input_tracking_tests;

/// ECMAScript resolver bound to an explicitly supplied filesystem and project root.
///
/// This adapter does not decide project conditions or read the filesystem on its
/// own: the host supplies both the filesystem implementation and Oxc options.
/// That allows native and virtual filesystems to use the same resolution logic.
pub struct OxcModuleResolver<Fs> {
    project_root: PathBuf,
    import_resolver: ResolverGeneric<ObservedFileSystem<Fs>>,
    commonjs_resolver: ResolverGeneric<ObservedFileSystem<Fs>>,
    identity: ModuleResolverIdentity,
    cache_access: Mutex<()>,
    observed_paths: ObservedPaths,
}

/// Errors from invalid repository-relative source paths or Oxc resolution.
#[derive(Debug)]
pub enum ModuleResolutionError {
    /// Project roots must be absolute because Oxc resolves from absolute paths.
    RelativeProjectRoot(PathBuf),
    /// The source path must be a non-empty normalized repository-relative path.
    InvalidSourcePath(String),
    /// The Node resolver was asked to process a non-ECMAScript import mode.
    UnsupportedMode,
    /// Oxc could not resolve the request under the supplied options/filesystem.
    Resolver(ResolveError),
    /// Cache access was poisoned by a previous failed operation.
    CachePoisoned,
}

impl std::fmt::Display for ModuleResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ModuleResolutionError {}

impl From<ResolveError> for ModuleResolutionError {
    fn from(error: ResolveError) -> Self {
        Self::Resolver(error)
    }
}

impl<Fs> OxcModuleResolver<Fs>
where
    Fs: FileSystem + 'static,
{
    /// Creates a resolver using the supplied virtual/native filesystem and options.
    ///
    /// # Errors
    /// Returns an error if `project_root` is not absolute.
    pub fn new(
        project_root: impl Into<PathBuf>,
        file_system: Fs,
        options: ResolveOptions,
    ) -> Result<Self, ModuleResolutionError> {
        let project_root = project_root.into();
        if !project_root.is_absolute() {
            return Err(ModuleResolutionError::RelativeProjectRoot(project_root));
        }
        let settings = format!("{options:?}");
        let import_options = options_for_mode(options.clone(), "import");
        let commonjs_options = options_for_mode(options, "require");
        let observed_paths = Arc::new(Mutex::new(std::collections::BTreeSet::new()));
        let file_system = ObservedFileSystem {
            inner: file_system,
            paths: Arc::clone(&observed_paths),
        };
        let import_resolver = ResolverGeneric::new_with_file_system(file_system, import_options);
        let commonjs_resolver = import_resolver.clone_with_options(commonjs_options);
        let identity = ModuleResolverIdentity {
            namespace: "syntaxmesh/oxc-module-resolver".to_owned(),
            version: "syntaxmesh-node-resolution-v2+oxc-11.24.3".to_owned(),
            settings_fingerprint: blake3::hash(settings.as_bytes()).as_bytes().to_vec(),
        };
        Ok(Self {
            project_root,
            import_resolver,
            commonjs_resolver,
            identity,
            cache_access: Mutex::new(()),
            observed_paths,
        })
    }

    /// Sorted filesystem paths probed by this adapter, including missing paths.
    /// Retained across refresh; this is not a content fingerprint or snapshot.
    ///
    /// # Errors
    /// Returns an error if synchronized adapter observations are poisoned.
    pub fn observed_paths(&self) -> Result<Vec<PathBuf>, ModuleResolutionError> {
        let _guard = self
            .cache_access
            .lock()
            .map_err(|_error| ModuleResolutionError::CachePoisoned)?;
        let paths = self
            .observed_paths
            .lock()
            .map_err(|_error| ModuleResolutionError::CachePoisoned)?;
        Ok(paths.iter().cloned().collect())
    }

    /// Creates the explicit Node profile used by the local CLI.
    ///
    /// It enables TypeScript config discovery and the ECMAScript pack's source
    /// extensions; package conditions are selected per import occurrence.
    ///
    /// # Errors
    /// Returns an error if `project_root` is not absolute.
    pub fn for_node_project(
        project_root: impl Into<PathBuf>,
        file_system: Fs,
    ) -> Result<Self, ModuleResolutionError> {
        let mut options = ResolveOptions::default()
            .with_extension(".ts")
            .with_extension(".tsx")
            .with_extension(".js")
            .with_extension(".jsx")
            .with_extension(".mjs")
            .with_extension(".cjs");
        options.tsconfig = Some(oxc_resolver::TsconfigDiscovery::Auto);
        options.condition_names = vec!["node".to_owned()];
        Self::new(project_root, file_system, options)
    }

    /// Resolves a specifier from one indexed repository-relative source file.
    ///
    /// Oxc's result may point outside the indexed module inventory (for example
    /// into `node_modules`); callers must only create graph edges to indexed
    /// module nodes.
    ///
    /// # Errors
    /// Returns an error for a malformed source path or Oxc resolution failure.
    pub fn resolve(
        &self,
        source_path: &str,
        specifier: &str,
    ) -> Result<PathBuf, ModuleResolutionError> {
        self.resolve_with_mode(source_path, specifier, ModuleResolutionMode::Import)
    }

    /// Resolves a module under the conditions associated with its source syntax.
    ///
    /// # Errors
    /// Returns an error for a malformed source path or Oxc resolution failure.
    pub fn resolve_with_mode(
        &self,
        source_path: &str,
        specifier: &str,
        mode: ModuleResolutionMode,
    ) -> Result<PathBuf, ModuleResolutionError> {
        let _cache_guard = self
            .cache_access
            .lock()
            .map_err(|_error| ModuleResolutionError::CachePoisoned)?;
        let source_path = Path::new(source_path);
        if source_path.as_os_str().is_empty()
            || source_path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(ModuleResolutionError::InvalidSourcePath(
                source_path.to_string_lossy().into_owned(),
            ));
        }
        let absolute_source = self.project_root.join(source_path);
        let resolver = match mode {
            ModuleResolutionMode::Import => &self.import_resolver,
            ModuleResolutionMode::CommonJs => &self.commonjs_resolver,
            ModuleResolutionMode::PythonModule
            | ModuleResolutionMode::PythonFrom
            | ModuleResolutionMode::PythonStar => {
                return Err(ModuleResolutionError::UnsupportedMode);
            }
        };
        let resolution = resolver.resolve_file(absolute_source, specifier)?;
        Ok(resolution.path().to_owned())
    }
}

impl<Fs> ModuleResolutionProvider for OxcModuleResolver<Fs>
where
    Fs: FileSystem + 'static,
{
    fn observed_inputs(&self) -> Result<Vec<String>, String> {
        self.observed_paths()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|path| {
                path.into_os_string()
                    .into_string()
                    .map_err(|path| format!("module input path is not UTF-8: {path:?}"))
            })
            .collect()
    }
    fn refresh(&self) -> Result<(), String> {
        let _cache_guard = self
            .cache_access
            .lock()
            .map_err(|error| error.to_string())?;
        self.import_resolver.clear_cache();
        self.commonjs_resolver.clear_cache();
        Ok(())
    }
    fn identity(&self) -> &ModuleResolverIdentity {
        &self.identity
    }

    fn supports(&self, request: &ModuleResolutionRequest) -> bool {
        matches!(
            request.mode,
            ModuleResolutionMode::Import | ModuleResolutionMode::CommonJs
        ) && Path::new(&request.source_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"
                )
            })
    }

    fn resolve(&self, request: &ModuleResolutionRequest) -> ModuleResolutionOutcome {
        match OxcModuleResolver::resolve_with_mode(
            self,
            &request.source_path,
            &request.specifier,
            request.mode,
        ) {
            Err(ModuleResolutionError::CachePoisoned) => ModuleResolutionOutcome::Invalid(
                "module resolver cache lock is poisoned".to_owned(),
            ),
            Ok(path) => match path.strip_prefix(&self.project_root) {
                Ok(relative)
                    if relative
                        .components()
                        .all(|component| matches!(component, Component::Normal(_))) =>
                {
                    ModuleResolutionOutcome::Resolved(relative.to_string_lossy().replace('\\', "/"))
                }
                _ => ModuleResolutionOutcome::ResolvedButNotIndexed,
            },
            Err(ModuleResolutionError::InvalidSourcePath(_))
            | Err(ModuleResolutionError::RelativeProjectRoot(_)) => {
                ModuleResolutionOutcome::Invalid("invalid repository-relative request".to_owned())
            }
            Err(ModuleResolutionError::UnsupportedMode) => {
                ModuleResolutionOutcome::Invalid("unsupported Node resolution mode".to_owned())
            }
            Err(ModuleResolutionError::Resolver(ResolveError::NotFound(_)))
            | Err(ModuleResolutionError::Resolver(ResolveError::MatchedAliasNotFound(..)))
            | Err(ModuleResolutionError::Resolver(ResolveError::Ignored(_))) => {
                ModuleResolutionOutcome::Unresolved("no matching module".to_owned())
            }
            Err(ModuleResolutionError::Resolver(ResolveError::Builtin { .. })) => {
                ModuleResolutionOutcome::ResolvedButNotIndexed
            }
            Err(ModuleResolutionError::Resolver(_)) => ModuleResolutionOutcome::Invalid(
                "resolver configuration or request could not be evaluated".to_owned(),
            ),
        }
    }
}

fn options_for_mode(mut options: ResolveOptions, mode: &str) -> ResolveOptions {
    options
        .condition_names
        .retain(|condition| condition != "import" && condition != "require");
    options.condition_names.push(mode.to_owned());
    options
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxc_resolver::{FileSystemOs, ResolveOptions, TsconfigDiscovery};
    use tempfile::tempdir;

    use super::{ModuleResolutionError, OxcModuleResolver};
    use syntaxmesh_core::NodeId;
    use syntaxmesh_resolver::{
        ModuleResolutionMode, ModuleResolutionOutcome, ModuleResolutionProvider,
        ModuleResolutionRequest,
    };

    fn options() -> ResolveOptions {
        ResolveOptions::default()
            .with_extension(".ts")
            .with_extension(".tsx")
            .with_extension(".js")
            .with_extension(".jsx")
            .with_extension(".mjs")
            .with_extension(".cjs")
    }

    #[test]
    fn resolves_extensionless_relative_typescript_module() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = tempdir()?;
        fs::create_dir_all(root.path().join("src"))?;
        fs::write(root.path().join("src/main.ts"), "import './lib';")?;
        fs::write(root.path().join("src/lib.ts"), "export const value = 1;")?;
        let resolver = OxcModuleResolver::new(root.path(), FileSystemOs, options())?;
        let target = resolver.resolve("src/main.ts", "./lib")?;
        if !target.ends_with("src/lib.ts") {
            return Err(format!("unexpected resolution target: {}", target.display()).into());
        }
        Ok(())
    }

    #[test]
    fn resolves_package_exports_using_explicit_import_conditions()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let source = root.path().join("src/main.js");
        let package = root.path().join("node_modules/sample-package");
        fs::create_dir_all(source.parent().ok_or("source parent missing")?)?;
        fs::create_dir_all(package.join("esm"))?;
        fs::create_dir_all(package.join("cjs"))?;
        fs::write(&source, "import 'sample-package';")?;
        fs::write(
            package.join("package.json"),
            r#"{"exports":{"import":"./esm/index.js","require":"./cjs/index.js"}}"#,
        )?;
        fs::write(package.join("esm/index.js"), "export {};")?;
        fs::write(package.join("cjs/index.js"), "module.exports = {};")?;
        let import_options = options().with_condition_names(&["node", "import"]);
        let resolver = OxcModuleResolver::new(root.path(), FileSystemOs, import_options)?;
        let target = resolver.resolve("src/main.js", "sample-package")?;
        let commonjs_target = resolver.resolve_with_mode(
            "src/main.js",
            "sample-package",
            ModuleResolutionMode::CommonJs,
        )?;
        if !target.ends_with("node_modules/sample-package/esm/index.js") {
            return Err(format!("unexpected package target: {}", target.display()).into());
        }
        if !commonjs_target.ends_with("node_modules/sample-package/cjs/index.js") {
            return Err(
                format!("unexpected CommonJS target: {}", commonjs_target.display()).into(),
            );
        }
        Ok(())
    }

    #[test]
    fn resolves_typescript_path_mapping_from_project_config()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        fs::create_dir_all(root.path().join("src"))?;
        fs::write(
            root.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        )?;
        fs::write(root.path().join("src/main.ts"), "import '@/lib';")?;
        fs::write(root.path().join("src/lib.ts"), "export const value = 1;")?;
        let mut configured_options = options();
        configured_options.tsconfig = Some(TsconfigDiscovery::Auto);
        configured_options.condition_names = vec!["node".to_owned(), "import".to_owned()];
        let resolver = OxcModuleResolver::new(root.path(), FileSystemOs, configured_options)?;
        let target = resolver.resolve("src/main.ts", "@/lib")?;
        if !target.ends_with("src/lib.ts") {
            return Err(format!("unexpected tsconfig path target: {}", target.display()).into());
        }
        Ok(())
    }

    #[test]
    fn provider_returns_portable_unresolved_and_invalid_outcomes()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        fs::create_dir_all(root.path().join("src"))?;
        fs::write(root.path().join("src/main.ts"), "import './missing';")?;
        let resolver = OxcModuleResolver::for_node_project(root.path(), FileSystemOs)?;
        let unresolved = ModuleResolutionProvider::resolve(
            &resolver,
            &ModuleResolutionRequest {
                occurrence: NodeId::derive(&[b"portable-unresolved-occurrence"]),
                source_path: "src/main.ts".to_owned(),
                specifier: "./missing".to_owned(),
                mode: ModuleResolutionMode::Import,
            },
        );
        let invalid = ModuleResolutionProvider::resolve(
            &resolver,
            &ModuleResolutionRequest {
                occurrence: NodeId::derive(&[b"portable-invalid-occurrence"]),
                source_path: "../main.ts".to_owned(),
                specifier: "./missing".to_owned(),
                mode: ModuleResolutionMode::Import,
            },
        );
        if unresolved != ModuleResolutionOutcome::Unresolved("no matching module".to_owned())
            || invalid
                != ModuleResolutionOutcome::Invalid(
                    "invalid repository-relative request".to_owned(),
                )
        {
            return Err(
                format!("unexpected portable outcomes: {unresolved:?}, {invalid:?}").into(),
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_non_repository_relative_source_paths() -> Result<(), ModuleResolutionError> {
        let root = std::env::current_dir()
            .map_err(|error| ModuleResolutionError::InvalidSourcePath(error.to_string()))?;
        let resolver = OxcModuleResolver::new(root, FileSystemOs, options())?;
        if !matches!(
            resolver.resolve("../outside.ts", "./target"),
            Err(ModuleResolutionError::InvalidSourcePath(_))
        ) {
            return Err(ModuleResolutionError::InvalidSourcePath(
                "parent traversal was not rejected".to_owned(),
            ));
        }
        Ok(())
    }
}
