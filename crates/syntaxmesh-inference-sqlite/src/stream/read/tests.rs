use super::*;
use crate::{RebuildRequest, StreamSyncRequest, test_fixture};
use syntaxmesh_core::StableId;
use syntaxmesh_inference::{InvalidationLimits, Limits, Rule};
use syntaxmesh_store::InMemoryGraphStore;

fn sync(
    materializer: &mut InferenceMaterializer,
    store: &InMemoryGraphStore,
    origin: StreamOrigin,
    through: GenerationId,
    rules: &[Rule],
    limits: &Limits,
) -> Result<StreamCheckpoint> {
    materializer.sync_stream(
        store,
        &StreamSyncRequest {
            origin,
            through,
            rules,
            limits,
            range_limits: &InvalidationLimits {
                generations: 1,
                versions: 100,
                postings: 100,
                claims: 10,
            },
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
            evidence_budget: 100,
            matched_claim_bytes: 16_384,
            page_budget: 4096,
        },
    )
}

#[test]
fn retained_reads_preserve_birth_scope_exact_bounds_and_reopen_without_store() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("read.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = InMemoryGraphStore::new();
    let (rule, declared, retracted) = test_fixture::fixture(&mut store)?;
    test_fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
    let restored = GenerationId::derive(&[b"restored-declaration"]);
    let empty = GenerationId::derive(&[b"empty-followup"]);
    let limits = test_fixture::limits();
    let rules = [rule];
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation: declared,
            rules: &rules,
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
    )?;
    let original = materializer.claims(baseline, 1, 16_384)?;
    let bytes = usize::try_from(bincode::serialized_size(
        original.first().ok_or("missing original claim")?,
    )?)?;
    let origin = materializer.begin_stream(baseline, 1, bytes, 3, 4096)?;
    if materializer
        .stream_claims(origin, declared, 1, bytes, 3, 0)
        .is_ok()
    {
        return Err("stream read admitted zero page budget".into());
    }
    if materializer
        .stream_claims(origin, declared, 1, bytes, 3, 4096)?
        .claims
        != original
    {
        return Err("stream baseline read changed payloads".into());
    }
    for (count, bound, postings) in [
        (0, bytes, 3),
        (1, bytes.checked_sub(1).ok_or("empty bytes")?, 3),
        (1, bytes, 2),
    ] {
        if materializer
            .stream_claims(origin, declared, count, bound, postings, 4096)
            .is_ok()
        {
            return Err("stream read ignored exhausted independent bounds".into());
        }
    }
    sync(
        &mut materializer,
        &store,
        origin,
        retracted,
        &rules,
        &limits,
    )?;
    if !materializer
        .stream_claims(origin, retracted, 1, 1, 1, 4096)?
        .claims
        .is_empty()
        || materializer
            .stream_claims(origin, declared, 1, bytes, 3, 4096)?
            .claims
            != original
        || materializer
            .stream_claims(origin, restored, 1, 16_384, 3, 4096)
            .is_ok()
    {
        return Err("stream read confused retained coverage or future checkpoint".into());
    }
    sync(&mut materializer, &store, origin, restored, &rules, &limits)?;
    sync(&mut materializer, &store, origin, empty, &rules, &limits)?;
    let current = materializer.stream_claims(origin, empty, 1, 16_384, 3, 4096)?;
    if current.checkpoint.selection.generation != empty
        || current
            .claims
            .first()
            .is_none_or(|claim| claim.generation != restored)
    {
        return Err("stream query selection replaced immutable witness birth generation".into());
    }
    let foreign = StreamOrigin {
        stream: StableId::derive("foreign-stream", &[b"x"]),
        ..origin
    };
    if materializer
        .stream_claims(foreign, empty, 1, 16_384, 3, 4096)
        .is_ok()
    {
        return Err("stream read accepted foreign root".into());
    }
    drop(store);
    drop(materializer);
    let reopened = InferenceMaterializer::open(&path)?;
    if reopened.stream_claims(origin, empty, 1, 16_384, 3, 4096)? != current
        || reopened
            .stream_claims(origin, declared, 1, bytes, 3, 4096)?
            .claims
            != original
    {
        return Err("retained reads required a canonical store or rule replay".into());
    }
    Ok(())
}

#[test]
fn payload_interval_missing_and_excess_posting_corruption_rejects() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("corrupt-read.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = InMemoryGraphStore::new();
    let (rule, generation, _current) = test_fixture::fixture(&mut store)?;
    let limits = test_fixture::limits();
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation,
            rules: &[rule],
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
    )?;
    let origin = materializer.begin_stream(baseline, 1, 16_384, 3, 4096)?;
    let claim = materializer
        .stream_claims(origin, generation, 1, 16_384, 3, 4096)?
        .claims
        .into_iter()
        .next()
        .ok_or("missing claim")?;
    materializer.connection.execute(
        "UPDATE inference_stream_claims SET payload_digest = 'corrupt' WHERE stream = ?1",
        [origin.stream.to_hex()],
    )?;
    if materializer
        .stream_claims(origin, generation, 1, 16_384, 3, 4096)
        .is_ok()
    {
        return Err("corrupt stream payload digest accepted".into());
    }
    materializer.connection.execute("UPDATE inference_stream_claims SET payload_digest = ?2, born_sequence = 1 WHERE stream = ?1",
        params![origin.stream.to_hex(), blake3::hash(&bincode::serialize(&claim)?).to_hex().to_string()])?;
    if materializer
        .stream_claims(origin, generation, 1, 16_384, 3, 4096)
        .is_ok()
    {
        return Err("corrupt stream birth sequence accepted".into());
    }
    materializer.connection.execute(
        "UPDATE inference_stream_claims SET born_sequence = ?2 WHERE stream = ?1",
        params![
            origin.stream.to_hex(),
            i64::try_from(baseline.generation_sequence)?
        ],
    )?;
    let reference = claim.premises.first().ok_or("missing premise")?;
    materializer.connection.execute(
        "INSERT INTO inference_stream_premises VALUES (?1, ?2, 3, ?3, ?4)",
        params![
            origin.stream.to_hex(),
            claim.id.to_hex(),
            bincode::serialize(&reference.fact)?,
            reference.valid_from.0.to_hex()
        ],
    )?;
    if materializer
        .stream_claims(origin, generation, 1, 16_384, 3, 4096)
        .is_ok()
    {
        return Err("excess stream witness posting accepted".into());
    }
    materializer.connection.execute(
        "DELETE FROM inference_stream_premises WHERE stream = ?1 AND ordinal IN (1, 3)",
        [origin.stream.to_hex()],
    )?;
    if materializer
        .stream_claims(origin, generation, 1, 16_384, 3, 4096)
        .is_ok()
    {
        return Err("missing stream witness posting accepted".into());
    }
    if materializer.claims(baseline, 1, 16_384)?.len() != 1 {
        return Err("stream corruption changed immutable complete selection".into());
    }
    Ok(())
}
