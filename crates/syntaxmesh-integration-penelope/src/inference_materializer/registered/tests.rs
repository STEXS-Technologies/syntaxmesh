use super::*;
use crate::inference_materializer::{
    PenelopeInferenceMaterializer, PenelopeInferenceSetup, PenelopeInferenceStream, journal, tests,
};
use syntaxmesh_core::{EdgeId, GraphDelta, IndexRunId, NodeId};
use syntaxmesh_inference::InvalidationLimits;
use syntaxmesh_inference::{ONTOLOGY_DEFINITION_SCHEMA_VERSION, OntologyRelationDefinition};
use syntaxmesh_inference_sqlite::{
    InferenceMaterializer, RebuildRequest, StreamOrigin, StreamSyncRequest,
};
use syntaxmesh_query::Query;
use syntaxmesh_store::{DurableRecordStore, GraphStore, InMemoryGraphStore};

const REGISTERED_PREFIX: &str = "syntaxmesh.penelope.inference-registered.v1/";

fn definition(rule: &Rule) -> OntologyDefinition {
    let source_concept = NodeId::derive(&[b"source-concept"]);
    let relation = |name: &str| OntologyRelationDefinition {
        name: name.to_owned(),
        subject_domain: Vec::new(),
        object_range: Vec::new(),
    };
    let mut writes = relation("writes");
    writes.subject_domain.push(source_concept);
    writes.object_range.push(rule.concept);
    let mut instance = relation("instance_of");
    instance.object_range.extend([rule.concept, source_concept]);
    let mut output = relation("participates");
    output.object_range.push(rule.concept);
    OntologyDefinition {
        schema_version: ONTOLOGY_DEFINITION_SCHEMA_VERSION,
        namespace: "example".to_owned(),
        version: "1".to_owned(),
        concepts: vec![rule.concept, source_concept],
        instance_relation: "instance_of".to_owned(),
        relations: vec![writes, instance, output],
    }
}

#[test]
fn registered_rebuild_recovers_from_persisted_definitions_before_and_after_sql_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("registered-inference.sqlite");
    let mut store = InMemoryGraphStore::new();
    let (mut rule, generation, current) = tests::lifecycle_fixture(&mut store)?;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    let rules = [rule.clone()];
    let limits = tests::lifecycle_limits();
    let request = RebuildRequest {
        generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let definitions = [definition(&rule)];
    let admission = RegisteredInferenceAdmission {
        registry_entries: 16,
        registry_bytes: 16_384,
        bindings: 1,
    };
    let mut coordinator = PenelopeInferenceMaterializer::new(store);
    if coordinator
        .rebuild_registered(&database, &request, &definitions, admission)
        .is_ok()
    {
        return Err("registered rebuild unexpectedly opened an unmigrated database".into());
    }
    store = coordinator.into_store();
    let page = store.records_with_prefix_page(REGISTERED_PREFIX, None, 32)?;
    let (key, bytes) = page.records.first().ok_or("missing registered request")?;
    let prepared = journal::decode::<Request>(bytes)?;
    if page.records.len() != 1
        || prepared.receipt.is_some()
        || prepared.request.definitions != canonical_definitions(&definitions)
        || prepared.request.binding_budget != admission.bindings
    {
        return Err("registered request did not durably retain its ontology".into());
    }
    let key = key.clone();
    let prepared_request = prepared.request;
    InferenceMaterializer::migrate(&database)?;
    let mut recovered = PenelopeInferenceMaterializer::new(store);
    if recovered.recover_registered_pending()? != 1 || recovered.recover_registered_pending()? != 0
    {
        return Err("registered rebuild did not recover exactly once".into());
    }
    let mut recovered_store = recovered.into_store();
    let completed = recovered_store
        .read_record(&key)?
        .ok_or("missing completed registered journal")?;
    let record = journal::decode::<Request>(&completed)?;
    let receipt = record.receipt.ok_or("registered receipt missing")?;
    let registry = OntologyRegistry::from_definitions(&definitions, 16, 16_384)?;
    let bound = registry.bind_rules(&rules, 1, 1, limits.input_bytes)?;
    let expected = Query::new(&recovered_store, generation).infer_bound(
        &registry,
        &bound,
        &limits,
        request.history_scan_budget,
        request.rule_budget,
        admission.bindings,
    )?;
    if expected.is_empty()
        || InferenceMaterializer::open(&database)?.claims(receipt, 10, 16_384)? != expected
    {
        return Err("recovered registered claims differ from validated Query output".into());
    }

    // Simulate SQL publication followed by a crash before the journal completion write.
    let pending = journal::encode(&journal::prepare(prepared_request)?)?;
    recovered_store.compare_exchange_record(&key, Some(&completed), &pending)?;
    let mut interrupted = PenelopeInferenceMaterializer::new(recovered_store);
    if interrupted.recover_registered_pending()? != 1 {
        return Err("registered SQL-before-journal recovery was not idempotent".into());
    }
    let mut interrupted_store = interrupted.into_store();
    let retained = interrupted_store
        .read_record(&key)?
        .ok_or("missing replayed journal")?;
    if journal::decode::<Request>(&retained)?.receipt != Some(receipt) {
        return Err("registered retry changed the materialization receipt".into());
    }
    tests::lifecycle_restore(&mut interrupted_store, &rule, generation, current)?;
    Ok(())
}

