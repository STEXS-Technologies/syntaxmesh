use std::cell::Cell;

use syntaxmesh_core::{
    FileId, Node, NodeId, NodeKind, ProvenanceId, SourceLocation, SourceSpan, StableId,
};
use syntaxmesh_extension_sdk::FactBatch;
use syntaxmesh_semantic::{
    SemanticClaim, SemanticDocumentChunk, SemanticEvidence, SemanticProviderIdentity,
    SemanticRequest,
};
use syntaxmesh_store::{DurableRecordStore, FileGraphStore};
use tempfile::tempdir;

use super::{PenelopeSemanticEnricher, RECORD_PREFIX, RecordPhase, decode_record};

struct TestProvider {
    calls: Cell<usize>,
    fail_once: Cell<bool>,
    identity: SemanticProviderIdentity,
}

impl syntaxmesh_semantic::SemanticProvider for TestProvider {
    fn identity(&self) -> SemanticProviderIdentity {
        self.identity.clone()
    }

    fn extract(
        &self,
        request: &SemanticRequest,
    ) -> Result<syntaxmesh_semantic::SemanticOutput, String> {
        self.calls.set(self.calls.get().saturating_add(1));
        if self.fail_once.replace(false) {
            return Err("injected provider interruption".to_owned());
        }
        let prompt_chunk = request
            .prompt_chunks()
            .into_iter()
            .next()
            .ok_or_else(|| "test request unexpectedly had no chunks".to_owned())?;
        Ok(syntaxmesh_semantic::SemanticOutput {
            claims: vec![SemanticClaim {
                subject: "Runtime-neutral engine".to_owned(),
                relation: "supports_runtime_agnostic_hosts".to_owned(),
                object: "multiple hosts".to_owned(),
                evidence: vec![SemanticEvidence {
                    chunk_content_hash: prompt_chunk.content_hash,
                    quote: "supports runtime agnostic hosts".to_owned(),
                }],
            }],
        })
    }
}

fn identity() -> SemanticProviderIdentity {
    SemanticProviderIdentity {
        provider: "fixture".to_owned(),
        model: "fixture-model".to_owned(),
        model_revision: "rev-1".to_owned(),
        prompt_version: "docs-v1".to_owned(),
        prompt_hash: *blake3::hash(b"fixture prompt").as_bytes(),
        configuration_hash: *blake3::hash(b"fixture config").as_bytes(),
    }
}

fn request(
    node: &[u8],
    file: &[u8],
    start: u64,
) -> Result<SemanticRequest, Box<dyn std::error::Error>> {
    request_with_identity(identity(), node, file, start)
}

fn request_with_identity(
    identity: SemanticProviderIdentity,
    node: &[u8],
    file: &[u8],
    start: u64,
) -> Result<SemanticRequest, Box<dyn std::error::Error>> {
    request_with_text(
        identity,
        node,
        file,
        start,
        "Architecture supports runtime agnostic hosts.",
    )
}

fn request_with_text(
    identity: SemanticProviderIdentity,
    node: &[u8],
    file: &[u8],
    start: u64,
    text: &str,
) -> Result<SemanticRequest, Box<dyn std::error::Error>> {
    let end = start.saturating_add(u64::try_from(text.len())?);
    let file_id = FileId::derive(&[file]);
    let node = Node {
        id: NodeId(StableId::derive("test-node", &[node])),
        kind: NodeKind::DocumentChunk,
        name: text.to_owned(),
        owner_file: Some(file_id),
        source: Some(SourceLocation {
            file_id,
            content_hash: *blake3::hash(text.as_bytes()).as_bytes(),
            span: SourceSpan::new(start, end)?,
        }),
        provenance: ProvenanceId(StableId::derive("test-provenance", &[node])),
        extension_payload: None,
    };
    Ok(SemanticRequest::new(
        identity,
        vec![SemanticDocumentChunk {
            node,
            text: text.to_owned(),
            context: Vec::new(),
        }],
    )?)
}

struct BatchProvider {
    identity: SemanticProviderIdentity,
    batch_calls: Cell<usize>,
    request_count: Cell<usize>,
}

struct PartialFailureProvider {
    identity: SemanticProviderIdentity,
    batch_calls: Cell<usize>,
    request_count: Cell<usize>,
}

impl syntaxmesh_semantic::SemanticProvider for BatchProvider {
    fn identity(&self) -> SemanticProviderIdentity {
        self.identity.clone()
    }

    fn extract(
        &self,
        _request: &SemanticRequest,
    ) -> Result<syntaxmesh_semantic::SemanticOutput, String> {
        Err("batch provider should receive extract_many".to_owned())
    }

