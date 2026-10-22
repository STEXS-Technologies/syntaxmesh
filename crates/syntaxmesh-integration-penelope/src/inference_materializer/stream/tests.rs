use super::*;
use syntaxmesh_inference_sqlite::RebuildRequest;

fn exercise<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::tests::{lifecycle_fixture, lifecycle_limits, lifecycle_restore};
    let mut store = open()?;
    let (rule, retained, retracted) = lifecycle_fixture(&mut store)?;
    lifecycle_restore(&mut store, &rule, retained, retracted)?;
    let restored = GenerationId::derive(&[b"restored-declaration"]);
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
    let mut materializer = InferenceMaterializer::open(database)?;
    let baseline = materializer.rebuild(&store, &rebuild)?;
    let origin = materializer.begin_stream(baseline, 10, 16_384, 100, 4096)?;
    let bounds = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let request = StreamSyncRequest {
        origin,
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
    drop(materializer);
    // Only the generated temporary fixture database is removed to simulate a
    // failed effect. Canonical graph and durable requests remain intact.
    std::fs::remove_file(database)?;
    let mut failed = PenelopeInferenceStream::new(store);
    // Prepare in reverse order; recovery must use sequence, not insertion/hash.
    if failed
        .sync(
            database,
            &StreamSyncRequest {
                through: restored,
                ..request
            },
        )
        .is_ok()
        || failed.sync(database, &request).is_ok()
    {
        return Err("missing stream effect falsely completed".into());
    }
    store = failed.into_store();
    let records = store.records_with_prefix_page(PREFIX, None, 32)?.records;
    if records.len() != 2 {
        return Err("missing prepared successor records".into());
    }
    let plans = records
        .iter()
        .map(|(_, bytes)| journal::decode::<Request>(bytes).map(|record| record.request))
        .collect::<Result<Vec<_>, _>>()?;
    if plans.first().is_none_or(|plan| plan.through != retracted)
        || plans.last().is_none_or(|plan| plan.through != restored)
    {
        return Err("recovery keys do not order canonical successors".into());
    }
    let original_plan = plans.first().ok_or("missing first plan")?;
    let prepared = journal::prepare(original_plan.clone())?;
    let mut changed = original_plan.clone();
    changed.page_budget = 8192;
    if journal::key(&changed)? != journal::key(original_plan)?
        || journal::validate(&prepared, &changed).is_ok()
    {
        return Err("stream controls changed semantic identity or bypassed digest".into());
    }
    changed = original_plan.clone();
    changed
        .rules
        .first_mut()
        .ok_or("missing rule")?
        .version
        .push('x');
    if journal::key(&changed).is_ok() {
        return Err("stream rule substitution bypassed baseline configuration".into());
    }
    changed = original_plan.clone();
    changed.sequence = 0;
    let mut invalid_sequence = PenelopeInferenceStream::new(open()?);
    match invalid_sequence.run(&changed) {
        Err(error) if error.to_string().contains("canonical sequence mismatch") => {}
        _ => return Err("stream canonical sequence substitution reached effect".into()),
    }
    drop(store);
    InferenceMaterializer::migrate(database)?;
    let mut rebuilt_materializer = InferenceMaterializer::open(database)?;
    let rebuilt = rebuilt_materializer.rebuild(&open()?, &rebuild)?;
    if rebuilt_materializer.begin_stream(rebuilt, 10, 16_384, 100, 4096)? != origin {
        return Err("fixture baseline did not preserve stream identity".into());
    }
    drop(rebuilt_materializer);
    let mut recovered = PenelopeInferenceStream::new(open()?);
    if recovered.recover_pending()? != 2 || recovered.recover_pending()? != 0 {
        return Err("ordered recovery did not publish both successors".into());
    }
    let closure = recovered.sync(database, &request)?;
    let restoration = recovered.sync(
        database,
        &StreamSyncRequest {
            through: restored,
            ..request
        },
    )?;
    if closure.active_claims != 0
        || closure.affected_claims != 1
        || restoration.active_claims != 1
        || restoration.added_claims != 1
    {
        return Err("recovered stream lifetimes differ from expected witnesses".into());
    }
    if recovered
        .sync(
            database,
            &StreamSyncRequest {
                output_bytes: 32_768,
                ..request
            },
        )
        .is_ok()
    {
        return Err("changed immutable controls accepted".into());
    }
    store = recovered.into_store();
    // Simulate the SQL effect committed but journal completion was lost, for
    // both generations, including an older target below the current head.
    for ((key, _), plan) in records.iter().zip(&plans) {
        let completed = store.read_record(key)?.ok_or("missing completed journal")?;
        let prepared_bytes = journal::encode(&journal::prepare(plan.clone())?)?;
        store.compare_exchange_record(key, Some(&completed), &prepared_bytes)?;
    }
    drop(store);
    let mut interrupted = PenelopeInferenceStream::new(open()?);
    if interrupted.recover_pending()? != 2
        || interrupted.sync(database, &request)? != closure
        || interrupted.sync(
            database,
            &StreamSyncRequest {
                through: restored,
                ..request
            },
        )? != restoration
    {
        return Err("SQL-before-journal recovery lost retained checkpoints".into());
    }
    let retained_reader = InferenceMaterializer::open(database)?;
    if retained_reader
        .stream_claims(origin, restored, 10, 16_384, 100, 4096)?
        .claims
        .len()
        != 1
        || !retained_reader
            .stream_claims(origin, retracted, 10, 16_384, 100, 4096)?
            .claims
            .is_empty()
    {
        return Err("old recovery rewound current or retained active roots".into());
    }
    drop(retained_reader);
    store = interrupted.into_store();
    let key = &records.first().ok_or("missing journal key")?.0;
    let valid = store.read_record(key)?.ok_or("missing recovered journal")?;
    let mut corrupt = valid.clone();
    corrupt.push(0);
    store.compare_exchange_record(key, Some(&valid), &corrupt)?;
    let mut tampered = PenelopeInferenceStream::new(store);
    if tampered.recover_pending().is_ok() {
        return Err("corrupt stream journal accepted".into());
    }
    store = tampered.into_store();
    store.compare_exchange_record(key, Some(&corrupt), &valid)?;
    std::fs::remove_file(database)?;
    if PenelopeInferenceStream::new(store)
        .sync(database, &request)
        .is_ok()
    {
        return Err("completed journal recreated missing stream effect".into());
    }
    Ok(())
}

#[test]
fn file_stream_recovers_ordered_successors_before_and_after_sql_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.snapshot");
    exercise(
        || Ok(syntaxmesh_store::FileGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn turso_stream_recovers_ordered_successors_before_and_after_sql_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&graph)?;
    exercise(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}
