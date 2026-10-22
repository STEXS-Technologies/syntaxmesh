use super::*;
use syntaxmesh_core::{
    FileId, FileVersion, GenerationId, GenerationStatus, IndexRunId, NodeId, RelationKind,
    RepositoryId, WorktreeId,
};
use syntaxmesh_inference::{
    Contract, ContractLimits, ContractOrigin, InvalidationLimits, Limits,
    ONTOLOGY_DEFINITION_SCHEMA_VERSION, OntologyDefinition, OntologyRelationDefinition,
    RelationExpectation, Rule,
};
use syntaxmesh_inference_sqlite::{
    ContractHistoryWatermark, InferenceMaterializer, StreamSyncRequest,
};
use syntaxmesh_integration_penelope::ContractHistoryRequest;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::SourceFile;
use syntaxmesh_query::Query;

#[path = "../../../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
mod fixture;

#[test]
fn contract_history_is_invariant_under_opt_in_statechronicle_verification()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let durable_database = directory.path().join("contract-durable.sqlite");
    let verified_database = directory.path().join("contract-verified.sqlite");
    InferenceMaterializer::migrate(&durable_database)?;
    InferenceMaterializer::migrate(&verified_database)?;
    let repository = RepositoryId::derive(&[b"contract-verification-toggle-repo"]);
    let worktree = WorktreeId::derive(&[b"contract-verification-toggle-worktree"]);
    let generation = GenerationId::derive(&[b"contract-verification-toggle-generation"]);
    let run_id = IndexRunId::derive(&[b"contract-verification-toggle-run"]);
    let content = "fn writer() {}\nfn entry() { writer(); }\n";
    let source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"src/lib.rs"]),
            normalized_path: "src/lib.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content: content.to_owned(),
    };
    let mut durable_engine = SyntaxMeshEngine::new(
        syntaxmesh_store::InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    durable_engine.index(std::slice::from_ref(&source), run_id, generation)?;
    let durable_snapshot = durable_engine
        .indexer
        .store()
        .historical_snapshot(generation)?;
    let call = durable_snapshot
        .edges
        .iter()
        .find(|edge| edge.relation == RelationKind::Calls)
        .ok_or("missing indexed call evidence")?;
    let contract = Contract {
        id: "entry-calls-writer".to_owned(),
        version: "1".to_owned(),
        origin: ContractOrigin::Declared,
        source: call.source,
        target: call.target,
        relation: RelationKind::Calls,
        expectation: RelationExpectation::Required,
    };
    let mut verified_engine = SyntaxMeshEngine::new(
        syntaxmesh_store::InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    )
    .with_statechronicle_verification();
    verified_engine.index(std::slice::from_ref(&source), run_id, generation)?;
    let verified_snapshot = verified_engine
        .indexer
        .store()
        .historical_snapshot(generation)?;
    if durable_snapshot != verified_snapshot {
        return Err("verification toggle changed indexed graph facts".into());
    }
    let contracts = [contract];
    let limits = ContractLimits {
        contracts: 1,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 1,
    };
    let durable_request = ContractHistoryRequest {
        generation,
        contracts: &contracts,
        limits: &limits,
        history_scan_budget: 100,
        output_bytes: 16_384,
    };
    let verified_request = ContractHistoryRequest {
        generation,
        ..durable_request
    };
    let durable_receipt =
        durable_engine.retain_contract_history(&durable_database, &durable_request)?;
    let verified_receipt =
        verified_engine.retain_contract_history(&verified_database, &verified_request)?;
    let durable_result = durable_engine.contract_result_at(
        &durable_database,
        repository,
        worktree,
        generation,
        "entry-calls-writer",
        "1",
    )?;
    let verified_result = verified_engine.contract_result_at(
        &verified_database,
        repository,
        worktree,
        generation,
        "entry-calls-writer",
        "1",
    )?;
    let durable_status = durable_engine
        .into_store()
        .current_generation(repository, worktree)?
        .ok_or("missing durable generation")?
        .status;
    let verified_status = verified_engine
        .into_store()
        .current_generation(repository, worktree)?
        .ok_or("missing verified generation")?
        .status;
    if durable_receipt.operation != verified_receipt.operation
        || durable_result != verified_result
        || durable_status != GenerationStatus::Durable
        || verified_status != GenerationStatus::Verified
    {
        return Err(format!(
            "StateChronicle toggle changed retained contract semantics: ops_equal={}, results_equal={}, durable_status={durable_status:?}, verified_status={verified_status:?}",
            durable_receipt.operation == verified_receipt.operation,
            durable_result == verified_result,
        ).into());
    }
    Ok(())
}

