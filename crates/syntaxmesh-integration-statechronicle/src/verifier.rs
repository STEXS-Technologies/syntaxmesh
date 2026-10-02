//! Durable, replay-checkable generation verification using StateChronicle digests.

use serde::{Deserialize, Serialize};
use statechronicle::core::digest::hash_bytes;
use std::collections::BTreeSet;
use syntaxmesh_core::{GenerationId, GenerationManifest, GenerationStatus};
use syntaxmesh_store::{DurableRecordStore, StoreError};
use syntaxmesh_workflow::{GenerationVerifier, WorkflowError, WorkflowStatus};

const HISTORY_PREFIX: &str = "syntaxmesh.statechronicle.generation.v1/";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryRecord {
    ordinal: u64,
    manifest: GenerationManifest,
    manifest_digest: String,
    previous_digest: Option<String>,
    record_digest: String,
}

#[derive(Debug, Clone, Serialize)]
struct HistoryRecordBody<'manifest> {
    ordinal: u64,
    manifest: &'manifest GenerationManifest,
    manifest_digest: &'manifest str,
    previous_digest: &'manifest Option<String>,
}

/// Verifies a manifest and retains a durable StateChronicle digest chain.
#[derive(Debug, Clone, Default)]
pub struct StateChronicleVerifier {
    last_digest: Option<String>,
}

impl StateChronicleVerifier {
    /// Creates an empty verifier.
    #[must_use]
    pub const fn new() -> Self {
        Self { last_digest: None }
    }

    /// Returns the StateChronicle digest of the last verified history record.
    #[must_use]
    pub fn last_digest(&self) -> Option<&str> {
        self.last_digest.as_deref()
    }
}

