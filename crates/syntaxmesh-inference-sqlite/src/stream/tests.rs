use super::*;
use crate::{RebuildRequest, test_fixture};
use syntaxmesh_store::GraphStore;

#[test]
fn baseline_bounds_rollback_reopen_and_tamper_rejection() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("stream.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, generation, current) = test_fixture::fixture(&mut store)?;
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
    let claims = materializer.claims(baseline, 1, 16_384)?;
    let claim = claims.first().ok_or("missing baseline claim")?;
    let bytes = usize::try_from(bincode::serialized_size(claim)?)?;
    for (count, bound, postings) in [
        (0, bytes, 3),
        (1, bytes.checked_sub(1).ok_or("empty bytes")?, 3),
        (1, bytes, 2),
    ] {
        if materializer
            .begin_stream(baseline, count, bound, postings, 4096)
            .is_ok()
        {
            return Err("stream baseline admitted exhausted bounds".into());
        }
        let published: i64 = materializer.connection.query_row(
            "SELECT COUNT(*) FROM inference_streams",
            [],
            |row| row.get(0),
        )?;
        if published != 0 {
            return Err("failed baseline published a stream".into());
        }
    }
    materializer.connection.execute_batch("CREATE TRIGGER interrupt_stream BEFORE INSERT ON inference_stream_premises BEGIN SELECT RAISE(ABORT, 'injected stream failure'); END;")?;
    if materializer
        .begin_stream(baseline, 1, bytes, 3, 4096)
        .is_ok()
    {
        return Err("stream SQL failure ignored".into());
    }
    for table in [
        "inference_streams",
        "inference_stream_checkpoints",
        "inference_stream_claims",
        "inference_stream_premises",
        "inference_stream_closures",
    ] {
        let rows: i64 = materializer.connection.query_row(
            &format!("SELECT COUNT(*) FROM {table}"),
            [],
            |row| row.get(0),
        )?;
        if rows != 0 {
            return Err("failed stream publication left partial rows".into());
        }
    }
    materializer
        .connection
        .execute_batch("DROP TRIGGER interrupt_stream")?;
    let receipt = materializer.begin_stream(baseline, 1, bytes, 3, 4096)?;
    if materializer.begin_stream(baseline, 1, bytes, 3, 4096)? != receipt {
        return Err("stream baseline retry changed identity".into());
    }
    drop(materializer);
    let mut reopened = InferenceMaterializer::open(&path)?;
    if reopened.begin_stream(baseline, 1, bytes, 3, 4096)? != receipt
        || reopened.claims(baseline, 1, bytes)? != claims
    {
        return Err("stream initialization did not retain baseline across reopen".into());
    }
    // Simulate a later accepted checkpoint; initialization must not rewind it.
    let later_sequence = i64::try_from(store.generation_sequence(current)?)?;
    reopened.connection.execute(
        "INSERT INTO inference_stream_checkpoints VALUES (?1, ?2, ?3, ?4, 0, 1, 1, 0)",
        params![
            receipt.stream.to_hex(),
            current.0.to_hex(),
            later_sequence,
            generation.0.to_hex()
        ],
    )?;
    reopened.connection.execute(
        "UPDATE inference_streams SET head_generation = ?2, head_sequence = ?3 WHERE stream = ?1",
        params![receipt.stream.to_hex(), current.0.to_hex(), later_sequence],
    )?;
    reopened.connection.execute(
        "INSERT INTO inference_stream_roots VALUES (?1, ?2, NULL)",
        params![receipt.stream.to_hex(), current.0.to_hex()],
    )?;
    reopened.connection.execute(
        "UPDATE inference_stream_claims SET closed_generation = ?2, closed_sequence = ?3 WHERE stream = ?1",
        params![receipt.stream.to_hex(), current.0.to_hex(), later_sequence],
    )?;
    if reopened.begin_stream(baseline, 1, bytes, 3, 4096)? != receipt {
        return Err("later coverage changed stream root".into());
    }
    let retained_head: String = reopened.connection.query_row(
        "SELECT head_generation FROM inference_streams WHERE stream = ?1",
        [receipt.stream.to_hex()],
        |row| row.get(0),
    )?;
    if retained_head != current.0.to_hex() {
        return Err("baseline retry rewound later head".into());
    }
    reopened.connection.execute("UPDATE inference_stream_premises SET valid_from = 'tampered' WHERE stream = ?1 AND ordinal = 0", [receipt.stream.to_hex()])?;
    if reopened.begin_stream(baseline, 1, bytes, 3, 4096).is_ok() {
        return Err("corrupt stream posting accepted".into());
    }
    Ok(())
}

#[test]
fn empty_baseline_is_explicit_and_stream_checkpoint_tampering_rejects() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("empty.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, _retained, generation) = test_fixture::fixture(&mut store)?;
    let limits = test_fixture::limits();
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation,
            rules: &[rule],
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 1,
            premise_budget: 1,
        },
    )?;
    if baseline.claim_count != 0 {
        return Err("expected empty baseline".into());
    }
    let receipt = materializer.begin_stream(baseline, 1, 1, 1, 4096)?;
    if materializer.begin_stream(baseline, 1, 1, 1, 4096)? != receipt {
        return Err("empty baseline lost scope".into());
    }
    materializer.connection.execute(
        "UPDATE inference_stream_checkpoints SET active_claims = 1 WHERE stream = ?1",
        [receipt.stream.to_hex()],
    )?;
    if materializer.begin_stream(baseline, 1, 1, 1, 4096).is_ok() {
        return Err("corrupt baseline checkpoint accepted".into());
    }
    Ok(())
}
