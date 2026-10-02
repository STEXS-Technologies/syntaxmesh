use std::fs::{self, OpenOptions, TryLockError};
use std::path::Path;

use crate::OwnershipError;
use crate::writer_lease::lock_path;

#[cfg(test)]
mod tests;

/// Observe whether a cooperating writer holds an existing store's sidecar lock.
///
/// Does not create files or open the database. A false result is not a lease:
/// acquire and retain `WriterLease` before any direct store operation.
/// Path validation assumes operator-controlled directories, as the lease does.
///
/// # Errors
/// Returns errors for absent or invalid store targets, invalid sidecars, and
/// filesystem failures other than a missing sidecar.
pub fn is_writer_active(target: &Path) -> Result<bool, OwnershipError> {
    let resolved = fs::canonicalize(target)?;
    if !fs::metadata(&resolved)?.is_file() {
        return Err(OwnershipError::InvalidTarget(
            "store target must be a regular file",
        ));
    }
    let path = lock_path(&resolved)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(OwnershipError::InvalidTarget(
                "index ownership sidecar must be a regular file",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    let file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    match file.try_lock() {
        Ok(()) => Ok(false),
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}