#[test]
fn engine_retains_contract_history_through_penelope_and_recovers_it()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("engine-contract-history.sqlite");
    InferenceMaterializer::migrate(&database)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (_rule, generation, _later) = fixture::fixture(&mut store)?;
    let manifest = store.manifest(generation)?;
    let contract = Contract {
        id: "entry-calls-writer".to_owned(),
        version: "1".to_owned(),
        origin: ContractOrigin::Declared,
        source: NodeId::derive(&[b"entry"]),
        target: NodeId::derive(&[b"writer"]),
        relation: RelationKind::Calls,
        expectation: RelationExpectation::Required,
    };
    let contracts = [contract];
    let limits = ContractLimits {
        contracts: 1,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 1,
    };
    let request = ContractHistoryRequest {
        generation,
        contracts: &contracts,
        limits: &limits,
        history_scan_budget: 100,
        output_bytes: 16_384,
    };
    let expected = Query::new(&store, generation).evaluate_contracts(&contracts, &limits, 100)?;
    let mut engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    let receipt: ContractHistoryWatermark = engine.retain_contract_history(&database, &request)?;
    let result = engine
        .contract_result_at(
            &database,
            receipt.repository,
            receipt.worktree,
            generation,
            "entry-calls-writer",
            "1",
        )?
        .ok_or("engine did not retain its contract result")?;
    let history = engine.contract_history(
        &database,
        &syntaxmesh_inference_sqlite::ContractHistoryQuery {
            repository: receipt.repository,
            worktree: receipt.worktree,
            contract_id: "entry-calls-writer".to_owned(),
            contract_version: "1".to_owned(),
            after: None,
            limit: 1,
        },
    )?;
    let as_of = engine.contract_result_as_of(&database, generation, "entry-calls-writer", "1")?;
    let unknown_generation = GenerationId::derive(&[b"unknown-as-of-generation"]);
    if engine
        .contract_result_as_of(&database, unknown_generation, "entry-calls-writer", "1")
        .is_ok()
    {
        return Err("as-of lookup accepted an unknown canonical generation".into());
    }
    if result.evaluation.generation != generation
        || expected.first().map(|evaluation| evaluation.id) != Some(result.evaluation.id)
        || history.items.first() != Some(&result)
        || history.next_cursor.is_some()
        || as_of.target_generation != generation
        || as_of.target_sequence != receipt.generation_sequence
        || as_of.evaluation.as_ref() != Some(&result)
    {
        return Err(
            "engine materialization changed generation-pinned evaluation or as-of selection".into(),
        );
    }
    if engine.recover_inference()?.contract_history != 0 {
        return Err("completed contract history was counted as pending recovery".into());
    }
    Ok(())
}

