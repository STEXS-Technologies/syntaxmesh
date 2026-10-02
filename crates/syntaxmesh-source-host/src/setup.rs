use std::path::Path;
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_language_sdk::{ExtractorRegistryError, LanguageExtractor, SendCompositeExtractor};
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use crate::{
    ProjectConfig, ProjectResolverError, project_resolvers, repository_scope,
    supported_source_extractors,
};

#[cfg(test)]
mod tests;

/// An Engine and matching structural planning inputs for a configured source host.
pub struct ConfiguredSourceEngine<S> {
    pub engine: SyntaxMeshEngine<S, SendCompositeExtractor>,
    pub extractor_fingerprint: [u8; 32],
    pub resolver_fingerprint: Vec<u8>,
}

/// Failure to configure the bundled source pack or explicit project resolvers.
#[derive(Debug)]
pub enum SourceSetupError {
    Extractors(ExtractorRegistryError),
    Resolvers(ProjectResolverError),
}

impl std::fmt::Display for SourceSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extractors(error) => error.fmt(formatter),
            Self::Resolvers(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SourceSetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Extractors(error) => Some(error),
            Self::Resolvers(error) => Some(error),
        }
    }
}

/// Configure an owned store with the existing supported project source policies.
/// The caller holds writer ownership and supplies a canonical root and its loaded
/// configuration. This does not migrate, recover, publish, or execute semantic AI.
///
/// # Errors
/// Returns bundled registry or explicitly configured resolver setup errors.
pub fn configured_source_engine<S>(
    store: S,
    root: &Path,
    config: &ProjectConfig,
    verify: bool,
) -> Result<ConfiguredSourceEngine<S>, SourceSetupError>
where
    S: GraphStore + DurableRecordStore,
{
    let extractors = supported_source_extractors().map_err(SourceSetupError::Extractors)?;
    let extractor_fingerprint = extractors.configuration_fingerprint();
    let resolvers = project_resolvers(root, config).map_err(SourceSetupError::Resolvers)?;
    let (repository, worktree) = repository_scope(root);
    let mut engine = SyntaxMeshEngine::new(store, extractors, repository, worktree);
    if let Some(provider) = resolvers.provider {
        engine = engine.with_module_resolution_provider(provider);
    }
    if verify || config.verified_history() {
        engine = engine.with_statechronicle_verification();
    }
    Ok(ConfiguredSourceEngine {
        engine,
        extractor_fingerprint,
        resolver_fingerprint: resolvers.fingerprint,
    })
}