#[test]
fn registered_stream_journals_definitions_and_recovers_after_sql_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("registered-stream.sqlite");
    InferenceMaterializer::migrate(&database)?;
    let mut source = InMemoryGraphStore::new();
    let (mut rule, source_declared, _source_retracted) = tests::indexed_fixture(&mut source)?;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    let source_baseline = source
        .manifest(source_declared)?
        .parent
        .ok_or("missing source stream baseline")?;
    let source_manifest = source.manifest(source_baseline)?;
    let source_snapshot = source.historical_snapshot(source_baseline)?;
    let baseline_generation =
        syntaxmesh_core::GenerationId::derive(&[b"penelope-registered-membership-baseline"]);
    let declared =
        syntaxmesh_core::GenerationId::derive(&[b"penelope-registered-membership-declared"]);
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository: source_manifest.repository,
        worktree: source_manifest.worktree,
        run_id: IndexRunId::derive(&[b"penelope-registered-membership-baseline-run"]),
        expected_base: None,
        next_generation: baseline_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: source_snapshot.provenance,
        upsert_nodes: source_snapshot.nodes,
        upsert_edges: source_snapshot.edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let source_transition = source
        .changes_between_page(source_baseline, source_declared, None, 1)?
        .items
        .into_iter()
        .next()
        .ok_or("missing registered declaration transition")?;
    let mut declared_delta = source_transition.delta;
    declared_delta.expected_base = Some(baseline_generation);
    declared_delta.next_generation = declared;
    declared_delta.run_id = IndexRunId::derive(&[b"penelope-registered-membership-declared-run"]);
    store.apply_delta(declared_delta)?;
    let definitions = [definition(&rule)];
    let admission = RegisteredInferenceAdmission {
        registry_entries: 16,
        registry_bytes: 16_384,
        bindings: 1,
    };
    let rules = [rule];
    let limits = tests::lifecycle_limits();
    let baseline_request = RebuildRequest {
        generation: baseline_generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let baseline = InferenceMaterializer::open(&database)?.rebuild_registered(
        &store,
        &baseline_request,
        &OntologyRegistry::from_definitions(&definitions, admission.registry_entries, 16_384)?,
        admission.bindings,
    )?;
    let origin = StreamOrigin {
        stream: syntaxmesh_core::StableId::derive("inference-stream-v1", &[&baseline.operation.0]),
        baseline,
    };
    InferenceMaterializer::open(&database)?.begin_stream(baseline, 10, 16_384, 100, 4096)?;
    let limits_range = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let request = StreamSyncRequest {
        origin,
        through: declared,
        rules: &rules,
        limits: &limits,
        range_limits: &limits_range,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
        evidence_budget: 100,
        matched_claim_bytes: 16_384,
        page_budget: 4096,
    };
    let mut coordinator = PenelopeInferenceStream::new(store);
    let receipt = coordinator
        .sync_registered(&database, &request, &definitions, admission)
        .map_err(|error| format!("initial registered successor failed: {error}"))?;
    if receipt.added_claims != 1 || receipt.active_claims != 1 {
        return Err("Penelope did not publish the registered discovered claim".into());
    }
    store = coordinator.into_store();
    let page = store.records_with_prefix_page(STREAM_PREFIX, None, 32)?;
    let (key, bytes) = page
        .records
        .first()
        .ok_or("missing registered stream journal")?;
    let completed = journal::decode::<RegisteredStreamRequest>(bytes)?;
    if page.records.len() != 1
        || completed.request.definitions != canonical_definitions(&definitions)
        || completed.receipt != Some(receipt)
    {
        return Err("registered stream did not persist its full request and receipt".into());
    }
    let key = key.clone();
    let prepared = journal::encode(&journal::prepare(completed.request)?)?;
    store.compare_exchange_record(&key, Some(bytes), &prepared)?;
    let mut recovered = PenelopeInferenceStream::new(store);
    if recovered
        .recover_registered_stream_pending()
        .map_err(|error| format!("first registered recovery failed: {error}"))?
        != 1
        || recovered
            .recover_registered_stream_pending()
            .map_err(|error| format!("second registered recovery failed: {error}"))?
            != 0
    {
        return Err("registered stream SQL-before-journal recovery was not idempotent".into());
    }
    let mut recovered_store = recovered.into_store();
    let final_record = recovered_store
        .read_record(&key)?
        .ok_or("missing recovered stream record")?;
    if journal::decode::<RegisteredStreamRequest>(&final_record)?.receipt != Some(receipt) {
        return Err("registered stream recovery changed its retained receipt".into());
    }

    let snapshot = recovered_store.historical_snapshot(declared)?;
    let membership = snapshot
        .edges
        .iter()
        .find(|edge| edge.id == EdgeId::derive(&[b"writer-instance"]))
        .ok_or("missing writer membership")?;
    let write = snapshot
        .edges
        .iter()
        .find(|edge| matches!(&edge.relation, syntaxmesh_core::RelationKind::External { relation, .. } if relation == "writes"))
        .ok_or("missing write fact")?;
    let manifest = recovered_store.manifest(declared)?;
    let membership_retracted =
        syntaxmesh_core::GenerationId::derive(&[b"penelope-registered-membership-retracted"]);
    recovered_store.apply_delta(GraphDelta {
        repository: manifest.repository,
        worktree: manifest.worktree,
        run_id: IndexRunId::derive(&[b"penelope-membership-retraction-run"]),
        expected_base: Some(declared),
        next_generation: membership_retracted,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: vec![membership.id, write.id],
    })?;
    let retraction_request = StreamSyncRequest {
        origin,
        through: membership_retracted,
        rules: &rules,
        limits: &limits,
        range_limits: &limits_range,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
        evidence_budget: 100,
        matched_claim_bytes: 16_384,
        page_budget: 4096,
    };
    let mut reconciliation_coordinator = PenelopeInferenceStream::new(recovered_store);
    let retraction = reconciliation_coordinator
        .sync_registered(&database, &retraction_request, &definitions, admission)
        .map_err(|error| format!("registered membership sync failed: {error}"))?;
    if retraction.affected_claims != 1 || retraction.active_claims != 0 {
        return Err("Penelope did not reconcile the registered membership retraction".into());
    }
    let mut reconciliation_store = reconciliation_coordinator.into_store();
    let reconciliation_page =
        reconciliation_store.records_with_prefix_page(STREAM_PREFIX, None, 32)?;
    let (reconciliation_key, reconciliation_bytes) = reconciliation_page
        .records
        .iter()
        .find(|(_, record_bytes)| {
            journal::decode::<RegisteredStreamRequest>(record_bytes)
                .is_ok_and(|record| record.request.through == membership_retracted)
        })
        .ok_or("missing membership-reconciliation journal")?;
    let reconciliation_key = reconciliation_key.clone();
    let completed_reconciliation = reconciliation_bytes.clone();
    let completed_request =
        journal::decode::<RegisteredStreamRequest>(&completed_reconciliation)?.request;
    let prepared_reconciliation = journal::encode(&journal::prepare(completed_request)?)?;
    reconciliation_store.compare_exchange_record(
        &reconciliation_key,
        Some(&completed_reconciliation),
        &prepared_reconciliation,
    )?;
    let mut recovered_reconciliation = PenelopeInferenceStream::new(reconciliation_store);
    if recovered_reconciliation
        .recover_registered_stream_pending()
        .map_err(|error| format!("membership registered recovery failed: {error}"))?
        != 1
        || recovered_reconciliation
            .recover_registered_stream_pending()
            .map_err(|error| format!("membership repeat recovery failed: {error}"))?
            != 0
    {
        return Err("membership reconciliation did not recover idempotently".into());
    }
    let restored_store = recovered_reconciliation.into_store();
    let retained = restored_store
        .read_record(&reconciliation_key)?
        .ok_or("missing recovered membership journal")?;
    if journal::decode::<RegisteredStreamRequest>(&retained)?.receipt != Some(retraction) {
        return Err("membership recovery changed its stream receipt".into());
    }
    Ok(())
}

