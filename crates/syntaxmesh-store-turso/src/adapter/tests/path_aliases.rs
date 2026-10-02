use super::*;

#[cfg(unix)]
#[test]
fn migration_rejects_dangling_alias_without_creating_target()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("absent.db");
    let alias = fixture.path().join("dangling.db");
    std::os::unix::fs::symlink(&target, &alias)?;
    if TursoGraphStore::migrate(&alias).is_ok()
        || target.exists()
        || fixture.path().join("dangling.db-wal").exists()
    {
        return Err("dangling alias migration created divergent database state".into());
    }
    let missing_parent = fixture.path().join("missing");
    if TursoGraphStore::migrate(missing_parent.join("graph.db")).is_ok() || missing_parent.exists()
    {
        return Err("migration created a missing parent directory".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn aliases_share_wal_schema_and_durable_records() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let real = fixture.path().join("real");
    std::fs::create_dir(&real)?;
    let parent_alias = fixture.path().join("parent-alias");
    std::os::unix::fs::symlink(&real, &parent_alias)?;
    let target = real.join("graph.db");
    TursoGraphStore::migrate(parent_alias.join("graph.db"))?;
    let mut store = TursoGraphStore::open(&target)?;
    store.compare_exchange_record("alias-fixture", None, b"retained")?;
    drop(store);
    let alias = fixture.path().join("file-alias.db");
    std::os::unix::fs::symlink(&target, &alias)?;
    let reopened = TursoGraphStore::open(&alias)?;
    if reopened.read_record("alias-fixture")? != Some(b"retained".to_vec()) {
        return Err("file alias lost WAL-backed durable record".into());
    }
    drop(reopened);
    if TursoGraphStore::migration_status(&alias)? != TursoGraphStore::migration_status(&target)? {
        return Err("alias migration status differs".into());
    }
    TursoGraphStore::migrate(&alias)?;
    let migrated = TursoGraphStore::open(&target)?;
    if migrated.read_record("alias-fixture")? != Some(b"retained".to_vec()) {
        return Err("alias migration lost durable records".into());
    }
    Ok(())
}
