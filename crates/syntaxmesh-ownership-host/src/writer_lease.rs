use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::OwnershipError;

#[cfg(test)]
mod tests;

/// Exclusive host lease for cooperating store writers.
/// Dropping the guard releases the lock but preserves its sidecar.
pub struct WriterLease {
    _file: File,
    pub(super) target: PathBuf,
}

impl WriterLease {
    /// Canonical store target protected by this held lease.
    #[must_use]
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Acquire ownership before opening a store for mutation.
    ///
    /// # Errors
    /// Returns an error for invalid paths, competing ownership, or filesystem failures.
    pub fn acquire(target: &Path) -> Result<Self, OwnershipError> {
        let resolved = match fs::canonicalize(target) {
            Ok(path) => {
                if !fs::metadata(&path)?.is_file() {
                    return Err(OwnershipError::InvalidTarget(
                        "store target must be a regular file",
                    ));
                }
                path
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::symlink_metadata(target) {
                    Ok(_) => {
                        return Err(OwnershipError::InvalidTarget(
                            "store target must not be a dangling symlink",
                        ));
                    }
                    Err(metadata_error)
                        if metadata_error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(metadata_error) => return Err(OwnershipError::Io(metadata_error)),
                }
                let name = target.file_name().ok_or(OwnershipError::InvalidTarget(
                    "index target needs a filename",
                ))?;
                let parent = target
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                fs::create_dir_all(parent)?;
                fs::canonicalize(parent)?.join(name)
            }
            Err(error) => return Err(OwnershipError::Io(error)),
        };
        let lock_path = lock_path(&resolved)?;
        if fs::symlink_metadata(&lock_path).is_ok_and(|metadata| !metadata.file_type().is_file()) {
            return Err(OwnershipError::InvalidTarget(
                "index ownership sidecar must be a regular file",
            ));
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        match file.try_lock() {
            Ok(()) => Ok(Self {
                _file: file,
                target: resolved,
            }),
            Err(TryLockError::WouldBlock) => Err(OwnershipError::AlreadyOwned),
            Err(TryLockError::Error(error)) => Err(OwnershipError::Io(error)),
        }
    }
}

pub(super) fn lock_path(resolved: &Path) -> Result<PathBuf, OwnershipError> {
    let mut name = resolved
        .file_name()
        .ok_or(OwnershipError::InvalidTarget(
            "index target needs a filename",
        ))?
        .to_os_string();
    name.push(".syntaxmesh-owner.lock");
    Ok(resolved.with_file_name(name))
}