#[test]
fn registered_setup_recovers_and_supports_registered_successors()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("registered-setup.sqlite");
    let mut store = InMemoryGraphStore::new();
    let (mut rule, declared, _retracted) = tests::lifecycle_fixture(&mut store)?;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    let rules = [rule.clone()];
    let limits = tests::lifecycle_limits();
    let baseline_generation = store
        .manifest(declared)?
        .parent
        .ok_or("missing registered setup baseline")?;
    let rebuild = RebuildRequest {
        generation: baseline_generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let definitions = [definition(&rule)];
    let admission = RegisteredInferenceAdmission {
        registry_entries: 16,
        registry_bytes: 16_384,
        bindings: 1,
    };
    let mut setup = PenelopeInferenceSetup::new(store);
    if setup
        .initialize_registered(&database, &rebuild, &definitions, admission, 4096)
        .is_ok()
    {
        return Err("registered setup unexpectedly opened an unmigrated database".into());
    }
    store = setup.into_store();
    if store
        .records_with_prefix_page(SETUP_PREFIX, None, 32)?
        .records
        .len()
        != 1
    {
        return Err("registered parent setup intent was not persisted first".into());
    }
    InferenceMaterializer::migrate(&database)?;
    setup = PenelopeInferenceSetup::new(store);
    let origin = setup.initialize_registered(&database, &rebuild, &definitions, admission, 4096)?;
    if setup.recover_pending()? != 0 {
        return Err("completed registered setup was recovered twice".into());
    }
    store = setup.into_store();
    let range_limits = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let successor = StreamSyncRequest {
        origin,
        through: declared,
        rules: &rules,
        limits: &limits,
        range_limits: &range_limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
        evidence_budget: 100,
        matched_claim_bytes: 16_384,
        page_budget: 4096,
    };
    let receipt = PenelopeInferenceStream::new(store).sync_registered(
        &database,
        &successor,
        &definitions,
        admission,
    )?;
    if receipt.added_claims != 1 || receipt.active_claims != 1 {
        return Err("registered setup did not support a registered successor".into());
    }
    Ok(())
}

