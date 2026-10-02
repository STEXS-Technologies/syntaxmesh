use std::path::Path;
use syntaxmesh_core::{RepositoryId, WorktreeId};

#[test]
fn local_scope_preserves_existing_bytes_and_separates_roots()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new("/workspace/é/project");
    let text = root.to_string_lossy();
    let expected = (
        RepositoryId::derive(&[text.as_bytes()]),
        WorktreeId::derive(&[text.as_bytes(), b"working-tree"]),
    );
    let actual = super::repository_scope(root);
    if actual != expected
        || actual == super::repository_scope(Path::new("/workspace/other/project"))
    {
        return Err("local scope identity changed or roots collapsed".into());
    }
    Ok(())
}
