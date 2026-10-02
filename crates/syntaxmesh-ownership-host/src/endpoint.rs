use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{OwnershipError, WriterLease, read_active_discovery};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct OwnerEndpoint {
    record: Record,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema_version: u32,
    store: PathBuf,
    address: SocketAddr,
    instance: String,
}

impl OwnerEndpoint {
    /// Create a fresh host instance record for an existing store and bound listener.
    ///
    /// # Errors
    /// Rejects invalid stores/addresses or OS random-source failure.
    pub fn new(store: &Path, address: SocketAddr) -> Result<Self, OwnershipError> {
        let store = std::fs::canonicalize(store)?;
        if !std::fs::metadata(&store)?.is_file() {
            return Err(OwnershipError::InvalidTarget(
                "endpoint store must be a regular file",
            ));
        }
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err(OwnershipError::InvalidTarget(
                "endpoint requires bound literal loopback address",
            ));
        }
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes)
            .map_err(|error| OwnershipError::Io(std::io::Error::other(error.to_string())))?;
        Ok(Self {
            record: Record {
                schema_version: 1,
                store,
                address,
                instance: hex::encode(bytes),
            },
        })
    }

    /// Literal loopback listener selected for this owner instance.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.record.address
    }

    /// Canonical store path bound to this record.
    #[must_use]
    pub fn store(&self) -> &Path {
        &self.record.store
    }

    /// Fresh instance identity; not an authentication secret.
    #[must_use]
    pub fn instance(&self) -> &str {
        &self.record.instance
    }

    /// Publish through the matching store's held lease.
    ///
    /// # Errors
    /// Fails for a foreign lease, invalid path encoding, or publication failure.
    pub fn publish(&self, lease: &WriterLease) -> Result<(), OwnershipError> {
        if lease.target != self.record.store {
            return Err(OwnershipError::InvalidTarget(
                "endpoint does not match writer lease",
            ));
        }
        lease.publish_discovery(&self.encode()?)
    }

    /// Encode a single bounded discovery JSON object.
    ///
    /// # Errors
    /// Rejects unrepresentable paths or records exceeding 64 KiB.
    pub fn encode(&self) -> Result<Vec<u8>, OwnershipError> {
        let bytes = serde_json::to_vec(&self.record)
            .map_err(|_error| OwnershipError::InvalidTarget("endpoint record cannot be encoded"))?;
        if bytes.len() > 65536 {
            return Err(OwnershipError::InvalidTarget(
                "endpoint record exceeds 64 KiB",
            ));
        }
        Ok(bytes)
    }

    /// Validate untrusted metadata against the requested canonical store.
    /// Does not establish active ownership or authenticate the listener.
    ///
    /// # Errors
    /// Fails for malformed/oversized records, foreign stores, or invalid endpoints.
    pub fn decode(bytes: &[u8], store: &Path) -> Result<Self, OwnershipError> {
        if bytes.len() > 65536 {
            return Err(OwnershipError::InvalidTarget(
                "endpoint record exceeds 64 KiB",
            ));
        }
        let record: Record = serde_json::from_slice(bytes)
            .map_err(|_error| OwnershipError::InvalidTarget("invalid endpoint record"))?;
        let canonical = std::fs::canonicalize(store)?;
        if !std::fs::metadata(&canonical)?.is_file()
            || record.schema_version != 1
            || record.store != canonical
            || !record.address.ip().is_loopback()
            || record.address.port() == 0
            || record.instance.len() != 64
            || !record
                .instance
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(OwnershipError::InvalidTarget(
                "endpoint scope or identity is invalid",
            ));
        }
        Ok(Self { record })
    }

    /// Read and validate an observed active owner's record. Instance validation
    /// through the transport is still required before any attached operation.
    ///
    /// # Errors
    /// Propagates active-owner discovery and record validation failures.
    pub fn discover(store: &Path) -> Result<Option<Self>, OwnershipError> {
        read_active_discovery(store)?
            .map(|bytes| Self::decode(&bytes, store))
            .transpose()
    }
}
