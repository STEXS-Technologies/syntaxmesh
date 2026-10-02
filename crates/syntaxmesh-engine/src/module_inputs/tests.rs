use super::*;
use syntaxmesh_store::InMemoryGraphStore;

#[test]
fn file_restart_retains_coverage_and_rejects_noncanonical_records()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("graph.snapshot");
    let repository = RepositoryId::derive(&[b"restart-module-inputs"]);
    let worktree = WorktreeId::derive(&[b"restart-module-inputs"]);
    let mut store = syntaxmesh_store::FileGraphStore::open(&path)?;
    save_module_inputs(
        &mut store,
        repository,
        worktree,
        b"node",
        &["config.json".to_owned()],
    )?;
    drop(store);
    let mut restored = syntaxmesh_store::FileGraphStore::open(&path)?;
    if load_module_inputs(&restored, repository, worktree, b"node")? != ["config.json"] {
        return Err("restart lost resolver input coverage".into());
    }
    let corrupt_key = key(repository, worktree, b"corrupt");
    restored.compare_exchange_record(&corrupt_key, None, br#"["z","a"]"#)?;
    if load_module_inputs(&restored, repository, worktree, b"corrupt").is_ok()
        || save_module_inputs(&mut restored, repository, worktree, b"corrupt", &[]).is_ok()
        || restored.read_record(&corrupt_key)?.as_deref() != Some(br#"["z","a"]"#.as_slice())
    {
        return Err("malformed resolver inventory was accepted or overwritten".into());
    }
    Ok(())
}

#[test]
fn coverage_is_canonical_scoped_and_survives_store_transfer() -> Result<(), StoreError> {
    let mut store = InMemoryGraphStore::new();
    let repository = RepositoryId::derive(&[b"module-inputs"]);
    let worktree = WorktreeId::derive(&[b"module-inputs"]);
    save_module_inputs(
        &mut store,
        repository,
        worktree,
        b"node",
        &["b".to_owned(), "a".to_owned(), "b".to_owned()],
    )?;
    let restored = store;
    if load_module_inputs(&restored, repository, worktree, b"node")? != ["a", "b"]
        || !load_module_inputs(&restored, repository, worktree, b"python")?.is_empty()
        || !load_module_inputs(
            &restored,
            repository,
            WorktreeId::derive(&[b"other"]),
            b"node",
        )?
        .is_empty()
    {
        return Err(StoreError::Integrity(
            "module coverage scope or canonicalization differs".to_owned(),
        ));
    }
    Ok(())
}