fn exercise_registered_setup_restart<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = open()?;
    let (mut rule, declared, _retracted) = tests::lifecycle_fixture(&mut store)?;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    let rules = [rule.clone()];
    let limits = tests::lifecycle_limits();
    let baseline_generation = store
        .manifest(declared)?
        .parent
        .ok_or("missing registered restart baseline")?;
    let rebuild = RebuildRequest {
        generation: baseline_generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let definitions = [definition(&rule)];
    let admission = RegisteredInferenceAdmission {
        registry_entries: 16,
        registry_bytes: 16_384,
        bindings: 1,
    };

    // Parent intent is durable even though the downstream database does not yet exist.
    let mut setup = PenelopeInferenceSetup::new(store);
    if setup
        .initialize_registered(database, &rebuild, &definitions, admission, 4096)
        .is_ok()
    {
        return Err("registered setup unexpectedly succeeded without an effect database".into());
    }
    store = setup.into_store();
    drop(store);
    InferenceMaterializer::migrate(database)?;

    setup = PenelopeInferenceSetup::new(open()?);
    if setup.recover_pending()? != 1 {
        return Err("reopened store did not recover registered setup intent".into());
    }
    let origin = setup.initialize_registered(database, &rebuild, &definitions, admission, 4096)?;
    if setup.recover_pending()? != 0 {
        return Err("recovered registered setup did not become idempotent".into());
    }
    store = setup.into_store();

    let range_limits = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let receipt = PenelopeInferenceStream::new(store).sync_registered(
        database,
        &StreamSyncRequest {
            origin,
            through: declared,
            rules: &rules,
            limits: &limits,
            range_limits: &range_limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
            evidence_budget: 100,
            matched_claim_bytes: 16_384,
            page_budget: 4096,
        },
        &definitions,
        admission,
    )?;
    if receipt.added_claims != 1 || receipt.active_claims != 1 {
        return Err("recovered registered setup rejected its immediate successor".into());
    }
    Ok(())
}

#[test]
fn file_registered_setup_recovers_across_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("registered-setup.snapshot");
    let database = directory.path().join("registered-setup.sqlite");
    exercise_registered_setup_restart(
        || Ok(syntaxmesh_store::FileGraphStore::open(&graph)?),
        &database,
    )
}

#[test]
fn turso_registered_setup_recovers_across_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("registered-setup.db");
    let database = directory.path().join("registered-setup.sqlite");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&graph)?;
    exercise_registered_setup_restart(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&graph)?),
        &database,
    )
}
