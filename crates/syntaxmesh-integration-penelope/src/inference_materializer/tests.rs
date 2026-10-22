use super::*;
use syntaxmesh_query::Query;

#[path = "../../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
pub(super) mod fixture;

pub(super) fn lifecycle_fixture<S: GraphStore>(
    store: &mut S,
) -> Result<
    (
        Rule,
        syntaxmesh_core::GenerationId,
        syntaxmesh_core::GenerationId,
    ),
    Box<dyn std::error::Error>,
> {
    fixture::fixture(store)
}

pub(super) fn indexed_fixture<S: GraphStore>(
    store: &mut S,
) -> Result<
    (
        Rule,
        syntaxmesh_core::GenerationId,
        syntaxmesh_core::GenerationId,
    ),
    Box<dyn std::error::Error>,
> {
    fixture::fixture(store)
}

pub(super) const fn lifecycle_limits() -> Limits {
    fixture::limits()
}

pub(super) fn lifecycle_restore<S: GraphStore>(
    store: &mut S,
    rule: &Rule,
    retained: syntaxmesh_core::GenerationId,
    current: syntaxmesh_core::GenerationId,
) -> Result<(), Box<dyn std::error::Error>> {
    fixture::verify_restoration(store, rule, retained, current)
}

#[test]
fn generic_journal_preserves_original_rebuild_record_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use penelope::{
        ActionId, ContentDigest, DefinitionId, DefinitionVersion, InputId, LinearSagaDefinition,
        LinearSagaEventEnvelope, LinearSagaInput, ProcessActionKind, ProcessId, RetryPolicy,
        StepId, StepPlan,
    };
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, retained, _current) = fixture::fixture(&mut store)?;
    let manifest = store.manifest(retained)?;
    let selection = ClaimSelection {
        repository: manifest.repository,
        worktree: manifest.worktree,
        generation: retained,
    };
    let rules = vec![rule];
    let request = Request {
        selection,
        operation: materialization_identity(selection, &rules, 1, 16_384)?,
        database: "/tmp/inference.sqlite".to_owned(),
        rules,
        limits: fixture::limits(),
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let suffix = hex::encode(
        ContentDigest::sha256(&bincode::serialize(&(
            request.operation,
            &request.database,
        ))?)
        .0,
    );
    let digest = ContentDigest::sha256(&bincode::serialize(&request)?);
    let definition = LinearSagaDefinition::new(
        DefinitionId::new("def_syntaxmesh_inference_materializer".to_owned())?,
        DefinitionVersion::new("dfv_v1".to_owned())?,
        digest,
        vec![StepPlan {
            step_id: StepId::new(format!("stp_inference_{suffix}"))?,
            action_kind: ProcessActionKind::ExternalEffect,
            payload_digest: digest,
            retry_policy: RetryPolicy::no_retry(),
            compensation: None,
        }],
    );
    let process = ProcessId::new(format!("prc_{suffix}"))?;
    let event = LinearSagaInput::Start {
        input_id: InputId::new(format!("inp_start_{}", process.as_str()))?,
        action_id: ActionId::new(format!("act_{suffix}"))?,
    }
    .to_event(&definition);
    #[derive(serde::Serialize)]
    struct LegacyRecord<'request> {
        version: u16,
        request: &'request Request,
        digest: ContentDigest,
        events: Vec<LinearSagaEventEnvelope>,
        receipt: Option<Watermark>,
    }
    let legacy = LegacyRecord {
        version: 1,
        request: &request,
        digest,
        events: vec![LinearSagaEventEnvelope { sequence: 0, event }],
        receipt: None,
    };
    let original = bincode::serialize(&legacy)?;
    let generic = journal::prepare(request.clone())?;
    if journal::encode(&generic)? != original {
        return Err("generic journal changed original rebuild record encoding".into());
    }
    journal::validate(&journal::decode::<Request>(&original)?, &request)?;
    Ok(())
}

