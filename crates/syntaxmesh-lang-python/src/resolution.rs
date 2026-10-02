//! Conservative static resolution for Python source modules.

use std::path::{Component, Path, PathBuf};

use syntaxmesh_resolver::{
    ModuleResolutionMode, ModuleResolutionOutcome, ModuleResolutionProvider,
    ModuleResolutionRequest, ModuleResolverIdentity,
};

/// Filesystem capability needed by static Python source-module lookup.
pub trait PythonModuleFileSystem: Send + Sync {
    /// Returns whether `path` is a regular file in the supplied filesystem.
    fn is_file(&self, path: &Path) -> bool;
}

/// Native filesystem adapter. Embedders can supply a virtual or sandboxed
/// implementation of [`PythonModuleFileSystem`] instead.
#[derive(Debug, Clone, Copy, Default)]
pub struct PythonFileSystemOs;

impl PythonModuleFileSystem for PythonFileSystemOs {
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// Resolver for configured Python source roots and classic `__init__.py`
/// packages. This does not execute Python or inspect the active environment.
pub struct PythonModuleResolver<Fs> {
    project_root: PathBuf,
    source_roots: Vec<PathBuf>,
    file_system: Fs,
    identity: ModuleResolverIdentity,
}

impl<Fs> PythonModuleResolver<Fs>
where
    Fs: PythonModuleFileSystem + 'static,
{
    /// Creates a resolver with ordered repository-relative source roots.
    /// Empty roots default to the repository root.
    ///
    /// # Errors
    /// Returns an error for a relative project root or invalid source root.
    pub fn new(
        project_root: impl Into<PathBuf>,
        file_system: Fs,
        source_roots: &[String],
    ) -> Result<Self, String> {
        let project_root = project_root.into();
        if !project_root.is_absolute() {
            return Err("Python resolver project root must be absolute".to_owned());
        }
        let source_roots = if source_roots.is_empty() {
            vec![PathBuf::new()]
        } else {
            source_roots
                .iter()
                .map(|root| normalize_source_root(root))
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut fingerprint_input = Vec::new();
        fingerprint_input.extend_from_slice(b"syntaxmesh-python-classic-packages-v1");
        for root in &source_roots {
            let root_text = root.to_string_lossy();
            let length = u64::try_from(root_text.len())
                .map_err(|error| format!("source root is too long: {error}"))?;
            fingerprint_input.extend_from_slice(&length.to_le_bytes());
            fingerprint_input.extend_from_slice(root_text.as_bytes());
        }
        let fingerprint = blake3::hash(&fingerprint_input);
        let identity = ModuleResolverIdentity {
            namespace: "syntaxmesh/python-source-resolver".to_owned(),
            version: "syntaxmesh-python-classic-imports-v1".to_owned(),
            settings_fingerprint: fingerprint.as_bytes().to_vec(),
        };
        Ok(Self {
            project_root,
            source_roots,
            file_system,
            identity,
        })
    }

    fn resolve_request(&self, request: &ModuleResolutionRequest) -> ModuleResolutionOutcome {
        let source_path = Path::new(&request.source_path);
        if !is_normalized_relative_path(source_path) || !has_python_extension(source_path) {
            return ModuleResolutionOutcome::Invalid("invalid Python source path".to_owned());
        }
        let parts = match request.mode {
            ModuleResolutionMode::PythonModule => absolute_module_parts(&request.specifier),
            ModuleResolutionMode::PythonFrom | ModuleResolutionMode::PythonStar => {
                self.relative_or_absolute_parts(source_path, &request.specifier)
            }
            ModuleResolutionMode::Import | ModuleResolutionMode::CommonJs => None,
        };
        let Some(parts) = parts else {
            return ModuleResolutionOutcome::Invalid(
                "invalid Python import or unsupported resolution mode".to_owned(),
            );
        };
        if parts.is_empty() {
            return ModuleResolutionOutcome::Invalid("Python import names no module".to_owned());
        }

        for source_root in &self.source_roots {
            if strip_source_root(source_path, source_root).is_none() {
                continue;
            }
            let absolute_root = self.project_root.join(source_root);
            if let Some(target) = self.resolve_parts(&absolute_root, &parts) {
                let relative = source_root.join(target);
                let path = relative.to_string_lossy().replace('\\', "/");
                return ModuleResolutionOutcome::Resolved(path);
            }
        }
        ModuleResolutionOutcome::Unresolved("no classic Python source module matched".to_owned())
    }

    fn relative_or_absolute_parts(
        &self,
        source_path: &Path,
        specifier: &str,
    ) -> Option<Vec<String>> {
        let dot_count = specifier.bytes().take_while(|byte| *byte == b'.').count();
        if dot_count == 0 {
            return absolute_module_parts(specifier);
        }
        let suffix = &specifier[dot_count..];
        let suffix_parts = if suffix.is_empty() {
            Vec::new()
        } else {
            split_module_parts(suffix)?
        };
        for source_root in &self.source_roots {
            let Some(root_relative) = strip_source_root(source_path, source_root) else {
                continue;
            };
            let mut package_parts = path_parent_parts(root_relative)?;
            if root_relative.file_name().and_then(|name| name.to_str()) == Some("__init__.py") {
                package_parts = path_parent_parts(root_relative.parent()?)?;
                package_parts.push(root_relative.parent()?.file_name()?.to_str()?.to_owned());
            }
            let absolute_root = self.project_root.join(source_root);
            if package_parts.is_empty()
                || !self.package_chain_exists(&absolute_root, &package_parts)
            {
                return None;
            }
            let parents_to_remove = dot_count.saturating_sub(1);
            if parents_to_remove > package_parts.len() {
                return None;
            }
            let package_len = package_parts.len().checked_sub(parents_to_remove)?;
            package_parts.truncate(package_len);
            if package_parts.is_empty() {
                return None;
            }
            package_parts.extend(suffix_parts);
            return Some(package_parts);
        }
        None
    }

    fn resolve_parts(&self, source_root: &Path, parts: &[String]) -> Option<PathBuf> {
        if parts.iter().any(|part| !is_valid_module_segment(part)) {
            return None;
        }
        let (last, parents) = parts.split_last()?;
        let mut prefix = PathBuf::new();
        for part in parents {
            prefix.push(part);
            let init_file = source_root.join(&prefix).join("__init__.py");
            if !self.file_system.is_file(&init_file) {
                return None;
            }
        }
        let mut package = prefix.clone();
        package.push(last);
        let package_init = source_root.join(&package).join("__init__.py");
        if self.file_system.is_file(&package_init) {
            return Some(package.join("__init__.py"));
        }
        let module = source_root.join(package.with_extension("py"));
        self.file_system
            .is_file(&module)
            .then(|| package.with_extension("py"))
    }

    fn package_chain_exists(&self, source_root: &Path, package_parts: &[String]) -> bool {
        let mut package = PathBuf::new();
        package_parts.iter().all(|part| {
            package.push(part);
            self.file_system
                .is_file(&source_root.join(&package).join("__init__.py"))
        })
    }
}

impl<Fs> ModuleResolutionProvider for PythonModuleResolver<Fs>
where
    Fs: PythonModuleFileSystem + 'static,
{
    fn identity(&self) -> &ModuleResolverIdentity {
        &self.identity
    }

    fn supports(&self, request: &ModuleResolutionRequest) -> bool {
        matches!(
            request.mode,
            ModuleResolutionMode::PythonModule
                | ModuleResolutionMode::PythonFrom
                | ModuleResolutionMode::PythonStar
        ) && Path::new(&request.source_path)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
    }

    fn resolve(&self, request: &ModuleResolutionRequest) -> ModuleResolutionOutcome {
        self.resolve_request(request)
    }
}

fn normalize_source_root(root: &str) -> Result<PathBuf, String> {
    let path = Path::new(root);
    if path.is_absolute() {
        return Err(format!("Python source root must be relative: {root}"));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Python source root escapes the project: {root}"));
            }
        }
    }
    Ok(normalized)
}

