use std::fs;
use std::path::{Component, Path, PathBuf};
use syntaxmesh_core::FileVersion;
use syntaxmesh_query::ContextSourceProvider;

#[derive(Clone)]
pub struct RepositorySourceReader {
    root: PathBuf,
}

impl ContextSourceProvider for RepositorySourceReader {
    fn read_source(&self, file: &FileVersion) -> Result<Option<Vec<u8>>, String> {
        let relative = Path::new(&file.normalized_path);
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err("indexed source path is not a normalized repository-relative path".into());
        }
        let candidate = self.root.join(relative);
        let canonical = match fs::canonicalize(&candidate) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("canonicalize indexed source path: {error}")),
        };
        if !canonical.starts_with(&self.root) {
            return Err("indexed source path escapes the configured repository root".into());
        }
        fs::read(canonical)
            .map(Some)
            .map_err(|error| format!("read indexed source file: {error}"))
    }
}

impl RepositorySourceReader {
    /// Configure an existing repository directory.
    ///
    /// # Errors
    /// Fails if the root cannot be canonicalized or is not a directory.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = fs::canonicalize(root.as_ref())
            .map_err(|error| format!("canonicalize source root: {error}"))?;
        if !root.is_dir() {
            return Err("source root is not a directory".to_owned());
        }
        Ok(Self { root })
    }
}
