use crate::{ModuleResolutionProfile, ProjectConfig};
use oxc_resolver::FileSystemOs;
use std::path::Path;
use std::sync::Arc;
use syntaxmesh_core::StableId;
use syntaxmesh_lang_ecmascript::{ModuleResolutionError, OxcModuleResolver};
use syntaxmesh_lang_python::{PythonFileSystemOs, PythonModuleResolver};
use syntaxmesh_resolver::{CompositeModuleResolutionProvider, ModuleResolutionProvider};

#[cfg(test)]
mod tests;

/// Configured providers and matching suffix for source-planning fingerprints.
pub struct ProjectResolvers {
    pub provider: Option<Arc<dyn ModuleResolutionProvider>>,
    pub fingerprint: Vec<u8>,
}

/// Failure to construct an explicitly configured resolver.
#[derive(Debug)]
pub enum ProjectResolverError {
    Node(ModuleResolutionError),
    Python(String),
}

impl std::fmt::Display for ProjectResolverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(error) => write!(formatter, "module resolver setup failed: {error}"),
            Self::Python(message) => {
                write!(formatter, "Python module resolver setup failed: {message}")
            }
        }
    }
}
impl std::error::Error for ProjectResolverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Node(error) => Some(error),
            Self::Python(_) => None,
        }
    }
}

/// Construct explicit project resolvers and matching source cache inputs.
/// Pass a canonical repository root, append the suffix to inventory packing,
/// and install the returned provider on the Engine.
///
/// # Errors
/// Returns invalid configuration, root, or resolver setup errors.
pub fn project_resolvers(
    root: &Path,
    config: &ProjectConfig,
) -> Result<ProjectResolvers, ProjectResolverError> {
    let profiles = config.module_resolution_profiles();
    let mut providers: Vec<Arc<dyn ModuleResolutionProvider>> = Vec::new();
    for profile in &profiles {
        match profile {
            ModuleResolutionProfile::Node => providers.push(Arc::new(
                OxcModuleResolver::for_node_project(root.to_path_buf(), FileSystemOs)
                    .map_err(ProjectResolverError::Node)?,
            )),
            ModuleResolutionProfile::Python => providers.push(Arc::new(
                PythonModuleResolver::new(
                    root.to_path_buf(),
                    PythonFileSystemOs,
                    config.python_source_roots(),
                )
                .map_err(ProjectResolverError::Python)?,
            )),
        }
    }
    let provider = match providers.len() {
        0 => None,
        1 => providers.pop(),
        _ => Some(Arc::new(CompositeModuleResolutionProvider::new(providers))
            as Arc<dyn ModuleResolutionProvider>),
    };
    let mut fingerprint = syntaxmesh_resolver::REFERENCE_RESOLVER_REVISION.to_vec();
    if let Some(resolver) = &provider {
        let identity = resolver.identity();
        let packed = StableId::derive(
            "cli-module-resolver-v1",
            &[
                identity.namespace.as_bytes(),
                identity.version.as_bytes(),
                &identity.settings_fingerprint,
            ],
        );
        fingerprint.extend_from_slice(&packed.0);
    }
    if profiles.contains(&ModuleResolutionProfile::Node) {
        fingerprint.extend_from_slice(b"syntaxmesh-ecmascript-export-binding-v2");
    }
    Ok(ProjectResolvers {
        provider,
        fingerprint,
    })
}
