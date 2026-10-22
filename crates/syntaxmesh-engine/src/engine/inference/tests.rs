use super::*;
use syntaxmesh_core::{GenerationId, NodeId, RepositoryId};
use syntaxmesh_inference::{
    InvalidationLimits, Limits, ONTOLOGY_DEFINITION_SCHEMA_VERSION, OntologyDefinition,
    OntologyRelationDefinition, Rule,
};
use syntaxmesh_inference_sqlite::{InferenceMaterializer, StreamSyncRequest};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_query::Query;

#[path = "../../../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
mod fixture;

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
