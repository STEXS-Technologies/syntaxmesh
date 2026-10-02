use crate::{WriterLease, read_active_discovery};

#[test]
fn active_records_are_bounded_and_stale_records_are_ignored()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    std::fs::write(&target, b"store")?;
    let record = fixture.path().join("graph.db.syntaxmesh-owner.discovery");
    if read_active_discovery(&target)?.is_some() || record.exists() {
        return Err("inactive read created discovery".into());
    }
    let lease = WriterLease::acquire(&target)?;
    if read_active_discovery(&target).is_ok() {
        return Err("active owner without discovery allowed fallback".into());
    }
    let expected = vec![1; 65536];
    lease.publish_discovery(&expected)?;
    if read_active_discovery(&target)?.as_deref() != Some(expected.as_slice()) {
        return Err("maximum-sized active discovery did not round trip".into());
    }
    std::fs::write(&record, vec![2; 65537])?;
    if read_active_discovery(&target).is_ok() {
        return Err("oversized active discovery accepted".into());
    }
    drop(lease);
    if read_active_discovery(&target)?.is_some() || std::fs::read(&target)? != b"store" {
        return Err("stale discovery used or store changed".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn aliases_read_the_same_record_and_symlink_records_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    std::fs::write(&target, b"store")?;
    let alias = fixture.path().join("alias.db");
    std::os::unix::fs::symlink(&target, &alias)?;
    let lease = WriterLease::acquire(&target)?;
    lease.publish_discovery(b"record")?;
    if read_active_discovery(&alias)?.as_deref() != Some(b"record".as_slice()) {
        return Err("alias did not read canonical owner discovery".into());
    }
    let other = fixture.path().join("other.db");
    std::fs::write(&other, b"other")?;
    let other_lease = WriterLease::acquire(&other)?;
    std::os::unix::fs::symlink(
        &target,
        fixture.path().join("other.db.syntaxmesh-owner.discovery"),
    )?;
    if read_active_discovery(&other).is_ok() {
        return Err("symlink discovery was accepted".into());
    }
    drop(other_lease);
    Ok(())
}