fn is_normalized_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn has_python_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
}

fn strip_source_root<'source>(
    source_path: &'source Path,
    source_root: &Path,
) -> Option<&'source Path> {
    if source_root.as_os_str().is_empty() {
        Some(source_path)
    } else {
        source_path.strip_prefix(source_root).ok()
    }
}

fn absolute_module_parts(specifier: &str) -> Option<Vec<String>> {
    split_module_parts(specifier)
}

fn split_module_parts(specifier: &str) -> Option<Vec<String>> {
    let parts = specifier.split('.').map(str::to_owned).collect::<Vec<_>>();
    (!parts.is_empty() && parts.iter().all(|part| is_valid_module_segment(part))).then_some(parts)
}

fn is_valid_module_segment(segment: &str) -> bool {
    !segment.is_empty() && segment != "." && segment != ".." && !segment.contains(['/', '\\'])
}

fn path_parent_parts(path: &Path) -> Option<Vec<String>> {
    path.parent()?
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_owned),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use syntaxmesh_core::NodeId;
    use syntaxmesh_resolver::{
        ModuleResolutionMode, ModuleResolutionOutcome, ModuleResolutionProvider,
        ModuleResolutionRequest,
    };

    use super::{PythonModuleFileSystem, PythonModuleResolver};

    #[derive(Default)]
    struct FixtureFileSystem {
        files: BTreeSet<String>,
    }

    impl PythonModuleFileSystem for FixtureFileSystem {
        fn is_file(&self, path: &Path) -> bool {
            self.files
                .contains(&path.to_string_lossy().replace('\\', "/"))
        }
    }

    fn request(
        source: &str,
        specifier: &str,
        mode: ModuleResolutionMode,
    ) -> ModuleResolutionRequest {
        ModuleResolutionRequest {
            occurrence: NodeId::derive(&[b"python-import"]),
            source_path: source.to_owned(),
            specifier: specifier.to_owned(),
            mode,
        }
    }

    fn resolver() -> Result<PythonModuleResolver<FixtureFileSystem>, String> {
        let file_system = FixtureFileSystem {
            files: [
                "/repo/src/pkg/__init__.py",
                "/repo/src/pkg.py",
                "/repo/src/pkg/mod.py",
                "/repo/src/pkg/helpers.py",
                "/repo/src/pkg/sub/__init__.py",
                "/repo/src/pkg/sub/client.py",
                "/repo/src/standalone.py",
                "/repo/other/standalone.py",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        };
        PythonModuleResolver::new(
            "/repo",
            file_system,
            &["src".to_owned(), "other".to_owned()],
        )
    }

    #[test]
    fn resolves_absolute_modules_and_classic_packages_in_configured_root_order()
    -> Result<(), String> {
        let resolver = resolver()?;
        let package = resolver.resolve(&request(
            "src/pkg/mod.py",
            "pkg",
            ModuleResolutionMode::PythonFrom,
        ));
        let module = resolver.resolve(&request(
            "src/pkg/mod.py",
            "pkg.sub.client",
            ModuleResolutionMode::PythonModule,
        ));
        if package != ModuleResolutionOutcome::Resolved("src/pkg/__init__.py".to_owned())
            || module != ModuleResolutionOutcome::Resolved("src/pkg/sub/client.py".to_owned())
        {
            return Err("absolute Python module/package resolution was incorrect".to_owned());
        }
        Ok(())
    }

    #[test]
    fn resolves_relative_imports_from_package_context_and_rejects_overflow() -> Result<(), String> {
        let resolver = resolver()?;
        let sibling = resolver.resolve(&request(
            "src/pkg/sub/client.py",
            "..helpers",
            ModuleResolutionMode::PythonFrom,
        ));
        let parent_overflow = resolver.resolve(&request(
            "src/pkg/mod.py",
            "...missing",
            ModuleResolutionMode::PythonFrom,
        ));
        let top_level_overflow = resolver.resolve(&request(
            "src/pkg/mod.py",
            "..helpers",
            ModuleResolutionMode::PythonFrom,
        ));
        if sibling != ModuleResolutionOutcome::Resolved("src/pkg/helpers.py".to_owned())
            || !matches!(parent_overflow, ModuleResolutionOutcome::Invalid(_))
            || !matches!(top_level_overflow, ModuleResolutionOutcome::Invalid(_))
        {
            return Err("relative Python import handling was incorrect".to_owned());
        }
        Ok(())
    }

    #[test]
    fn provider_ignores_node_modes_and_reports_missing_source_modules() -> Result<(), String> {
        let resolver = resolver()?;
        let node_request = request("src/pkg/mod.py", "pkg", ModuleResolutionMode::Import);
        let missing = resolver.resolve(&request(
            "src/pkg/mod.py",
            "pkg.not_found",
            ModuleResolutionMode::PythonModule,
        ));
        if resolver.supports(&node_request)
            || !matches!(missing, ModuleResolutionOutcome::Unresolved(_))
        {
            return Err("Python provider claimed Node work or misclassified a miss".to_owned());
        }
        Ok(())
    }

    #[test]
    fn rejects_source_roots_that_escape_the_repository() -> Result<(), String> {
        if PythonModuleResolver::new(
            "/repo",
            FixtureFileSystem::default(),
            &["../outside".to_owned()],
        )
        .is_ok()
        {
            return Err("Python resolver accepted an escaping source root".to_owned());
        }
        Ok(())
    }
}