    fn extract_many(
        &self,
        requests: &[SemanticRequest],
    ) -> Vec<Result<syntaxmesh_semantic::SemanticOutput, String>> {
        if !requests.is_empty() {
            self.batch_calls
                .set(self.batch_calls.get().saturating_add(1));
        }
        self.request_count
            .set(self.request_count.get().saturating_add(requests.len()));
        requests
            .iter()
            .map(|_| Ok(syntaxmesh_semantic::SemanticOutput::default()))
            .collect()
    }
}

impl syntaxmesh_semantic::SemanticProvider for PartialFailureProvider {
    fn identity(&self) -> SemanticProviderIdentity {
        self.identity.clone()
    }

    fn extract(
        &self,
        _request: &SemanticRequest,
    ) -> Result<syntaxmesh_semantic::SemanticOutput, String> {
        Err("partial-failure provider should receive extract_many".to_owned())
    }

    fn extract_many(
        &self,
        requests: &[SemanticRequest],
    ) -> Vec<Result<syntaxmesh_semantic::SemanticOutput, String>> {
        let batch_number = self.batch_calls.get();
        self.batch_calls.set(batch_number.saturating_add(1));
        self.request_count
            .set(self.request_count.get().saturating_add(requests.len()));
        requests
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if batch_number == 0 && index == 1 {
                    Err("injected per-document failure".to_owned())
                } else {
                    Ok(syntaxmesh_semantic::SemanticOutput::default())
                }
            })
            .collect()
    }
}

#[test]
fn enrich_many_retains_successful_document_cache_when_peer_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store_path = temporary.path().join("semantic-partial-failure.snapshot");
    let provider = PartialFailureProvider {
        identity: identity(),
        batch_calls: Cell::new(0),
        request_count: Cell::new(0),
    };
    let requests = vec![
        request_with_text(
            provider.identity.clone(),
            b"document-a",
            b"file-a",
            0,
            "The graph keeps exact source evidence.",
        )?,
        request_with_text(
            provider.identity.clone(),
            b"document-b",
            b"file-b",
            0,
            "Penelope resumes interrupted workflows.",
        )?,
    ];
    let mut store = FileGraphStore::open(&store_path)?;
    {
        let mut enricher = PenelopeSemanticEnricher::new(&mut store);
        if enricher.enrich_many(requests.clone(), &provider).is_ok() {
            return Err("injected per-document failure unexpectedly succeeded".into());
        }
        let records = enricher.store.records_with_prefix(RECORD_PREFIX)?;
        let phases = records
            .iter()
            .map(|(_, bytes)| decode_record(bytes).map(|record| record.phase))
            .collect::<Result<Vec<_>, _>>()?;
        if phases.len() != 2
            || phases
                .iter()
                .filter(|phase| **phase == RecordPhase::Completed)
                .count()
                != 1
            || phases
                .iter()
                .filter(|phase| **phase == RecordPhase::Prepared)
                .count()
                != 1
        {
            return Err(
                "partial failure did not retain one completed and one prepared document job".into(),
            );
        }
        enricher.enrich_many(requests, &provider)?;
        let retried = enricher.store.records_with_prefix(RECORD_PREFIX)?;
        if retried.len() != 2
            || retried.iter().any(|(_, bytes)| match decode_record(bytes) {
                Ok(record) => record.phase != RecordPhase::Completed,
                Err(_) => true,
            })
        {
            return Err("retry did not complete both independent document jobs".into());
        }
    }
    if provider.batch_calls.get() != 2 || provider.request_count.get() != 3 {
        return Err(
            "retry re-ran the successful document instead of only the failed request".into(),
        );
    }
    Ok(())
}

#[test]
fn enrich_many_persists_document_cache_units_independently()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store_path = temporary.path().join("semantic-batches.snapshot");
    let provider = BatchProvider {
        identity: identity(),
        batch_calls: Cell::new(0),
        request_count: Cell::new(0),
    };
    let requests = vec![
        request_with_text(
            provider.identity.clone(),
            b"document-a",
            b"file-a",
            0,
            "The graph keeps exact source evidence.",
        )?,
        request_with_text(
            provider.identity.clone(),
            b"document-b",
            b"file-b",
            0,
            "Penelope resumes interrupted workflows.",
        )?,
    ];
    if requests
        .first()
        .zip(requests.get(1))
        .is_none_or(|(first, second)| first.cache_key() == second.cache_key())
    {
        return Err("test documents did not produce independent cache keys".into());
    }
    let mut store = FileGraphStore::open(&store_path)?;
    {
        let mut enricher = PenelopeSemanticEnricher::new(&mut store);
        enricher.enrich_many(requests.clone(), &provider)?;
        if provider.batch_calls.get() != 1 || provider.request_count.get() != 2 {
            return Err("Penelope did not submit both cache misses in one provider batch".into());
        }
        let records = enricher.store.records_with_prefix(RECORD_PREFIX)?;
        if records.len() != 2
            || records.iter().any(|(_, bytes)| match decode_record(bytes) {
                Ok(record) => record.phase != RecordPhase::Completed,
                Err(_) => true,
            })
        {
            return Err("Penelope did not complete one durable record per document".into());
        }
        enricher.enrich_many(requests, &provider)?;
    }
    if provider.batch_calls.get() != 1 || provider.request_count.get() != 2 {
        return Err("unchanged document requests were not reused independently".into());
    }
    Ok(())
}