impl GenerationVerifier for StateChronicleVerifier {
    fn verify(
        &mut self,
        generation: &GenerationManifest,
        records: &mut dyn DurableRecordStore,
    ) -> Result<WorkflowStatus, WorkflowError> {
        if generation.status != GenerationStatus::Durable {
            return Err(WorkflowError::Verification(
                "only durable generations can be verified".to_owned(),
            ));
        }

        let prefix = scope_prefix(generation);
        let manifest_digest = digest(generation)?;
        let latest = records
            .last_record_with_prefix(&prefix)?
            .map(|(key, bytes)| decode_history_record(&prefix, &key, &bytes))
            .transpose()?;

        if let Some((ordinal, head)) = &latest {
            validate_history_head(*ordinal, head, generation)?;
            if head.manifest.generation == generation.generation {
                if head.manifest_digest != manifest_digest {
                    return Err(WorkflowError::Verification(
                        "generation already exists in history with different contents".to_owned(),
                    ));
                }
                self.last_digest = Some(head.record_digest.clone());
                return Ok(WorkflowStatus::Verified);
            }
            if generation.parent != Some(head.manifest.generation) {
                let history = read_history(records, &prefix)?;
                validate_history(&history, generation)?;
                if let Some(existing) = history
                    .iter()
                    .find(|record| record.manifest.generation == generation.generation)
                {
                    if existing.manifest_digest != manifest_digest {
                        return Err(WorkflowError::Verification(
                            "generation already exists in history with different contents"
                                .to_owned(),
                        ));
                    }
                    self.last_digest = Some(existing.record_digest.clone());
                    return Ok(WorkflowStatus::Verified);
                }
                return Err(WorkflowError::Verification(
                    "generation does not extend the verified history head".to_owned(),
                ));
            }
        } else if generation.parent.is_some() {
            return Err(WorkflowError::Verification(
                "generation does not extend the verified history head".to_owned(),
            ));
        }

        let ordinal = latest
            .as_ref()
            .map(|(ordinal, _)| ordinal.checked_add(1))
            .unwrap_or(Some(0))
            .ok_or_else(|| WorkflowError::Verification("history ordinal overflow".to_owned()))?;
        let previous_digest = latest.map(|(_, record)| record.record_digest);
        let body = HistoryRecordBody {
            ordinal,
            manifest: generation,
            manifest_digest: &manifest_digest,
            previous_digest: &previous_digest,
        };
        let record_digest = digest(&body)?;
        let record = HistoryRecord {
            ordinal,
            manifest: generation.clone(),
            manifest_digest,
            previous_digest,
            record_digest: record_digest.clone(),
        };
        let key = format!("{prefix}{ordinal:020}");
        let bytes = serde_json::to_vec(&record)
            .map_err(|error| WorkflowError::Verification(error.to_string()))?;
        match records.compare_exchange_record(&key, None, &bytes) {
            Ok(()) => {
                self.last_digest = Some(record_digest);
                Ok(WorkflowStatus::Verified)
            }
            Err(StoreError::RecordConflict(_)) => {
                // A concurrent verifier may have committed this exact record.
                let history = read_history(records, &prefix)?;
                validate_history(&history, generation)?;
                if let Some(existing) = history
                    .iter()
                    .find(|item| item.manifest.generation == generation.generation)
                    .filter(|item| item.manifest_digest == record.manifest_digest)
                {
                    self.last_digest = Some(existing.record_digest.clone());
                    Ok(WorkflowStatus::Verified)
                } else {
                    Err(WorkflowError::Verification(
                        "concurrent generation history update conflicted".to_owned(),
                    ))
                }
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl StateChronicleVerifier {
    /// Replays and validates every StateChronicle generation record in scope.
    ///
    /// Normal verification validates only the hash-checked head and appends
    /// one link. Use this explicit audit when the integrity of the full
    /// retained chain must be re-established.
    ///
    /// # Errors
    /// Returns a workflow error if any key, record, manifest, or chain link is
    /// missing, malformed, out of scope, or fails digest validation.
    pub fn verify_history(
        &mut self,
        scope: &GenerationManifest,
        records: &dyn DurableRecordStore,
    ) -> Result<Option<String>, WorkflowError> {
        let history = read_history(records, &scope_prefix(scope))?;
        validate_history(&history, scope)?;
        self.last_digest = history.last().map(|record| record.record_digest.clone());
        Ok(self.last_digest.clone())
    }
}

fn scope_prefix(generation: &GenerationManifest) -> String {
    format!(
        "{HISTORY_PREFIX}{}/{}/",
        generation.repository.0.to_hex(),
        generation.worktree.0.to_hex()
    )
}

fn digest<T: Serialize>(value: &T) -> Result<String, WorkflowError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    Ok(hash_bytes(&bytes).to_string())
}

fn read_history(
    records: &dyn DurableRecordStore,
    prefix: &str,
) -> Result<Vec<HistoryRecord>, WorkflowError> {
    records
        .records_with_prefix(prefix)?
        .into_iter()
        .enumerate()
        .map(|(index, (key, bytes))| {
            let expected_key = u64::try_from(index)
                .map(|ordinal| format!("{prefix}{ordinal:020}"))
                .map_err(|error| {
                    WorkflowError::Verification(format!("history is too long: {error}"))
                })?;
            if key != expected_key {
                return Err(WorkflowError::Verification(
                    "generation history keys are missing or out of sequence".to_owned(),
                ));
            }
            serde_json::from_slice(&bytes).map_err(|error| {
                WorkflowError::Verification(format!("invalid history record: {error}"))
            })
        })
        .collect()
}

fn decode_history_record(
    prefix: &str,
    key: &str,
    bytes: &[u8],
) -> Result<(u64, HistoryRecord), WorkflowError> {
    let ordinal_text = key.strip_prefix(prefix).ok_or_else(|| {
        WorkflowError::Verification("generation history key is outside its scope".to_owned())
    })?;
    let ordinal = ordinal_text.parse::<u64>().map_err(|error| {
        WorkflowError::Verification(format!("invalid generation history key: {error}"))
    })?;
    if key != format!("{prefix}{ordinal:020}") {
        return Err(WorkflowError::Verification(
            "generation history key is not canonically ordered".to_owned(),
        ));
    }
    let record: HistoryRecord = serde_json::from_slice(bytes)
        .map_err(|error| WorkflowError::Verification(format!("invalid history record: {error}")))?;
    if record.ordinal != ordinal {
        return Err(WorkflowError::Verification(
            "generation history key ordinal differs from record".to_owned(),
        ));
    }
    Ok((ordinal, record))
}

fn validate_history_head(
    ordinal: u64,
    record: &HistoryRecord,
    requested: &GenerationManifest,
) -> Result<(), WorkflowError> {
    if record.ordinal != ordinal
        || record.manifest.repository != requested.repository
        || record.manifest.worktree != requested.worktree
        || record.manifest.status != GenerationStatus::Durable
        || record.manifest_digest != digest(&record.manifest)?
        || (ordinal == 0 && record.previous_digest.is_some())
        || (ordinal > 0 && (record.previous_digest.is_none() || record.manifest.parent.is_none()))
    {
        return Err(WorkflowError::Verification(
            "generation history head metadata or manifest digest is invalid".to_owned(),
        ));
    }
    let body = HistoryRecordBody {
        ordinal: record.ordinal,
        manifest: &record.manifest,
        manifest_digest: &record.manifest_digest,
        previous_digest: &record.previous_digest,
    };
    if record.record_digest != digest(&body)? {
        return Err(WorkflowError::Verification(
            "generation history head digest is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn validate_history(
    history: &[HistoryRecord],
    requested: &GenerationManifest,
) -> Result<(), WorkflowError> {
    let mut previous: Option<&HistoryRecord> = None;
    let mut generation_ids = BTreeSet::<GenerationId>::new();
    for (index, record) in history.iter().enumerate() {
        let ordinal = u64::try_from(index).map_err(|error| {
            WorkflowError::Verification(format!("history is too long: {error}"))
        })?;
        if record.ordinal != ordinal
            || record.manifest.repository != requested.repository
            || record.manifest.worktree != requested.worktree
            || record.manifest.status != GenerationStatus::Durable
            || record.manifest_digest != digest(&record.manifest)?
            || !generation_ids.insert(record.manifest.generation)
        {
            return Err(WorkflowError::Verification(
                "generation history record metadata or manifest digest is invalid".to_owned(),
            ));
        }
        let body = HistoryRecordBody {
            ordinal: record.ordinal,
            manifest: &record.manifest,
            manifest_digest: &record.manifest_digest,
            previous_digest: &record.previous_digest,
        };
        if record.record_digest != digest(&body)? {
            return Err(WorkflowError::Verification(
                "generation history record digest is invalid".to_owned(),
            ));
        }
        match previous {
            Some(prior)
                if record.previous_digest.as_deref() != Some(prior.record_digest.as_str())
                    || record.manifest.parent != Some(prior.manifest.generation) =>
            {
                return Err(WorkflowError::Verification(
                    "generation history chain is broken".to_owned(),
                ));
            }
            None if record.previous_digest.is_some() => {
                return Err(WorkflowError::Verification(
                    "generation history anchor has a predecessor".to_owned(),
                ));
            }
            _ => {}
        }
        previous = Some(record);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};
    use syntaxmesh_store::{FileGraphStore, InMemoryGraphStore};

    fn manifest(name: &[u8], parent: Option<GenerationId>) -> GenerationManifest {
        GenerationManifest {
            repository: RepositoryId::derive(&[b"repo"]),
            worktree: WorktreeId::derive(&[b"worktree"]),
            generation: GenerationId::derive(&[name]),
            parent,
            graph_root: [1; 32],
            configuration_hash: [2; 32],
            extractor_set_hash: [3; 32],
            schema_version: 1,
            status: GenerationStatus::Durable,
        }
    }

    #[test]
    fn persists_and_replays_generation_history() -> Result<(), WorkflowError> {
        let first = manifest(b"first", None);
        let second = manifest(b"second", Some(first.generation));
        let mut store = InMemoryGraphStore::new();
        let mut verifier = StateChronicleVerifier::new();
        if verifier.verify(&first, &mut store)? != WorkflowStatus::Verified {
            return Err(WorkflowError::Verification(
                "first record failed".to_owned(),
            ));
        }
        let first_digest = verifier.last_digest().map(str::to_owned);
        if verifier.verify(&second, &mut store)? != WorkflowStatus::Verified {
            return Err(WorkflowError::Verification(
                "second record failed".to_owned(),
            ));
        }
        let mut restarted = StateChronicleVerifier::new();
        if restarted.verify(&second, &mut store)? != WorkflowStatus::Verified
            || restarted.last_digest() == first_digest.as_deref()
        {
            return Err(WorkflowError::Verification(
                "history did not replay after verifier restart".to_owned(),
            ));
        }
        Ok(())
    }

    #[test]
    fn rejects_corrupt_history() -> Result<(), WorkflowError> {
        let first = manifest(b"first", None);
        let mut store = InMemoryGraphStore::new();
        StateChronicleVerifier::new().verify(&first, &mut store)?;
        let prefix = scope_prefix(&first);
        let key = format!("{prefix}{:020}", 0);
        let mut bytes = store
            .read_record(&key)?
            .ok_or_else(|| WorkflowError::Verification("history record missing".to_owned()))?;
        let first_byte = bytes.first_mut().ok_or_else(|| {
            WorkflowError::Verification("history record was unexpectedly empty".to_owned())
        })?;
        *first_byte ^= 1;
        store.compare_exchange_record(&key, store.read_record(&key)?.as_deref(), &bytes)?;
        let result = StateChronicleVerifier::new().verify(&first, &mut store);
        if result.is_ok() {
            return Err(WorkflowError::Verification(
                "corrupt history was accepted".to_owned(),
            ));
        }
        Ok(())
    }

    #[test]
    fn normal_append_checks_only_head_and_explicit_audit_replays_chain() -> Result<(), WorkflowError>
    {
        let first = manifest(b"head-only-first", None);
        let second = manifest(b"head-only-second", Some(first.generation));
        let third = manifest(b"head-only-third", Some(second.generation));
        let mut store = InMemoryGraphStore::new();
        let mut verifier = StateChronicleVerifier::new();
        verifier.verify(&first, &mut store)?;
        verifier.verify(&second, &mut store)?;

        let prefix = scope_prefix(&first);
        let first_key = format!("{prefix}{:020}", 0);
        let mut bytes = store.read_record(&first_key)?.ok_or_else(|| {
            WorkflowError::Verification("first history record missing".to_owned())
        })?;
        let first_byte = bytes.first_mut().ok_or_else(|| {
            WorkflowError::Verification("first history record was empty".to_owned())
        })?;
        *first_byte ^= 1;
        store.compare_exchange_record(
            &first_key,
            store.read_record(&first_key)?.as_deref(),
            &bytes,
        )?;

        if verifier.verify(&third, &mut store)? != WorkflowStatus::Verified {
            return Err(WorkflowError::Verification(
                "normal append failed to validate the intact chain head".to_owned(),
            ));
        }
        if verifier.verify_history(&third, &store).is_ok() {
            return Err(WorkflowError::Verification(
                "explicit full-history audit accepted a corrupt prior record".to_owned(),
            ));
        }
        Ok(())
    }

    #[test]
    fn durable_history_survives_file_store_restart() -> Result<(), WorkflowError> {
        let first = manifest(b"restart-first", None);
        let second = manifest(b"restart-second", Some(first.generation));
        let path = std::env::temp_dir().join(format!(
            "syntaxmesh-statechronicle-{}-{}.json",
            std::process::id(),
            first.generation.0.to_hex()
        ));
        let mut store = FileGraphStore::open(&path)?;
        StateChronicleVerifier::new().verify(&first, &mut store)?;
        drop(store);

        let mut reopened = FileGraphStore::open(&path)?;
        let mut verifier = StateChronicleVerifier::new();
        if verifier.verify(&second, &mut reopened)? != WorkflowStatus::Verified {
            return Err(WorkflowError::Verification(
                "reopened store did not verify the next generation".to_owned(),
            ));
        }
        drop(reopened);
        std::fs::remove_file(path).map_err(|error| {
            WorkflowError::Verification(format!("could not remove test store: {error}"))
        })?;
        Ok(())
    }
}
