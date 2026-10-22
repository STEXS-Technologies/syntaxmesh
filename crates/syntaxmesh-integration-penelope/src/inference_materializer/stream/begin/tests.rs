use super::*;
use syntaxmesh_inference::InvalidationLimits;
use syntaxmesh_inference_sqlite::{RebuildRequest, StreamSyncRequest};

fn exercise<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::super::tests::{lifecycle_fixture, lifecycle_limits};
    let store = open()?;
    let mut source_store = store;
    let (rule, retained, retracted) = lifecycle_fixture(&mut source_store)?;
    let rules = [rule];
    let limits = lifecycle_limits();
    let rebuild = RebuildRequest {
        generation: retained,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    InferenceMaterializer::migrate(database)?;
    let baseline = InferenceMaterializer::open(database)?.rebuild(&source_store, &rebuild)?;
    let expected = StreamOrigin {
        stream: StableId::derive("inference-stream-v1", &[&baseline.operation.0]),
        baseline,
    };
    let request = StreamBeginRequest {
        baseline,
        claim_budget: 10,
        byte_budget: 16_384,
        premise_budget: 100,
        page_budget: 4096,
    };
    // This fixture owns the generated database; remove only its effect to force
    // initialization and successor requests to remain prepared across reopen.
    std::fs::remove_file(database)?;
    let mut failed = PenelopeInferenceStream::new(source_store);
    if failed.begin(database, &request).is_ok()
        || failed
            .begin(
                database,
                &StreamBeginRequest {
                    page_budget: 0,
                    ..request
                },
            )
            .is_ok()
    {
        return Err("missing or unbounded initialization falsely completed".into());
    }
    let bounds = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let successor = StreamSyncRequest {
        origin: expected,
        through: retracted,
        rules: &rules,
        limits: &limits,
        range_limits: &bounds,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
        evidence_budget: 100,
        matched_claim_bytes: 16_384,
        page_budget: 4096,
    };
    if failed.sync(database, &successor).is_ok() {
        return Err("successor published without initialization".into());
    }
    let mut journal_store = failed.into_store();
    let records = journal_store
        .records_with_prefix_page(PREFIX, None, 32)?
        .records;
    if records.len() != 1 {
        return Err("initialization did not journal exactly once".into());
    }
    let (key, encoded) = records.first().ok_or("missing initialization journal")?;
    let prepared = journal::decode::<Request>(encoded)?;
    if prepared.receipt.is_some() {
        return Err("failed initialization marked complete".into());
    }
    let mut changed = prepared.request.clone();
    changed.byte_budget = 32_768;
    if journal::key(&changed)? != *key || journal::validate(&prepared, &changed).is_ok() {
        return Err("initialization controls bypassed immutable digest".into());
    }
    drop(journal_store);
    InferenceMaterializer::migrate(database)?;
    InferenceMaterializer::open(database)?.rebuild(&open()?, &rebuild)?;
    let mut recovered = PenelopeInferenceStream::new(open()?);
    if recovered.recover_pending()? != 2
        || recovered.recover_pending()? != 0
        || recovered.begin(database, &request)? != expected
    {
        return Err("recovery did not initialize before applying successor".into());
    }
    let closure = recovered.sync(database, &successor)?;
    if closure.active_claims != 0 || closure.affected_claims != 1 {
        return Err("initialized successor returned wrong receipt".into());
    }
    journal_store = recovered.into_store();
    let completed = journal_store
        .read_record(key)?
        .ok_or("missing initialization completion")?;
    let prepared_again = journal::encode(&journal::prepare(prepared.request)?)?;
    journal_store.compare_exchange_record(key, Some(&completed), &prepared_again)?;
    drop(journal_store);
    let mut interrupted = PenelopeInferenceStream::new(open()?);
    if interrupted.recover_pending()? != 1
        || interrupted.begin(database, &request)? != expected
        || interrupted.sync(database, &successor)? != closure
    {
        return Err("SQL-before-journal initialization reset advanced stream".into());
    }
    let reader = InferenceMaterializer::open(database)?;
    if !reader
        .stream_claims(expected, retracted, 10, 16_384, 100, 4096)?
        .claims
        .is_empty()
        || reader
            .stream_claims(expected, retained, 10, 16_384, 100, 4096)?
            .claims
            .len()
            != 1
    {
        return Err("initialization retry changed historical roots".into());
    }
    drop(reader);
    journal_store = interrupted.into_store();
    let valid = journal_store
        .read_record(key)?
        .ok_or("missing recovered initialization")?;
    let mut corrupt = valid.clone();
    corrupt.push(0);
    journal_store.compare_exchange_record(key, Some(&valid), &corrupt)?;
    let mut tampered = PenelopeInferenceStream::new(journal_store);
    if tampered.recover_pending().is_ok() {
        return Err("corrupt initialization journal accepted".into());
    }
    journal_store = tampered.into_store();
    journal_store.compare_exchange_record(key, Some(&corrupt), &valid)?;
    std::fs::remove_file(database)?;
    InferenceMaterializer::migrate(database)?;
    InferenceMaterializer::open(database)?.rebuild(&journal_store, &rebuild)?;
    if PenelopeInferenceStream::new(journal_store)
        .begin(database, &request)
        .is_ok()
        || InferenceMaterializer::open(database)?
            .stream_checkpoint(expected, retained)
            .is_ok()
    {
        return Err("completed initialization silently recreated missing stream".into());
    }
    Ok(())
}

#[test]
fn file_initialization_recovers_before_successors_without_resetting_advanced_head()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.snapshot");
    exercise(
        || Ok(syntaxmesh_store::FileGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn turso_initialization_recovers_before_successors_without_resetting_advanced_head()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&graph)?;
    exercise(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}
