use crate::{WriterLease, is_writer_active};

#[test]
fn publication_replaces_complete_metadata_without_replacing_the_lock()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    std::fs::write(&target, b"graph bytes")?;
    let lease = WriterLease::acquire(&target)?;
    let record = fixture.path().join("graph.db.syntaxmesh-owner.discovery");
    let lock = fixture.path().join("graph.db.syntaxmesh-owner.lock");
    std::fs::write(&lock, b"lock identity")?;
    for bytes in [b"initial".as_slice(), b"replacement".as_slice()] {
        lease.publish_discovery(bytes)?;
        if std::fs::read(&record)? != bytes
            || std::fs::read(&lock)? != b"lock identity"
            || std::fs::read(&target)? != b"graph bytes"
            || !is_writer_active(&target)?
            || WriterLease::acquire(&target).is_ok()
            || std::fs::read_dir(fixture.path())?.count() != 3
        {
            return Err("discovery publication broke contents, lease, or temporary cleanup".into());
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&record)?.permissions().mode() & 0o077 != 0 {
            return Err("discovery bytes are not owner-only".into());
        }
    }
    drop(lease);
    if is_writer_active(&target)? {
        return Err("stale discovery record retained ownership".into());
    }
    Ok(())
}

#[test]
fn oversized_and_directory_records_preserve_existing_data() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    let lease = WriterLease::acquire(&target)?;
    let record = fixture.path().join("graph.db.syntaxmesh-owner.discovery");
    if lease.publish_discovery(&vec![0; 65537]).is_ok() || record.exists() || target.exists() {
        return Err("oversized discovery publication changed filesystem".into());
    }
    lease.publish_discovery(&vec![1; 65536])?;
    if lease.publish_discovery(&vec![2; 65537]).is_ok() || std::fs::read(&record)? != vec![1; 65536]
    {
        return Err("oversized replacement changed the previous record".into());
    }
    let second = fixture.path().join("second.db");
    let second_lease = WriterLease::acquire(&second)?;
    std::fs::create_dir(fixture.path().join("second.db.syntaxmesh-owner.discovery"))?;
    if second_lease.publish_discovery(b"invalid").is_ok() || second.exists() {
        return Err("directory discovery sidecar accepted".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_record_is_rejected_without_changing_its_destination()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let target = fixture.path().join("graph.db");
    let lease = WriterLease::acquire(&target)?;
    let unrelated = fixture.path().join("unrelated");
    std::fs::write(&unrelated, b"preserve")?;
    std::os::unix::fs::symlink(
        &unrelated,
        fixture.path().join("graph.db.syntaxmesh-owner.discovery"),
    )?;
    if lease.publish_discovery(b"invalid").is_ok() || std::fs::read(&unrelated)? != b"preserve" {
        return Err("symlink discovery was accepted or changed its destination".into());
    }
    Ok(())
}
