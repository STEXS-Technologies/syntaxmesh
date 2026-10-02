use serde::{Deserialize, Serialize};
use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};
use syntaxmesh_language_sdk::SourceFile;
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};

#[cfg(test)]
mod tests;

const SOURCE_INDEX_MARKER_PREFIX: &str = "syntaxmesh.cli.source-index.v1/";

/// Pack ordered source inventory into the existing source-planning fingerprint.
/// Callers must supply deterministic order and append all resolver/configuration
/// inputs before planning. Source content hashes must match the supplied bytes.
///
/// # Errors
/// Returns an error if a pathname length cannot be represented as u64.
pub fn source_inventory_fingerprint(files: &[SourceFile]) -> Result<Vec<u8>, StoreError> {
    let mut fingerprint = Vec::new();
    for file in files {
        let path_bytes = file.file.normalized_path.as_bytes();
        let path_length = u64::try_from(path_bytes.len())
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        fingerprint.extend_from_slice(&path_length.to_le_bytes());
        fingerprint.extend_from_slice(path_bytes);
        fingerprint.extend_from_slice(&file.file.content_hash);
    }
    Ok(fingerprint)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SourceIndexMarker {
    extractor_fingerprint: Vec<u8>,
    source_fingerprint: [u8; 32],
    generation: GenerationId,
}

/// Prepare the existing durable source-index transition and no-op policy.
/// Caller fingerprints must include extraction and resolution configuration.
/// This writes a planning marker, not a graph publication; recover pending
/// workflows before planning and publish needed generations through the Engine.
///
/// # Errors
/// Returns store or marker encoding errors. Invalid markers fail closed.
pub fn prepare_source_index<S: GraphStore + DurableRecordStore>(
    store: &mut S,
    repository: RepositoryId,
    worktree: WorktreeId,
    files: &[SourceFile],
    extractor_fingerprint: &[u8],
    fingerprint: &[u8],
) -> Result<(GenerationId, bool), StoreError> {
    let source_fingerprint = *blake3::hash(fingerprint).as_bytes();
    let snapshot_generation =
        GenerationId::derive(&[b"source-index-v1", extractor_fingerprint, fingerprint]);

    let marker_key = source_index_marker_key(repository, worktree);
    let previous_marker = store
        .read_record(&marker_key)?
        .map(|encoded| {
            serde_json::from_slice::<SourceIndexMarker>(&encoded)
                .map(|marker| (encoded, marker))
                .map_err(|error| encoding_error(&error))
        })
        .transpose()?;
    let current = store.current_generation(repository, worktree)?;
    let current_files_match = if let Some(current) = current.as_ref() {
        let mut current_files = store.files(current.generation)?;
        current_files.sort_by_key(|file| file.file_id);
        let mut source_files = files
            .iter()
            .map(|file| file.file.clone())
            .collect::<Vec<_>>();
        source_files.sort_by_key(|file| file.file_id);
        current_files == source_files
    } else {
        false
    };
    let marker_matches = previous_marker.as_ref().is_some_and(|(_, marker)| {
        marker.source_fingerprint == source_fingerprint
            && marker.extractor_fingerprint == extractor_fingerprint
            && current_files_match
            && store.manifest(marker.generation).is_ok_and(|manifest| {
                manifest.repository == repository && manifest.worktree == worktree
            })
    });
    let (generation, needs_index) = if marker_matches {
        let marker = previous_marker
            .as_ref()
            .map(|(_, marker)| marker.generation)
            .ok_or_else(|| StoreError::Backend("source index marker disappeared".to_owned()))?;
        (marker, false)
    } else if let Some(current) = current.as_ref() {
        let current_entry = store.current_generation_entry(repository, worktree)?;
        let already_indexed_transition = current_entry
            .as_ref()
            .and_then(|entry| entry.delta.as_ref())
            .is_some_and(|delta| {
                transition_generation(delta.expected_base, extractor_fingerprint, fingerprint)
                    == current.generation
            });
        if current.generation == snapshot_generation || already_indexed_transition {
            (current.generation, false)
        } else {
            (
                transition_generation(Some(current.generation), extractor_fingerprint, fingerprint),
                true,
            )
        }
    } else {
        (snapshot_generation, true)
    };

    let marker = SourceIndexMarker {
        extractor_fingerprint: extractor_fingerprint.to_vec(),
        source_fingerprint,
        generation,
    };
    let marker_bytes = serde_json::to_vec(&marker).map_err(|error| encoding_error(&error))?;
    match previous_marker {
        Some((encoded, _)) => {
            store.compare_exchange_record(&marker_key, Some(&encoded), &marker_bytes)?
        }
        None => store.compare_exchange_record(&marker_key, None, &marker_bytes)?,
    }

    Ok((generation, needs_index))
}

fn encoding_error(error: &serde_json::Error) -> StoreError {
    StoreError::Backend(format!("source index marker encoding: {error}"))
}

fn source_index_marker_key(repository: RepositoryId, worktree: WorktreeId) -> String {
    format!(
        "{SOURCE_INDEX_MARKER_PREFIX}{}/{}",
        repository.0.to_hex(),
        worktree.0.to_hex()
    )
}

fn transition_generation(
    base: Option<GenerationId>,
    extractor_fingerprint: &[u8],
    source_fingerprint: &[u8],
) -> GenerationId {
    let base_bytes = base.map_or_else(Vec::new, |generation| generation.0.0.to_vec());
    GenerationId::derive(&[
        b"source-index-transition-v1",
        base_bytes.as_slice(),
        extractor_fingerprint,
        source_fingerprint,
    ])
}
