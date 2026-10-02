//! Single-entry scoped cache; callers serialize access under the Engine lock.

use std::sync::Arc;
use syntaxmesh_core::{GenerationId, GenerationManifest, RepositoryId, WorktreeId};
use syntaxmesh_query::GenerationIdentifierIndex;

#[derive(PartialEq, Eq)]
struct Identity {
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
    graph_root: [u8; 32],
}

pub(super) struct PlannerCache {
    entry: Option<(Identity, Arc<GenerationIdentifierIndex>)>,
}

impl PlannerCache {
    #[cfg(test)]
    pub(super) fn cached_index(&self) -> Option<Arc<GenerationIdentifierIndex>> {
        self.entry.as_ref().map(|entry| Arc::clone(&entry.1))
    }

    pub(super) const fn new() -> Self {
        Self { entry: None }
    }

    pub(super) fn get_or_build<F>(
        &mut self,
        manifest: &GenerationManifest,
        build: F,
    ) -> Result<Arc<GenerationIdentifierIndex>, String>
    where
        F: FnOnce() -> Result<GenerationIdentifierIndex, String>,
    {
        let identity = Identity {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: manifest.generation,
            graph_root: manifest.graph_root,
        };
        if let Some((cached, index)) = &self.entry
            && *cached == identity
        {
            return Ok(Arc::clone(index));
        }
        self.entry = None;
        let index = Arc::new(build()?);
        self.entry = Some((identity, Arc::clone(&index)));
        Ok(index)
    }
}

#[cfg(test)]
#[path = "planner_cache/tests.rs"]
mod tests;
