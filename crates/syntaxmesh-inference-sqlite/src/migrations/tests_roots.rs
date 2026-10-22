use super::*;
use crate::{InferenceMaterializer, RebuildRequest, StreamSyncRequest, test_fixture};
use syntaxmesh_core::GenerationId;
use syntaxmesh_inference::InvalidationLimits;

#[test]
fn explicit_v4_root_backfill_matches_old_roots_and_rolls_back_bad_counts_or_ledger() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("v4.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, declared, retracted) = test_fixture::fixture(&mut store)?;
    test_fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
    let first = GenerationId::derive(&[b"first"]);
    let restored = GenerationId::derive(&[b"restored-declaration"]);
    let empty = GenerationId::derive(&[b"empty-followup"]);
    let limits = test_fixture::limits();
    let rules = [rule];
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation: first,
            rules: &rules,
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
    )?;
    let origin = materializer.begin_stream(baseline, 10, 16_384, 100, 4096)?;
    for through in [declared, retracted, restored, empty] {
        materializer.sync_stream(
            &store,
            &StreamSyncRequest {
                origin,
                through,
                rules: &rules,
                limits: &limits,
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
        )?;
    }
    let mut statement = materializer
        .connection
        .prepare("SELECT generation, root FROM inference_stream_roots ORDER BY generation")?;
    let expected = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    materializer.connection.execute_batch("DROP INDEX inference_contract_evaluations_at; DROP INDEX inference_contract_evaluations_history; DROP TABLE inference_contract_evaluations; DROP TABLE inference_contract_evaluation_sets; DROP TABLE inference_stream_roots; DROP TABLE inference_active_pages; DROP INDEX inference_stream_claim_closed_at; DELETE FROM inference_migrations WHERE version >= 5; PRAGMA user_version = 4;")?;
    drop(materializer);
    if open(&path).is_ok() {
        return Err("ordinary open migrated v4 active roots".into());
    }
    let connection = Connection::open(&path)?;
    connection.execute(
        "UPDATE inference_stream_checkpoints SET active_claims = 1 WHERE generation = ?1",
        [retracted.0.to_hex()],
    )?;
    if migrate(&path).is_ok() {
        return Err("root backfill invented an inconsistent active set".into());
    }
    let partial: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('inference_stream_roots', 'inference_active_pages')", [], |row| row.get(0))?;
    if partial != 0 {
        return Err("bad root backfill did not roll back schema/pages".into());
    }
    connection.execute(
        "UPDATE inference_stream_checkpoints SET active_claims = 0 WHERE generation = ?1",
        [retracted.0.to_hex()],
    )?;
    connection.execute_batch("CREATE TRIGGER interrupt_roots BEFORE INSERT ON inference_migrations WHEN NEW.version = 5 BEGIN SELECT RAISE(ABORT, 'injected root migration failure'); END;")?;
    if migrate(&path).is_ok() {
        return Err("root migration ledger failure ignored".into());
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 4 {
        return Err("failed root migration advanced schema version".into());
    }
    connection.execute_batch("DROP TRIGGER interrupt_roots")?;
    migrate(&path)?;
    let reopened = InferenceMaterializer::open(&path)?;
    let mut roots = reopened
        .connection
        .prepare("SELECT generation, root FROM inference_stream_roots ORDER BY generation")?;
    let actual = roots
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if actual != expected {
        return Err("migrated active roots differ from original persistent roots".into());
    }
    drop(roots);
    for at in [first, declared, retracted, restored, empty] {
        reopened.stream_claims(origin, at, 10, 16_384, 100, 4096)?;
    }
    Ok(())
}
