use super::*;
use syntaxmesh_inference::InvalidationLimits;
use syntaxmesh_inference_sqlite::StreamSyncRequest;

fn exercise<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
    completed_children: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::tests::{lifecycle_fixture, lifecycle_limits};
    let mut store = open()?;
    let (rule, retained, retracted) = lifecycle_fixture(&mut store)?;
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
    let manifest = store.manifest(retained)?;
    let selection = ClaimSelection {
        repository: manifest.repository,
        worktree: manifest.worktree,
        generation: retained,
    };
    let request = Request {
        rebuild: super::super::Request {
            selection,
            operation: materialization_identity(selection, &rules, 1, limits.input_bytes)?,
            database: database.to_str().ok_or("non-UTF8 fixture path")?.to_owned(),
            rules: rules.to_vec(),
            limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
        page_budget: 4096,
    };
    let key = journal::key(&request)?;
    let prepared = journal::encode(&journal::prepare(request)?)?;
    // Simulate interruption at the exact parent-before-children boundary.
    store.compare_exchange_record(&key, None, &prepared)?;
    InferenceMaterializer::migrate(database)?;
    if completed_children > 0 {
        let baseline =
            PenelopeInferenceMaterializer::new(&mut store).rebuild(database, &rebuild)?;
        if completed_children > 1 {
            PenelopeInferenceStream::new(&mut store).begin(
                database,
                &StreamBeginRequest {
                    baseline,
                    claim_budget: 10,
                    byte_budget: 16_384,
                    premise_budget: 100,
                    page_budget: 4096,
                },
            )?;
        }
    }
    drop(store);
    let mut recovered = PenelopeInferenceSetup::new(open()?);
    if recovered.recover_pending()? != 1 || recovered.recover_pending()? != 0 {
        return Err("parent setup did not recover exactly once".into());
    }
    let origin = recovered.initialize(database, &rebuild, 4096)?;
    if recovered.initialize(database, &rebuild, 8192).is_ok() {
        return Err("setup changed immutable page admission".into());
    }
    store = recovered.into_store();
    let bounds = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let sync = StreamSyncRequest {
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
    PenelopeInferenceStream::new(&mut store).sync(database, &sync)?;
    // Both effects exist and the head advanced, but parent completion was lost.
    let completed = store
        .read_record(&key)?
        .ok_or("missing parent completion")?;
    store.compare_exchange_record(&key, Some(&completed), &prepared)?;
    drop(store);
    let mut interrupted = PenelopeInferenceSetup::new(open()?);
    if interrupted.recover_pending()? != 1
        || interrupted.initialize(database, &rebuild, 4096)? != origin
    {
        return Err("setup recovery changed stream identity after head advancement".into());
    }
    if !InferenceMaterializer::open(database)?
        .stream_claims(origin, retracted, 10, 16_384, 100, 4096)?
        .claims
        .is_empty()
    {
        return Err("parent recovery reset advanced active root".into());
    }
    store = interrupted.into_store();
    let valid = store.read_record(&key)?.ok_or("missing recovered setup")?;
    let mut corrupt = valid.clone();
    corrupt.push(0);
    store.compare_exchange_record(&key, Some(&valid), &corrupt)?;
    let mut tampered = PenelopeInferenceSetup::new(store);
    if tampered.recover_pending().is_ok() {
        return Err("corrupt setup journal accepted".into());
    }
    store = tampered.into_store();
    store.compare_exchange_record(&key, Some(&corrupt), &valid)?;
    // Only this generated temporary effect database is removed.
    std::fs::remove_file(database)?;
    InferenceMaterializer::migrate(database)?;
    InferenceMaterializer::open(database)?.rebuild(&store, &rebuild)?;
    if PenelopeInferenceSetup::new(store)
        .initialize(database, &rebuild, 4096)
        .is_ok()
        || InferenceMaterializer::open(database)?
            .stream_checkpoint(origin, retained)
            .is_ok()
    {
        return Err("completed parent recreated missing stream effect".into());
    }
    Ok(())
}

#[test]
fn file_parent_intent_recovers_all_child_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    for phase in 0..3 {
        let directory = tempfile::tempdir()?;
        let graph = directory.path().join("graph.snapshot");
        exercise(
            || Ok(syntaxmesh_store::FileGraphStore::open(&graph)?),
            &directory.path().join("inference.sqlite"),
            phase,
        )?;
    }
    Ok(())
}

#[test]
fn turso_parent_intent_recovers_all_child_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    for phase in 0..3 {
        let directory = tempfile::tempdir()?;
        let graph = directory.path().join("graph.db");
        syntaxmesh_store_turso::TursoGraphStore::migrate(&graph)?;
        exercise(
            || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&graph)?),
            &directory.path().join("inference.sqlite"),
            phase,
        )?;
    }
    Ok(())
}
