use super::WriterLease as IndexOwnership;

#[test]
fn directory_target_does_not_create_a_sidecar() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    std::fs::create_dir(&target)?;
    if IndexOwnership::acquire(&target).is_ok()
        || fixture
            .path()
            .join("graph.db.syntaxmesh-owner.lock")
            .exists()
    {
        return Err("directory target acquired ownership or created a sidecar".into());
    }
    Ok(())
}

#[test]
fn directory_sidecar_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    std::fs::create_dir(fixture.path().join("graph.db.syntaxmesh-owner.lock"))?;
    if IndexOwnership::acquire(&target).is_ok() || target.exists() {
        return Err("directory sidecar was accepted or created a store".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn new_target_parent_alias_shares_ownership() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let parent = fixture.path().join("real");
    std::fs::create_dir(&parent)?;
    let alias = fixture.path().join("alias");
    std::os::unix::fs::symlink(&parent, &alias)?;
    let target = parent.join("graph.db");
    let owner = IndexOwnership::acquire(&target)?;
    if IndexOwnership::acquire(&alias.join("graph.db")).is_ok() || target.exists() {
        return Err("new store parent alias bypassed ownership".into());
    }
    drop(owner);
    drop(IndexOwnership::acquire(&alias.join("graph.db"))?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn dangling_target_and_symlink_sidecar_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    let absent = fixture.path().join("absent.db");
    std::os::unix::fs::symlink(&absent, &target)?;
    if IndexOwnership::acquire(&target).is_ok()
        || fixture
            .path()
            .join("graph.db.syntaxmesh-owner.lock")
            .exists()
        || absent.exists()
    {
        return Err("dangling target acquired a divergent lease".into());
    }
    let second = fixture.path().join("second.db");
    let destination = fixture.path().join("unrelated-file");
    std::fs::write(&destination, b"preserve")?;
    std::os::unix::fs::symlink(
        &destination,
        fixture.path().join("second.db.syntaxmesh-owner.lock"),
    )?;
    if IndexOwnership::acquire(&second).is_ok()
        || second.exists()
        || std::fs::read(&destination)? != b"preserve"
    {
        return Err("symlink sidecar was accepted or modified its destination".into());
    }
    Ok(())
}

#[test]
fn ownership_is_exclusive_and_persistent_sidecar_survives_release()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    let owner = IndexOwnership::acquire(&target)?;
    if IndexOwnership::acquire(&target).is_ok() {
        return Err("competing index owner acquired lease".into());
    }
    if target.exists() {
        return Err("ownership created database".into());
    }
    drop(owner);
    let next = IndexOwnership::acquire(&target)?;
    if !fixture
        .path()
        .join("graph.db.syntaxmesh-owner.lock")
        .is_file()
    {
        return Err("ownership sidecar disappeared".into());
    }
    drop(next);
    Ok(())
}

#[cfg(unix)]
#[test]
fn existing_symlink_alias_uses_the_same_lease() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    std::fs::write(&target, [])?;
    let alias = fixture.path().join("alias.db");
    std::os::unix::fs::symlink(&target, &alias)?;
    let owner = IndexOwnership::acquire(&target)?;
    if IndexOwnership::acquire(&alias).is_ok() {
        return Err("symlink alias bypassed ownership".into());
    }
    drop(owner);
    drop(IndexOwnership::acquire(&alias)?);
    Ok(())
}