fn exercise_recovery<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = open()?;
    let (rule, retained, current) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, retained, current)?;
    let limits = fixture::limits();
    let rules = [rule.clone()];
    let request = RebuildRequest {
        generation: retained,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let expected = Query::new(&store, retained).infer(&rule, &limits, 100)?;
    let mut coordinator = PenelopeInferenceMaterializer::new(store);
    if coordinator.rebuild(database, &request).is_ok() {
        return Err("effect opened an unmigrated missing database".into());
    }
    store = coordinator.into_store();
    let page = store.records_with_prefix_page(PREFIX, None, 32)?;
    let (key, bytes) = page
        .records
        .first()
        .ok_or("missing prepared inference record")?;
    let prepared = journal::decode::<Request>(bytes)?;
    if page.records.len() != 1 || prepared.receipt.is_some() {
        return Err("failed effect was not durably prepared".into());
    }
    let key = key.clone();
    let prepared_request = prepared.request;
    drop(store);
    InferenceMaterializer::migrate(database)?;
    let mut recovered = PenelopeInferenceMaterializer::new(open()?);
    if recovered.recover_pending()? != 1 || recovered.recover_pending()? != 0 {
        return Err("restart did not recover exactly one prepared inference effect".into());
    }
    let receipt = recovered.rebuild(database, &request)?;
    if InferenceMaterializer::open(database)?.claims(receipt, 10, 16_384)? != expected {
        return Err("recovered effect differs from canonical rebuild".into());
    }
    store = recovered.into_store();
    let completed = store
        .read_record(&key)?
        .ok_or("missing completed journal")?;
    // Simulate SQL commit followed by a crash before the completion journal write.
    let prepared_again = journal::encode(&journal::prepare(prepared_request)?)?;
    store.compare_exchange_record(&key, Some(&completed), &prepared_again)?;
    drop(store);
    let mut interrupted = PenelopeInferenceMaterializer::new(open()?);
    if interrupted.recover_pending()? != 1 || interrupted.rebuild(database, &request)? != receipt {
        return Err("SQL-before-journal interruption was not idempotent".into());
    }
    store = interrupted.into_store();
    if Query::new(&store, retained).infer(&rule, &limits, 100)? != expected
        || !Query::new(&store, current)
            .infer(&rule, &limits, 100)?
            .is_empty()
    {
        return Err("downstream recovery mutated canonical history".into());
    }
    let valid = store
        .read_record(&key)?
        .ok_or("missing recovered completion")?;
    let mut corrupt = valid.clone();
    corrupt.push(0);
    store.compare_exchange_record(&key, Some(&valid), &corrupt)?;
    let mut tampered = PenelopeInferenceMaterializer::new(store);
    if tampered.recover_pending().is_ok() {
        return Err("corrupt inference journal recovered".into());
    }
    store = tampered.into_store();
    store.compare_exchange_record(&key, Some(&corrupt), &valid)?;
    fixture::verify_restoration(&mut store, &rule, retained, current)?;
    // Only this fixture's generated downstream database is removed.
    std::fs::remove_file(database)?;
    let mut missing_effect = PenelopeInferenceMaterializer::new(store);
    if missing_effect.rebuild(database, &request).is_ok() || database.exists() {
        return Err(
            "completed journal falsely accepted or recreated missing downstream data".into(),
        );
    }
    Ok(())
}

#[test]
fn file_journal_recovers_inference_before_and_after_sql_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("graph.snapshot");
    exercise_recovery(
        || Ok(syntaxmesh_store::FileGraphStore::open(&path)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn turso_journal_recovers_inference_before_and_after_sql_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&path)?;
    exercise_recovery(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&path)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn request_key_excludes_controls_but_digest_and_replay_bind_them()
-> Result<(), Box<dyn std::error::Error>> {
    use syntaxmesh_core::{GenerationId, NodeId, RelationKind, RepositoryId, WorktreeId};
    let selection = ClaimSelection {
        repository: RepositoryId::derive(&[b"repo"]),
        worktree: WorktreeId::derive(&[b"worktree"]),
        generation: GenerationId::derive(&[b"generation"]),
    };
    let rules = vec![Rule {
        id: "example:rule".to_owned(),
        version: "1".to_owned(),
        ontology_versions: std::collections::BTreeMap::new(),
        configuration_fingerprint: StableId::derive("config", &[b"1"]),
        path_relation: RelationKind::Calls,
        join_relation: RelationKind::Calls,
        declaration_relation: RelationKind::Calls,
        concept: NodeId::derive(&[b"concept"]),
        output_relation: RelationKind::External {
            namespace: "example".to_owned(),
            relation: "derived".to_owned(),
        },
    }];
    let request = Request {
        selection,
        operation: materialization_identity(selection, &rules, 1, 16_384)?,
        database: "/tmp/inference.sqlite".to_owned(),
        rules,
        limits: Limits {
            input_bytes: 16_384,
            facts: 100,
            edge_visits: 100,
            depth: 2,
            claims: 10,
        },
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let record = journal::prepare(request.clone())?;
    journal::validate(&journal::decode(&journal::encode(&record)?)?, &request)?;
    let mut forged_completion = record.clone();
    forged_completion.receipt = Some(Watermark {
        operation: request.operation,
        selection,
        generation_sequence: 1,
        configuration: syntaxmesh_inference::rule_set_identity(&request.rules, 1, 16_384)?,
        claim_count: 0,
    });
    if journal::validate(&forged_completion, &request).is_ok() {
        return Err("receipt without Penelope completion accepted".into());
    }
    let mut changed = request.clone();
    changed.output_bytes = 32_768;
    if journal::key(&changed)? != journal::key(&request)?
        || journal::validate(&record, &changed).is_ok()
    {
        return Err("changed controls altered semantic operation or bypassed digest".into());
    }
    changed = request.clone();
    changed.database = "/tmp/other.sqlite".to_owned();
    if journal::key(&changed)? == journal::key(&request)? {
        return Err("destination not bound to journal identity".into());
    }
    let mut bytes = journal::encode(&record)?;
    bytes.push(0);
    if journal::decode::<Request>(&bytes).is_ok() {
        return Err("trailing journal bytes accepted".into());
    }
    Ok(())
}