#[test]
fn engine_advances_retained_contracts_over_successors_and_recovers_partial_catch_up()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("engine-contract-catch-up.sqlite");
    InferenceMaterializer::migrate(&database)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (_rule, declared, retracted) = fixture::fixture(&mut store)?;
    let first = store
        .manifest(declared)?
        .parent
        .ok_or("fixture has no contract baseline generation")?;
    let manifest = store.manifest(declared)?;
    let contract = Contract {
        id: "target-not-in-concept".to_owned(),
        version: "1".to_owned(),
        origin: ContractOrigin::Declared,
        source: NodeId::derive(&[b"target"]),
        target: NodeId::derive(&[b"concept"]),
        relation: RelationKind::External {
            namespace: "example".to_owned(),
            relation: "instance_of".to_owned(),
        },
        expectation: RelationExpectation::Forbidden,
    };
    let contracts = [contract];
    let limits = ContractLimits {
        contracts: 1,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 1,
    };
    let mut engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    let baseline = engine.retain_contract_history(
        &database,
        &ContractHistoryRequest {
            generation: first,
            contracts: &contracts,
            limits: &limits,
            history_scan_budget: 100,
            output_bytes: 16_384,
        },
    )?;
    let request = ContractHistoryAdvanceRequest {
        through_generation: retracted,
        generation_budget: 2,
        history_scan_budget: 100,
        output_bytes_per_generation: 16_384,
        limits,
    };
    if engine
        .advance_contract_history(
            &database,
            ContractHistoryAdvanceRequest {
                generation_budget: 1,
                ..request
            },
        )
        .is_ok()
    {
        return Err("insufficient generation budget published a partial catch-up".into());
    }
    if InferenceMaterializer::open(&database)?
        .latest_contract_set(manifest.repository, manifest.worktree)?
        .is_none_or(|set| set.watermark != baseline)
    {
        return Err("generation-budget rejection moved the retained contract head".into());
    }

    let retracted_hex = retracted.0.to_hex();
    rusqlite::Connection::open(&database)?.execute_batch(&format!(
        "CREATE TRIGGER interrupt_contract_catch_up BEFORE INSERT ON inference_contract_evaluations
         WHEN NEW.generation = '{retracted_hex}'
         BEGIN SELECT RAISE(ABORT, 'injected catch-up failure'); END;"
    ))?;
    if engine.advance_contract_history(&database, request).is_ok() {
        return Err("injected second-generation contract failure was ignored".into());
    }
    let partial_head = InferenceMaterializer::open(&database)?
        .latest_contract_set(manifest.repository, manifest.worktree)?
        .ok_or("catch-up lost its complete prefix")?;
    if partial_head.watermark.generation != declared {
        return Err("failed catch-up did not preserve exactly its completed prefix".into());
    }
    rusqlite::Connection::open(&database)?
        .execute_batch("DROP TRIGGER interrupt_contract_catch_up")?;

    let resumed = engine.advance_contract_history(&database, request)?;
    let materializer = InferenceMaterializer::open(&database)?;
    let first_result = materializer
        .contract_result_at(
            manifest.repository,
            manifest.worktree,
            first,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("baseline contract result disappeared")?;
    let declared_result = materializer
        .contract_result_at(
            manifest.repository,
            manifest.worktree,
            declared,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("first successor contract result was not retained")?;
    let retracted_result = materializer
        .contract_result_at(
            manifest.repository,
            manifest.worktree,
            retracted,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("recovered successor contract result was not retained")?;
    if resumed.generations_advanced != 0
        || resumed.last_watermark.generation != retracted
        || first_result.evaluation.outcome
            != syntaxmesh_inference::ContractOutcome::InsufficientEvidence
        || declared_result.evaluation.outcome != syntaxmesh_inference::ContractOutcome::Contradicted
        || retracted_result.evaluation.outcome
            != syntaxmesh_inference::ContractOutcome::InsufficientEvidence
    {
        return Err("contract catch-up failed temporal outcomes or idempotent recovery".into());
    }
    let repeated = engine.advance_contract_history(&database, request)?;
    if repeated.generations_advanced != 0 || repeated.last_watermark != resumed.last_watermark {
        return Err("repeated contract catch-up was not an idempotent no-op".into());
    }
    Ok(())
}

fn exercise_contract_catch_up<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = open()?;
    let (_rule, declared, retracted) = fixture::fixture(&mut store)?;
    let first = store
        .manifest(declared)?
        .parent
        .ok_or("fixture has no contract baseline generation")?;
    let manifest = store.manifest(declared)?;
    let contract = Contract {
        id: "target-not-in-concept".to_owned(),
        version: "1".to_owned(),
        origin: ContractOrigin::Declared,
        source: NodeId::derive(&[b"target"]),
        target: NodeId::derive(&[b"concept"]),
        relation: RelationKind::External {
            namespace: "example".to_owned(),
            relation: "instance_of".to_owned(),
        },
        expectation: RelationExpectation::Forbidden,
    };
    let contracts = [contract];
    let limits = ContractLimits {
        contracts: 1,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 1,
    };
    let request = ContractHistoryAdvanceRequest {
        through_generation: retracted,
        generation_budget: 2,
        history_scan_budget: 100,
        output_bytes_per_generation: 16_384,
        limits,
    };
    let mut engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    engine.retain_contract_history(
        database,
        &ContractHistoryRequest {
            generation: first,
            contracts: &contracts,
            limits: &limits,
            history_scan_budget: 100,
            output_bytes: 16_384,
        },
    )?;
    drop(engine);

    let mut reopened = SyntaxMeshEngine::new(
        open()?,
        RustExtractor,
        manifest.repository,
        manifest.worktree,
    );
    let receipt = reopened.advance_contract_history(database, request)?;
    if receipt.generations_advanced != 2
        || receipt.starting_generation != first
        || receipt.through_generation != retracted
        || reopened.recover_inference()?.contract_history != 0
    {
        return Err("durable catch-up did not advance exactly through two generations".into());
    }
    drop(reopened);

    let mut idempotent_reopen = SyntaxMeshEngine::new(
        open()?,
        RustExtractor,
        manifest.repository,
        manifest.worktree,
    );
    let repeated = idempotent_reopen.advance_contract_history(database, request)?;
    if repeated.generations_advanced != 0 || repeated.last_watermark != receipt.last_watermark {
        return Err("durable repeated catch-up was not an idempotent no-op".into());
    }
    let materializer = InferenceMaterializer::open(database)?;
    let historical =
        materializer.contract_history(&syntaxmesh_inference_sqlite::ContractHistoryQuery {
            repository: manifest.repository,
            worktree: manifest.worktree,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
            after: None,
            limit: 10,
        })?;
    if historical.items.len() != 3
        || historical
            .items
            .iter()
            .map(|item| item.evaluation.generation)
            .collect::<Vec<_>>()
            != [first, declared, retracted]
    {
        return Err("durable catch-up lost chronological contract results".into());
    }
    Ok(())
}

#[test]
fn file_and_turso_contract_catch_up_survives_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file_graph = directory.path().join("contract-file.snapshot");
    let file_inference = directory.path().join("contract-file.sqlite");
    InferenceMaterializer::migrate(&file_inference)?;
    exercise_contract_catch_up(
        || Ok(syntaxmesh_store::FileGraphStore::open(&file_graph)?),
        &file_inference,
    )?;

    let turso_graph = directory.path().join("contract-turso.db");
    let turso_inference = directory.path().join("contract-turso.sqlite");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&turso_graph)?;
    InferenceMaterializer::migrate(&turso_inference)?;
    exercise_contract_catch_up(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&turso_graph)?),
        &turso_inference,
    )
}

