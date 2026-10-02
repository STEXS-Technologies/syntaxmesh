use syntaxmesh_core::{RepositoryId, StableId, WorktreeId};
use syntaxmesh_store::{DurableRecordStore, StoreError};

/// Read sorted resolver input coverage for one host scope and resolver policy.
///
/// # Errors
/// Returns store or malformed-record errors. No filesystem access is performed.
pub fn load_module_inputs<S: DurableRecordStore>(
    store: &S,
    repository: RepositoryId,
    worktree: WorktreeId,
    resolver_fingerprint: &[u8],
) -> Result<Vec<String>, StoreError> {
    store
        .read_record(&key(repository, worktree, resolver_fingerprint))?
        .map(|bytes| decode(&bytes))
        .transpose()
        .map(Option::unwrap_or_default)
}

/// Save resolver coverage through the existing scoped durable CAS record port.
///
/// # Errors
/// Returns encoding, invalid-existing-record, store, or CAS conflict errors.
pub fn save_module_inputs<S: DurableRecordStore>(
    store: &mut S,
    repository: RepositoryId,
    worktree: WorktreeId,
    resolver_fingerprint: &[u8],
    inputs: &[String],
) -> Result<(), StoreError> {
    let key = key(repository, worktree, resolver_fingerprint);
    let previous = store.read_record(&key)?;
    if let Some(bytes) = &previous {
        decode(bytes)?;
    }
    let mut ordered = inputs.to_vec();
    ordered.sort();
    ordered.dedup();
    let bytes = serde_json::to_vec(&ordered).map_err(|error| encoding_error(&error))?;
    if previous.as_deref() != Some(bytes.as_slice()) {
        store.compare_exchange_record(&key, previous.as_deref(), &bytes)?;
    }
    Ok(())
}

fn key(repository: RepositoryId, worktree: WorktreeId, fingerprint: &[u8]) -> String {
    let policy = StableId::derive("module-input-policy-v1", &[fingerprint]);
    format!(
        "syntaxmesh.module-inputs.v1/{}/{}/{}",
        repository.0.to_hex(),
        worktree.0.to_hex(),
        policy.to_hex()
    )
}

fn decode(bytes: &[u8]) -> Result<Vec<String>, StoreError> {
    let paths: Vec<String> =
        serde_json::from_slice(bytes).map_err(|error| encoding_error(&error))?;
    if paths
        .iter()
        .zip(paths.iter().skip(1))
        .any(|(left, right)| left >= right)
    {
        return Err(StoreError::Integrity(
            "module input inventory is not canonical".to_owned(),
        ));
    }
    Ok(paths)
}

fn encoding_error(error: &serde_json::Error) -> StoreError {
    StoreError::Backend(format!("module input inventory encoding: {error}"))
}

#[cfg(test)]
mod tests;