#[test]
fn prepared_semantic_job_resumes_after_restart_and_completed_output_rebinds()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store_path = temporary.path().join("graph.snapshot");
    let provider = TestProvider {
        calls: Cell::new(0),
        fail_once: Cell::new(true),
        identity: identity(),
    };
    let first_request = request(b"node-one", b"file-one", 10)?;
    let mut first_store = FileGraphStore::open(&store_path)?;
    {
        let mut first = PenelopeSemanticEnricher::new(&mut first_store);
        if first.enrich(first_request.clone(), &provider).is_ok() {
            return Err("injected semantic provider failure unexpectedly succeeded".into());
        }
        let records = first.store.records_with_prefix(RECORD_PREFIX)?;
        let Some((_, bytes)) = records.first() else {
            return Err("provider failure did not retain prepared semantic work".into());
        };
        if decode_record(bytes)?.phase != RecordPhase::Prepared {
            return Err("provider failure did not leave a prepared semantic job".into());
        }
    }
    drop(first_store);

    let mut recovered_store = FileGraphStore::open(&store_path)?;
    let mut recovered = PenelopeSemanticEnricher::new(&mut recovered_store);
    if recovered.recover_pending(&provider)? != 1 {
        return Err("semantic recovery did not resume the prepared job".into());
    }
    if provider.calls.get() != 2 {
        return Err("recovery did not invoke the provider exactly once after failure".into());
    }

    let second_request = request(b"node-two", b"file-two", 200)?;
    if first_request.cache_key() != second_request.cache_key() {
        return Err("identical content under a new path should share the semantic cache".into());
    }
    let batch: FactBatch = recovered.enrich(second_request, &provider)?;
    if provider.calls.get() != 2 {
        return Err("completed semantic result was not reused".into());
    }
    if batch.provenance.iter().all(|item| {
        item.source.as_ref().is_none_or(|source| {
            source.span.start_byte
                != 200_u64.saturating_add(u64::try_from("Architecture ".len()).unwrap_or(u64::MAX))
        })
    }) {
        return Err("cached quote evidence was not rebound to the new source span".into());
    }
    let completed = recovered.store.records_with_prefix(RECORD_PREFIX)?;
    let Some((_, completed_bytes)) = completed.first() else {
        return Err("completed semantic cache record disappeared".into());
    };
    let record = decode_record(completed_bytes)?;
    if record.phase != RecordPhase::Completed || record.request.is_some() {
        return Err("completed semantic cache did not compact its source request".into());
    }
    Ok(())
}

#[test]
fn changed_semantic_configuration_uses_a_distinct_cache_entry()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store_path = temporary.path().join("graph.snapshot");
    let first_provider = TestProvider {
        calls: Cell::new(0),
        fail_once: Cell::new(false),
        identity: identity(),
    };
    let mut changed_identity = identity();
    changed_identity.model_revision = "rev-2".to_owned();
    let changed_provider = TestProvider {
        calls: Cell::new(0),
        fail_once: Cell::new(false),
        identity: changed_identity.clone(),
    };
    let original = request(b"node", b"file", 0)?;
    let changed = request_with_identity(changed_identity, b"node", b"file", 0)?;
    if original.cache_key() == changed.cache_key() {
        return Err("changed model revision reused the old semantic cache key".into());
    }
    let mut store = FileGraphStore::open(&store_path)?;
    let mut enricher = PenelopeSemanticEnricher::new(&mut store);
    let _first = enricher.enrich(original, &first_provider)?;
    let _second = enricher.enrich(changed, &changed_provider)?;
    if first_provider.calls.get() != 1 || changed_provider.calls.get() != 1 {
        return Err("changed model revision reused a stale cached result".into());
    }
    let cached = enricher.store.records_with_prefix(RECORD_PREFIX)?;
    if cached.len() != 2 {
        return Err("semantic configurations did not retain separate cache entries".into());
    }
    Ok(())
}
