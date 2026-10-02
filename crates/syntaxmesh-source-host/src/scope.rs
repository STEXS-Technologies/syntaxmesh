use std::path::Path;
use syntaxmesh_core::{RepositoryId, WorktreeId};

#[cfg(test)]
mod tests;

/// Derive the existing local source scope from a canonical repository root.
///
/// This preserves CLI path identity, including its lossy UTF-8 encoding. It does
/// not discover Git identity or canonicalize the caller's path.
#[must_use]
pub fn repository_scope(root: &Path) -> (RepositoryId, WorktreeId) {
    let root_text = root.to_string_lossy();
    (
        RepositoryId::derive(&[root_text.as_bytes()]),
        WorktreeId::derive(&[root_text.as_bytes(), b"working-tree"]),
    )
}
