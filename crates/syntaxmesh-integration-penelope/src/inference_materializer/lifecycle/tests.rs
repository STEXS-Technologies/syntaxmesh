use super::*;

fn exercise<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::tests::{lifecycle_fixture, lifecycle_limits, lifecycle_restore};
    use syntaxmesh_inference_sqlite::RebuildRequest;
    let mut store = open()?;
    let (rule, retained, current) = lifecycle_fixture(&mut store)?;
    let limits = lifecycle_limits();
    let rules = [rule.clone()];
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
    let origin = InferenceMaterializer::open(database)?.rebuild(&store, &rebuild)?;
    let claim = InferenceMaterializer::open(database)?
        .claims(origin, 10, 16_384)?
        .into_iter()
        .next()
        .ok_or("missing claim")?;
    let bounds = InvalidationLimits {
        generations: 10,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let request = LifecycleRequest {
        origin,
        through: current,
        limits: &bounds,
        evidence_budget: 10,
        claim_bytes: 16_384,
    };
    // Remove only this fixture's generated effect database to force a prepared request.
    std::fs::remove_file(database)?;
    let mut failed = PenelopeInferenceLifecycle::new(store);
    if failed.invalidate(database, &request).is_ok() {
        return Err("missing lifecycle effect falsely succeeded".into());
    }
    store = failed.into_store();
    let page = store.records_with_prefix_page(PREFIX, None, 32)?;
    let (key, encoded) = page
        .records
        .first()
        .ok_or("missing prepared lifecycle journal")?;
    let prepared = journal::decode::<Request>(encoded)?;
    if prepared.receipt.is_some() {
        return Err("failed lifecycle effect marked completed".into());
    }
    let key = key.clone();
    let recorded = prepared.request;
    drop(store);
    InferenceMaterializer::migrate(database)?;
    InferenceMaterializer::open(database)?.rebuild(&open()?, &rebuild)?;
    let mut recovered = PenelopeInferenceLifecycle::new(open()?);
    if recovered.recover_pending()? != 1 || recovered.recover_pending()? != 0 {
        return Err("lifecycle restart did not recover one prepared effect".into());
    }
    let receipt = recovered.invalidate(database, &request)?;
    if receipt.affected_claims != 1 || receipt.closure_reasons != 1 {
        return Err("lifecycle recovered wrong effect receipt".into());
    }
    store = recovered.into_store();
    let completed = store.read_record(&key)?.ok_or("missing completion")?;
    let prepared_again = journal::encode(&journal::prepare(recorded)?)?;
    store.compare_exchange_record(&key, Some(&completed), &prepared_again)?;
    lifecycle_restore(&mut store, &rule, retained, current)?;
    let through = store
        .current_generation(origin.selection.repository, origin.selection.worktree)?
        .ok_or("missing restored current")?
        .generation;
    InferenceMaterializer::open(database)?
        .invalidate(&store, &LifecycleRequest { through, ..request })?;
    drop(store);
    let mut interrupted = PenelopeInferenceLifecycle::new(open()?);
    if interrupted.recover_pending()? != 1 || interrupted.invalidate(database, &request)? != receipt
    {
        return Err("old SQL effect lost after later coverage advancement".into());
    }
    let lifecycle = InferenceMaterializer::open(database)?.claim_lifecycle(origin, claim.id, 10)?;
    if lifecycle.checked_through.generation != through
        || lifecycle.valid_until.map(|until| until.generation) != Some(current)
    {
        return Err("historical recovery rewound coverage or revived invalid claim".into());
    }
    store = interrupted.into_store();
    let valid = store
        .read_record(&key)?
        .ok_or("missing recovered journal")?;
    let mut corrupt = valid.clone();
    corrupt.push(0);
    store.compare_exchange_record(&key, Some(&valid), &corrupt)?;
    let mut tampered = PenelopeInferenceLifecycle::new(store);
    if tampered.recover_pending().is_ok() {
        return Err("corrupt lifecycle journal accepted".into());
    }
    store = tampered.into_store();
    store.compare_exchange_record(&key, Some(&corrupt), &valid)?;
    std::fs::remove_file(database)?;
    if PenelopeInferenceLifecycle::new(store)
        .invalidate(database, &request)
        .is_ok()
    {
        return Err("completed journal accepted missing lifecycle receipt".into());
    }
    Ok(())
}

#[test]
fn file_lifecycle_recovers_after_later_coverage_without_rewinding()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.snapshot");
    exercise(
        || Ok(syntaxmesh_store::FileGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn turso_lifecycle_recovers_after_later_coverage_without_rewinding()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&graph)?;
    exercise(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn immutable_controls_share_operation_identity_but_not_request_validation()
-> Result<(), Box<dyn std::error::Error>> {
    use syntaxmesh_core::{RepositoryId, StableId, WorktreeId};
    use syntaxmesh_inference::ClaimSelection;
    let generation = GenerationId::derive(&[b"origin"]);
    let request = Request {
        origin: Watermark {
            operation: StableId::derive("operation", &[b"1"]),
            selection: ClaimSelection {
                repository: RepositoryId::derive(&[b"repo"]),
                worktree: WorktreeId::derive(&[b"worktree"]),
                generation,
            },
            generation_sequence: 1,
            configuration: StableId::derive("config", &[b"1"]),
            claim_count: 1,
        },
        through: GenerationId::derive(&[b"through"]),
        database: "/tmp/inference.sqlite".to_owned(),
        limits: InvalidationLimits {
            generations: 10,
            versions: 100,
            postings: 100,
            claims: 10,
        },
        evidence_budget: 10,
        claim_bytes: 16_384,
    };
    let prepared = journal::prepare(request.clone())?;
    let mut changed = request.clone();
    changed.claim_bytes = 32_768;
    if journal::key(&changed)? != journal::key(&request)?
        || journal::validate(&prepared, &changed).is_ok()
    {
        return Err("controls changed semantic operation or bypassed digest".into());
    }
    changed = request.clone();
    changed.through = generation;
    if journal::key(&changed)? == journal::key(&request)? {
        return Err("target generation not bound to lifecycle identity".into());
    }
    changed = request;
    changed.database = "/tmp/other.sqlite".to_owned();
    if journal::key(&changed)? == journal::key(&prepared.request)? {
        return Err("destination not bound to lifecycle identity".into());
    }
    Ok(())
}
