use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{OwnershipError, WriterLease};

#[cfg(test)]
mod tests;

impl WriterLease {
    /// Atomically publish bounded host discovery bytes while retaining ownership.
    /// The caller must supply and validate its own metadata contract. Published
    /// bytes alone do not establish a live owner or authorize store access.
    ///
    /// # Errors
    /// Rejects bytes exceeding 64 KiB, invalid sidecars, and filesystem failures.
    /// A directory-sync error after rename does not roll back the replacement.
    pub fn publish_discovery(&self, bytes: &[u8]) -> Result<(), OwnershipError> {
        if bytes.len() > 64 * 1024 {
            return Err(OwnershipError::InvalidTarget(
                "discovery metadata exceeds 64 KiB",
            ));
        }
        let parent = self.target.parent().ok_or(OwnershipError::InvalidTarget(
            "discovery target needs a parent directory",
        ))?;
        let path = discovery_path(&self.target)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(OwnershipError::InvalidTarget(
                    "discovery sidecar must be a regular file",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| OwnershipError::Io(error.error))?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

pub(super) fn discovery_path(target: &Path) -> Result<PathBuf, OwnershipError> {
    let mut name = target
        .file_name()
        .ok_or(OwnershipError::InvalidTarget(
            "discovery target needs a filename",
        ))?
        .to_os_string();
    name.push(".syntaxmesh-owner.discovery");
    Ok(target.with_file_name(name))
}