fn ontology_definition(rule: &Rule) -> OntologyDefinition {
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
fn engine_initializes_registered_baseline_before_registered_successor()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("engine-registered.sqlite");
    InferenceMaterializer::migrate(&database)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (mut rule, declared, _retracted) = fixture::fixture(&mut store)?;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    let definitions = [ontology_definition(&rule)];
    let admission = RegisteredInferenceAdmission {
        registry_entries: 16,
        registry_bytes: 16_384,
        bindings: 1,
    };
    let baseline_generation = store
        .manifest(declared)?
        .parent
        .ok_or("missing registered baseline generation")?;
    let manifest = store.manifest(declared)?;
    let rules = [rule];
    let limits = fixture::limits();
    let rebuild = RebuildRequest {
        generation: baseline_generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let mut engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    let origin = engine.initialize_registered_inference_stream(
        &database,
        &rebuild,
        &definitions,
        admission,
        4096,
    )?;
    let range_limits = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let receipt = engine.synchronize_registered_inference_stream(
        &database,
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
        return Err("engine registered setup did not produce a valid successor".into());
    }
    Ok(())
}

fn exercise<S: GraphStore + DurableRecordStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = open()?;
    let (rule, declared, retracted) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, declared, retracted)?;
    fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
    let scope = store.manifest(declared)?;
    let limits = fixture::limits();
    let rules = [rule];
    let rebuild = RebuildRequest {
        generation: declared,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    InferenceMaterializer::migrate(database)?;
    let mut foreign = SyntaxMeshEngine::new(
        store,
        RustExtractor,
        RepositoryId::derive(&[b"foreign-engine"]),
        scope.worktree,
    );
    if foreign
        .initialize_inference_stream(database, &rebuild, 4096)
        .is_ok()
    {
        return Err("foreign engine initialized an inference effect".into());
    }
    let mut returned_store = foreign.into_store();
    if !returned_store
        .records_with_prefix_page("syntaxmesh.penelope.inference", None, 32)?
        .records
        .is_empty()
    {
        return Err("scope rejection prepared an inference journal".into());
    }
    let expected_sequence = returned_store.generation_sequence(declared)?;
    let borrowed = &mut returned_store;
    if GraphStore::generation_sequence(&borrowed, declared)? != expected_sequence {
        return Err("borrowed canonical port changed generation coordinates".into());
    }
    let mut engine = SyntaxMeshEngine::new(
        returned_store,
        RustExtractor,
        scope.repository,
        scope.worktree,
    );
    if engine
        .initialize_inference_stream(database, &rebuild, 0)
        .is_ok()
    {
        return Err("zero page admission prepared baseline work".into());
    }
    // Only the generated fixture effect database is removed. Canonical source
    // and parent/child journals survive; recovery needs no original request.
    std::fs::remove_file(database)?;
    if engine
        .initialize_inference_stream(database, &rebuild, 4096)
        .is_ok()
    {
        return Err("engine setup falsely completed without effect database".into());
    }
    drop(engine);
    InferenceMaterializer::migrate(database)?;
    engine = SyntaxMeshEngine::new(open()?, RustExtractor, scope.repository, scope.worktree);
    if engine.recover_inference()?
        != (InferenceRecovery {
            contract_history: 0,
            baselines: 1,
            setups: 1,
            stream_effects: 0,
        })
    {
        return Err("engine did not recover whole setup intent after reopening".into());
    }
    let origin = engine.initialize_inference_stream(database, &rebuild, 4096)?;
    let bounds = InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let successor = StreamSyncRequest {
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
    let closed = engine.synchronize_inference_stream(database, &successor)?;
    let restored = GenerationId::derive(&[b"restored-declaration"]);
    let live = engine.synchronize_inference_stream(
        database,
        &StreamSyncRequest {
            through: restored,
            ..successor
        },
    )?;
    if closed.active_claims != 0
        || live.active_claims != 1
        || engine.initialize_inference_stream(database, &rebuild, 4096)? != origin
        || engine.recover_inference()?
            != (InferenceRecovery {
                contract_history: 0,
                baselines: 0,
                setups: 0,
                stream_effects: 0,
            })
    {
        return Err("engine inference composition lost receipts or reset head".into());
    }
    drop(engine);
    let mut reopened =
        SyntaxMeshEngine::new(open()?, RustExtractor, scope.repository, scope.worktree);
    if reopened.synchronize_inference_stream(database, &successor)? != closed
        || reopened.initialize_inference_stream(database, &rebuild, 4096)? != origin
    {
        return Err("engine inference retry required replay after reopen".into());
    }
    let page_request = syntaxmesh_inference_sqlite::StreamPageRequest {
        origin,
        generation: declared,
        cursor: None,
        claim_budget: 1,
        byte_budget: 16_384,
        premise_budget: 100,
        page_budget: 4096,
    };
    let direct = InferenceMaterializer::open(database)?;
    let baseline_page = reopened.inference_claims_page(database, &page_request)?;
    let detached = SyntaxMeshEngine::new(
        syntaxmesh_store::InMemoryGraphStore::new(),
        RustExtractor,
        scope.repository,
        scope.worktree,
    );
    if detached.inference_claims_page(database, &page_request)? != baseline_page
        || detached.inference_checkpoint(database, origin, retracted)? != Some(closed)
    {
        return Err("retained engine read required canonical history".into());
    }
    if baseline_page != direct.stream_claims_page(&page_request)?
        || baseline_page.claims.len() != 1
        || reopened.inference_checkpoint(database, origin, retracted)? != Some(closed)
        || !reopened
            .inference_claims_page(
                database,
                &syntaxmesh_inference_sqlite::StreamPageRequest {
                    generation: retracted,
                    ..page_request
                },
            )?
            .claims
            .is_empty()
        || reopened
            .inference_claims_page(
                database,
                &syntaxmesh_inference_sqlite::StreamPageRequest {
                    claim_budget: 0,
                    ..page_request
                },
            )
            .is_ok()
    {
        return Err("engine retained read lost selection or admission".into());
    }
    let cursor = syntaxmesh_inference_sqlite::StreamCursor {
        stream: origin.stream,
        selection: baseline_page.checkpoint.selection,
        root: None,
        after: baseline_page
            .claims
            .first()
            .ok_or("missing baseline claim")?
            .id,
    };
    if reopened
        .inference_claims_page(
            database,
            &syntaxmesh_inference_sqlite::StreamPageRequest {
                cursor: Some(cursor),
                ..page_request
            },
        )
        .is_ok()
    {
        return Err("engine retained read admitted substituted root".into());
    }
    let mut wrong_scope = SyntaxMeshEngine::new(
        reopened.into_store(),
        RustExtractor,
        RepositoryId::derive(&[b"other-engine"]),
        scope.worktree,
    );
    let unavailable = database.with_extension("missing");
    let scope_error = wrong_scope
        .inference_claims_page(&unavailable, &page_request)
        .err()
        .ok_or("foreign read must reject")?;
    if !scope_error.to_string().contains("outside engine scope") || unavailable.exists() {
        return Err("foreign read opened destination before scope admission".into());
    }
    if wrong_scope
        .synchronize_inference_stream(database, &successor)
        .is_ok()
    {
        return Err("foreign engine synchronized retained stream".into());
    }
    if InferenceMaterializer::open(database)?
        .stream_claims(origin, restored, 10, 16_384, 100, 4096)?
        .claims
        .len()
        != 1
    {
        return Err("engine retry changed retained active roots".into());
    }
    Ok(())
}

#[test]
fn file_engine_composes_scoped_inference_with_borrowed_journals()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.snapshot");
    exercise(
        || Ok(syntaxmesh_store::FileGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn turso_engine_composes_scoped_inference_with_borrowed_journals()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let graph = directory.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&graph)?;
    exercise(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&graph)?),
        &directory.path().join("inference.sqlite"),
    )
}
