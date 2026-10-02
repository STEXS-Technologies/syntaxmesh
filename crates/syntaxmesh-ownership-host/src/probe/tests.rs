use super::is_writer_active;
use crate::WriterLease;

#[test]
fn probe_does_not_create_missing_store_or_sidecar() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let store = fixture.path().join("graph.db");
    let sidecar = fixture.path().join("graph.db.syntaxmesh-owner.lock");
    if is_writer_active(&store).is_ok() || store.exists() || sidecar.exists() {
        return Err("probe created missing storage or accepted an absent store".into());
    }
    std::fs::write(&store, b"preserve store")?;
    if is_writer_active(&store)? || sidecar.exists() || std::fs::read(&store)? != b"preserve store"
    {
        return Err("probe created a sidecar or changed storage".into());
    }
    Ok(())
}

#[test]
fn probe_observes_active_writer_and_releases_its_own_lock() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = tempfile::tempdir()?;
    let store = fixture.path().join("graph.db");
    std::fs::write(&store, b"store")?;
    let owner = WriterLease::acquire(&store)?;
    let sidecar = fixture.path().join("graph.db.syntaxmesh-owner.lock");
    std::fs::write(&sidecar, b"preserve descriptor")?;
    if !is_writer_active(&store)? || WriterLease::acquire(&store).is_ok() {
        return Err("probe missed or released the active writer".into());
    }
    drop(owner);
    if is_writer_active(&store)? || is_writer_active(&store)? {
        return Err("probe retained its own lock".into());
    }
    let next = WriterLease::acquire(&store)?;
    if !is_writer_active(&store)? || std::fs::read(&sidecar)? != b"preserve descriptor" {
        return Err("probe changed sidecar contents or prevented reacquisition".into());
    }
    drop(next);
    Ok(())
}

#[test]
fn non_regular_targets_and_sidecars_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    if is_writer_active(fixture.path()).is_ok() {
        return Err("directory store accepted".into());
    }
    let store = fixture.path().join("graph.db");
    std::fs::write(&store, b"store")?;
    std::fs::create_dir(fixture.path().join("graph.db.syntaxmesh-owner.lock"))?;
    if is_writer_active(&store).is_ok() {
        return Err("directory sidecar accepted".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn aliases_share_observation_but_symlink_sidecars_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let store = fixture.path().join("graph.db");
    let alias = fixture.path().join("alias.db");
    std::fs::write(&store, b"store")?;
    std::os::unix::fs::symlink(&store, &alias)?;
    let owner = WriterLease::acquire(&store)?;
    if !is_writer_active(&alias)? {
        return Err("store alias bypassed active ownership".into());
    }
    drop(owner);
    if is_writer_active(&alias)? {
        return Err("store alias retained stale ownership".into());
    }
    let other = fixture.path().join("other.db");
    std::fs::write(&other, b"other")?;
    std::os::unix::fs::symlink(
        &store,
        fixture.path().join("other.db.syntaxmesh-owner.lock"),
    )?;
    if is_writer_active(&other).is_ok() || std::fs::read(&store)? != b"store" {
        return Err("symlink sidecar accepted or modified".into());
    }
    Ok(())
}
