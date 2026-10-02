//! Shared project host policy, kept outside runtime-neutral engine crates.

use std::path::{Path, PathBuf};

use serde::Deserialize;

const CONFIG_FILE_NAME: &str = "syntaxmesh.toml";

/// A project's optional SyntaxMesh host configuration.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    history: HistoryConfig,
    module_resolution: ModuleResolutionConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct HistoryConfig {
    verified: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ModuleResolutionConfig {
    profile: Option<ModuleResolutionProfile>,
    profiles: Vec<ModuleResolutionProfile>,
    source_roots: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleResolutionProfile {
    Node,
    Python,
}

#[derive(Debug)]
pub enum ProjectConfigError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    AlreadyExists(PathBuf),
}

impl std::fmt::Display for ProjectConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    f,
                    "failed to access project config {}: {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    f,
                    "failed to parse project config {}: {source}",
                    path.display()
                )
            }
            Self::AlreadyExists(path) => write!(
                f,
                "project config {} already exists; it was not overwritten",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ProjectConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::AlreadyExists(_) => None,
        }
    }
}

impl ProjectConfig {
    /// Loads the optional `syntaxmesh.toml` from `root`.
    ///
    /// # Errors
    /// Returns filesystem or TOML parsing errors for an existing configuration.
    pub fn load(root: &Path) -> Result<Self, ProjectConfigError> {
        let path = root.join(CONFIG_FILE_NAME);
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(ProjectConfigError::Io { path, source }),
        };
        toml::from_str(&content).map_err(|source| ProjectConfigError::Parse { path, source })
    }

    /// Returns whether StateChronicle verification is enabled by project policy.
    #[must_use]
    pub const fn verified_history(&self) -> bool {
        self.history.verified
    }

    /// Returns the explicitly selected module-resolution profile, if any.
    #[must_use]
    pub fn module_resolution_profiles(&self) -> Vec<ModuleResolutionProfile> {
        let mut profiles = self.module_resolution.profiles.clone();
        if let Some(profile) = self.module_resolution.profile
            && !profiles.contains(&profile)
        {
            profiles.insert(0, profile);
        }
        profiles
    }

    /// Returns configured ordered Python source roots, defaulting in the
    /// resolver to the repository root when empty.
    #[must_use]
    pub fn python_source_roots(&self) -> &[String] {
        &self.module_resolution.source_roots
    }
}

/// Creates a new minimal project config without replacing an existing path.
///
/// # Errors
/// Returns invalid-root, existing-path, filesystem, or durable-write errors.
pub fn init_project(
    root: &Path,
    verified: bool,
    module_resolution: &[ModuleResolutionProfile],
) -> Result<PathBuf, ProjectConfigError> {
    let root = std::fs::canonicalize(root).map_err(|source| ProjectConfigError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if !root.is_dir() {
        return Err(ProjectConfigError::Io {
            path: root,
            source: std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "project root is not a directory",
            ),
        });
    }
    let path = root.join(CONFIG_FILE_NAME);
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(ProjectConfigError::AlreadyExists(path));
        }
        Err(source) => return Err(ProjectConfigError::Io { path, source }),
    };
    let mut contents = format!("[history]\nverified = {verified}\n");
    match module_resolution {
        [] => {}
        [ModuleResolutionProfile::Node] => {
            contents.push_str("\n[module_resolution]\nprofile = \"node\"\n");
        }
        [ModuleResolutionProfile::Python] => {
            contents.push_str("\n[module_resolution]\nprofile = \"python\"\n");
        }
        profiles => {
            let names = profiles
                .iter()
                .map(|profile| match profile {
                    ModuleResolutionProfile::Node => "\"node\"",
                    ModuleResolutionProfile::Python => "\"python\"",
                })
                .collect::<Vec<_>>()
                .join(", ");
            contents.push_str("\n[module_resolution]\nprofiles = [");
            contents.push_str(&names);
            contents.push_str("]\n");
        }
    }
    if let Err(source) =
        std::io::Write::write_all(&mut file, contents.as_bytes()).and_then(|()| file.sync_all())
    {
        drop(file);
        if let Err(cleanup_error) = std::fs::remove_file(&path)
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(ProjectConfigError::Io {
                path,
                source: cleanup_error,
            });
        }
        return Err(ProjectConfigError::Io { path, source });
    }
    Ok(path)
}

#[cfg(test)]
mod tests;
