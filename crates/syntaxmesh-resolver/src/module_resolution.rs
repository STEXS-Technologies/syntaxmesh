use std::sync::Arc;

use syntaxmesh_core::{NodeId, StableId};

#[cfg(test)]
mod tests;

/// Stable identity and settings fingerprint for a module-resolution provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleResolverIdentity {
    pub namespace: String,
    pub version: String,
    pub settings_fingerprint: Vec<u8>,
}

/// One source-backed import or re-export presented to a host-supplied resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleResolutionRequest {
    pub occurrence: NodeId,
    pub source_path: String,
    pub specifier: String,
    pub mode: ModuleResolutionMode,
}

/// Package-condition mode implied by the source occurrence's syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleResolutionMode {
    Import,
    CommonJs,
    PythonModule,
    PythonFrom,
    PythonStar,
}

/// Runtime-neutral result. A resolved path is repository-relative and uses `/` separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleResolutionOutcome {
    Resolved(String),
    Unresolved(String),
    Ambiguous(Vec<String>),
    Invalid(String),
    /// The provider resolved the request, but the target is outside its
    /// repository-relative graph scope and cannot be named as an indexed path.
    ResolvedButNotIndexed,
}

/// Host-injected module lookup. Implementations may consult filesystem or project configuration;
/// the indexer owns indexed-target filtering and graph publication.
pub trait ModuleResolutionProvider: Send + Sync {
    fn identity(&self) -> &ModuleResolverIdentity;

    /// Refresh transient observations before preparing a new generation.
    /// Implementations must synchronize refresh with their resolution calls.
    ///
    /// # Errors
    /// Returns a diagnostic when observations cannot be safely refreshed.
    fn refresh(&self) -> Result<(), String> {
        Ok(())
    }

    /// Opaque UTF-8 host paths consulted by this provider, including missing inputs.
    /// These are not graph identities; hosts own persistence and fingerprinting.
    ///
    /// # Errors
    /// Returns a diagnostic if observations cannot be safely represented or read.
    fn observed_inputs(&self) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    /// Whether this provider understands the requested source and import mode.
    /// Unsupported requests remain source facts without being classified as
    /// failed resolution attempts.
    fn supports(&self, _request: &ModuleResolutionRequest) -> bool {
        true
    }

    fn resolve(&self, request: &ModuleResolutionRequest) -> ModuleResolutionOutcome;
}

/// Stable ordered composition of language-specific module resolvers.
pub struct CompositeModuleResolutionProvider {
    identity: ModuleResolverIdentity,
    providers: Vec<Arc<dyn ModuleResolutionProvider>>,
}

impl CompositeModuleResolutionProvider {
    /// Creates a language router. Provider order is part of its identity and
    /// the first provider claiming support handles an occurrence.
    #[must_use]
    pub fn new(providers: Vec<Arc<dyn ModuleResolutionProvider>>) -> Self {
        let mut fingerprint_input = Vec::new();
        for provider in &providers {
            let identity = provider.identity();
            append_identity_field(&mut fingerprint_input, identity.namespace.as_bytes());
            append_identity_field(&mut fingerprint_input, identity.version.as_bytes());
            append_identity_field(&mut fingerprint_input, &identity.settings_fingerprint);
        }
        let fingerprint =
            StableId::derive("module-resolution-provider-set-v1", &[&fingerprint_input]);
        Self {
            identity: ModuleResolverIdentity {
                namespace: "syntaxmesh/composite-module-resolver".to_owned(),
                version: "1".to_owned(),
                settings_fingerprint: fingerprint.0.to_vec(),
            },
            providers,
        }
    }
}

impl ModuleResolutionProvider for CompositeModuleResolutionProvider {
    fn observed_inputs(&self) -> Result<Vec<String>, String> {
        let mut inputs = std::collections::BTreeSet::new();
        for provider in &self.providers {
            inputs.extend(provider.observed_inputs()?);
        }
        Ok(inputs.into_iter().collect())
    }
    fn refresh(&self) -> Result<(), String> {
        for provider in &self.providers {
            provider.refresh()?;
        }
        Ok(())
    }
    fn identity(&self) -> &ModuleResolverIdentity {
        &self.identity
    }

    fn supports(&self, request: &ModuleResolutionRequest) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.supports(request))
    }

    fn resolve(&self, request: &ModuleResolutionRequest) -> ModuleResolutionOutcome {
        self.providers
            .iter()
            .find(|provider| provider.supports(request))
            .map_or_else(
                || ModuleResolutionOutcome::Invalid("no provider supports this request".to_owned()),
                |provider| provider.resolve(request),
            )
    }
}

fn append_identity_field(target: &mut Vec<u8>, field: &[u8]) {
    let length = u64::try_from(field.len()).unwrap_or(u64::MAX);
    target.extend_from_slice(&length.to_le_bytes());
    target.extend_from_slice(field);
}
