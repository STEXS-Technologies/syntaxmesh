use std::fs;
use std::io::Read;
use std::path::Path;

use crate::discovery::discovery_path;
use crate::{OwnershipError, is_writer_active};

#[cfg(test)]
mod tests;

/// Read bounded, untrusted metadata only while cooperating ownership is observed.
/// `None` means no observed writer, not permission for unleased database use.
/// A record is not endpoint authentication; validate owner identity separately.
/// Directory trust assumptions match `WriterLease`.
///
/// # Errors
/// Fails for invalid stores, missing/invalid/oversized active-owner records,
/// filesystem errors, or ownership disappearing during the read.
pub fn read_active_discovery(target: &Path) -> Result<Option<Vec<u8>>, OwnershipError> {
    let canonical = fs::canonicalize(target)?;
    if !is_writer_active(&canonical)? {
        return Ok(None);
    }
    let path = discovery_path(&canonical)?;
    if !fs::symlink_metadata(&path)?.file_type().is_file() {
        return Err(OwnershipError::InvalidTarget(
            "discovery sidecar must be a regular file",
        ));
    }
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(OwnershipError::InvalidTarget(
            "discovery sidecar must be a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(OwnershipError::InvalidTarget(
            "discovery metadata exceeds 64 KiB",
        ));
    }
    if !is_writer_active(&canonical)? {
        return Err(OwnershipError::InvalidTarget(
            "owner disappeared while reading discovery",
        ));
    }
    Ok(Some(bytes))
}
