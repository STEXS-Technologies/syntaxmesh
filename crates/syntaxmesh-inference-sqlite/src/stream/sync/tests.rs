use super::*;
use crate::{RebuildRequest, test_fixture};
use syntaxmesh_core::{EdgeId, GraphDelta, IndexRunId};
use syntaxmesh_inference::{
    ONTOLOGY_DEFINITION_SCHEMA_VERSION, OntologyDefinition, OntologyRegistry,
    OntologyRelationDefinition,
};

fn ontology_definition(rule: &Rule) -> OntologyDefinition {
    let source_concept = syntaxmesh_core::NodeId::derive(&[b"source-concept"]);
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
fn registered_sync_reconciles_membership_retraction_and_restoration() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("registered-stream.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut source = syntaxmesh_store::InMemoryGraphStore::new();
    let (mut rule, declared, _) = test_fixture::fixture(&mut source)?;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    let source_manifest = source.manifest(declared)?;
    let snapshot = source.historical_snapshot(declared)?;
    let membership = snapshot
        .edges
        .iter()
        .find(|edge| edge.id == EdgeId::derive(&[b"writer-instance"]))
        .ok_or("missing source membership")?
        .clone();
    let write = snapshot
        .edges
        .iter()
        .find(|edge| {
            matches!(&edge.relation, syntaxmesh_core::RelationKind::External { relation, .. } if relation == "writes")
        })
        .ok_or("missing write fact")?
        .clone();
    let first = GenerationId::derive(&[b"registered-stream-baseline"]);
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository: source_manifest.repository,
        worktree: source_manifest.worktree,
        run_id: IndexRunId::derive(&[b"registered-stream-baseline-run"]),
        expected_base: None,
        next_generation: first,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: snapshot.provenance,
        upsert_nodes: snapshot.nodes,
        upsert_edges: snapshot.edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let retracted = GenerationId::derive(&[b"registered-stream-membership-retracted"]);
    let manifest = store.manifest(first)?;
    store.apply_delta(GraphDelta {
        repository: manifest.repository,
        worktree: manifest.worktree,
        run_id: IndexRunId::derive(&[b"registered-stream-membership-retraction-run"]),
        expected_base: Some(first),
        next_generation: retracted,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: vec![membership.id, write.id],
    })?;
    let rules = [rule];
    let definition = ontology_definition(rules.first().ok_or("missing rule")?);
    let registry = OntologyRegistry::from_definitions(&[definition], 16, 16_384)?;
    let limits = test_fixture::limits();
    let baseline = materializer.rebuild_registered(
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
        &registry,
        1,
    )?;
    let origin = materializer.begin_stream(baseline, 10, 16_384, 100, 4096)?;
    let range = bounds();
    let mut under_admitted = request(origin, retracted, &rules, &limits, &range);
    under_admitted.evidence_budget = 1;
    if materializer
        .sync_stream_registered(&store, &under_admitted, &registry, 1)
        .is_ok()
        || materializer.stream_checkpoint(origin, retracted)?.is_some()
        || materializer.stream_checkpoint(origin, first)?.is_none()
    {
        return Err("under-admitted membership reconciliation partially published".into());
    }
    let retraction = materializer.sync_stream_registered(
        &store,
        &request(origin, retracted, &rules, &limits, &range),
        &registry,
        1,
    )?;
    let retracted_claims = materializer.stream_claims(origin, retracted, 10, 16_384, 100, 4096)?;
    if retraction.affected_claims != 1
        || retraction.active_claims != 0
        || retracted_claims.checkpoint != retraction
        || !retracted_claims.claims.is_empty()
    {
        return Err("membership retraction did not reconcile the active stream".into());
    }

    let restored = GenerationId::derive(&[b"registered-stream-membership-restored"]);
    let restoration_parent = store.manifest(retracted)?;
    store.apply_delta(GraphDelta {
        repository: restoration_parent.repository,
        worktree: restoration_parent.worktree,
        run_id: IndexRunId::derive(&[b"registered-stream-membership-restoration-run"]),
        expected_base: Some(retracted),
        next_generation: restored,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: vec![membership, write],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let restoration = materializer.sync_stream_registered(
        &store,
        &request(origin, restored, &rules, &limits, &range),
        &registry,
        1,
    )?;
    let restored_claims = materializer.stream_claims(origin, restored, 10, 16_384, 100, 4096)?;
    let complete =
        Query::new(&store, restored).infer_registered(&rules, &registry, &limits, 100, 1, 1)?;
    if restoration.added_claims != 1
        || restoration.active_claims != 1
        || restored_claims.claims != complete
    {
        return Err("membership restoration diverged from complete registered evaluation".into());
    }
    Ok(())
}

fn request<'controls>(
    origin: StreamOrigin,
    through: GenerationId,
    rules: &'controls [Rule],
    limits: &'controls Limits,
    range_limits: &'controls InvalidationLimits,
) -> StreamSyncRequest<'controls> {
    StreamSyncRequest {
        origin,
        through,
        rules,
        limits,
        range_limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
        evidence_budget: 100,
        matched_claim_bytes: 16_384,
        page_budget: 4096,
    }
}

fn bounds() -> InvalidationLimits {
    InvalidationLimits {
        generations: 1,
        versions: 100,
        postings: 100,
        claims: 10,
    }
}

fn verify_active(
    materializer: &InferenceMaterializer,
    store: &impl GraphStore,
    origin: StreamOrigin,
    at: GenerationId,
    rules: &[Rule],
    limits: &Limits,
) -> Result<()> {
    let receipt = materializer
        .stream_checkpoint(origin, at)?
        .ok_or("checkpoint missing")?;
    let mut statement = materializer.connection.prepare(
        "SELECT payload FROM inference_stream_claims WHERE stream = ?1 AND born_sequence <= ?2
         AND (closed_sequence IS NULL OR closed_sequence > ?2) ORDER BY claim",
    )?;
    let payloads = statement
        .query_map(
            params![origin.stream.to_hex(), i64::try_from(receipt.sequence)?],
            |row| row.get::<_, Vec<u8>>(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let retained = payloads
        .iter()
        .map(|payload| bincode::deserialize::<Claim>(payload))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let complete = Query::new(store, at).infer_rules(rules, limits, 100, 1)?;
    let view = materializer.stream_claims(origin, at, 100, 65_536, 1000, 4096)?;
    if view.checkpoint != receipt || view.claims != retained {
        return Err("public stream read disagreed with independent interval oracle".into());
    }
    let witnesses = |claims: &[Claim]| {
        let mut witnesses = claims
            .iter()
            .map(|claim| {
                (
                    claim.source,
                    claim.target,
                    claim.relation.clone(),
                    claim.premises.clone(),
                )
            })
            .collect::<Vec<_>>();
        witnesses.sort_by(|left, right| {
            (&left.0, &left.1, &left.3).cmp(&(&right.0, &right.1, &right.3))
        });
        witnesses
    };
    if retained.len() != receipt.active_claims || witnesses(&retained) != witnesses(&complete) {
        return Err("stream checkpoint active witnesses differ from complete oracle".into());
    }
    Ok(())
}

#[test]
fn create_close_restore_and_empty_successors_retain_transient_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("successors.sqlite");
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
    let range = bounds();
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
    if materializer
        .sync_stream(&store, &request(origin, retracted, &rules, &limits, &range))
        .is_ok()
    {
        return Err("stream skipped an intervening transient witness".into());
    }
    let mut creation = None;
    for (at, added, affected, active) in [
        (declared, 1, 0, 1),
        (retracted, 0, 1, 0),
        (restored, 1, 0, 1),
        (empty, 0, 0, 1),
    ] {
        let checkpoint =
            materializer.sync_stream(&store, &request(origin, at, &rules, &limits, &range))?;
        if checkpoint.added_claims != added
            || checkpoint.affected_claims != affected
            || checkpoint.active_claims != active
        {
            return Err("stream effect counters disagree with witness lifecycle".into());
        }
        if at == declared {
            creation = Some(checkpoint);
        }
        verify_active(&materializer, &store, origin, at, &rules, &limits)?;
    }
    let original = creation.ok_or("creation checkpoint missing")?;
    if materializer.sync_stream(&store, &request(origin, declared, &rules, &limits, &range))?
        != original
    {
        return Err("historical retry lost exact checkpoint".into());
    }
    verify_active(&materializer, &store, origin, declared, &rules, &limits)?;
    verify_active(&materializer, &store, origin, retracted, &rules, &limits)?;
    let births: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_stream_claims WHERE stream = ?1",
        [origin.stream.to_hex()],
        |row| row.get(0),
    )?;
    let head: String = materializer.connection.query_row(
        "SELECT head_generation FROM inference_streams WHERE stream = ?1",
        [origin.stream.to_hex()],
        |row| row.get(0),
    )?;
    if births != 2 || head != empty.0.to_hex() {
        return Err("unchanged witnesses copied or historical retry rewound head".into());
    }
    let pages: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_active_pages",
        [],
        |row| row.get(0),
    )?;
    if pages != 2
        || crate::stream::tree::root(&materializer.connection, origin.stream, restored)?
            != crate::stream::tree::root(&materializer.connection, origin.stream, empty)?
    {
        return Err("unchanged checkpoint copied active pages or changed its root".into());
    }
    drop(materializer);
    let reopened = InferenceMaterializer::open(&path)?;
    for at in [first, declared, retracted, restored, empty] {
        verify_active(&reopened, &store, origin, at, &rules, &limits)?;
    }
    Ok(())
}

#[test]
fn replacing_a_premise_closes_and_discovers_atomically_with_independent_bounds() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("atomic.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, declared, retracted) = test_fixture::fixture(&mut store)?;
    test_fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
    let restored = GenerationId::derive(&[b"restored-declaration"]);
    let prior = GenerationId::derive(&[b"empty-followup"]);
    let mut changed_edges = store
        .historical_snapshot(restored)?
        .edges
        .into_iter()
        .filter(|edge| {
            [EdgeId::derive(&[b"instance"]), EdgeId::derive(&[b"writes"])].contains(&edge.id)
        })
        .collect::<Vec<_>>();
    if changed_edges.len() != 2 {
        return Err("missing retained premises".into());
    }
    for edge in &mut changed_edges {
        edge.extension_payload = Some(syntaxmesh_core::ExtensionPayload {
            namespace: "test.inference".to_owned(),
            schema_version: 1,
            bytes: b"revision-2".to_vec(),
        });
    }
    let through = GenerationId::derive(&[b"changed-declaration"]);
    let manifest = store.manifest(prior)?;
    store.apply_delta(GraphDelta {
        repository: manifest.repository,
        worktree: manifest.worktree,
        run_id: IndexRunId::derive(&[b"changed-declaration-run"]),
        expected_base: Some(prior),
        next_generation: through,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: changed_edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let limits = test_fixture::limits();
    let rules = [rule];
    let range = bounds();
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation: prior,
            rules: &rules,
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
    )?;
    let origin = materializer.begin_stream(baseline, 10, 16_384, 100, 4096)?;
    for control in 0..10 {
        let mut admission = range;
        let mut evaluation = limits;
        match control {
            4 => admission.postings = 1,
            5 => admission.versions = 1,
            6 => evaluation.edge_visits = 1,
            _ => {}
        }
        let mut bounded = request(origin, through, &rules, &evaluation, &admission);
        match control {
            0 => bounded.matched_claim_bytes = 1,
            1 => bounded.output_bytes = 1,
            2 => bounded.premise_budget = 1,
            3 => bounded.history_scan_budget = 1,
            7 => bounded.evidence_budget = 0,
            8 => bounded.evidence_budget = 1,
            9 => bounded.page_budget = 1,
            _ => {}
        }
        if materializer.sync_stream(&store, &bounded).is_ok() {
            return Err("stream exhausted bounds published".into());
        }
        if materializer.stream_checkpoint(origin, through)?.is_some() {
            return Err("failed stream effect published checkpoint".into());
        }
        verify_active(&materializer, &store, origin, prior, &rules, &limits)?;
    }
    materializer.connection.execute_batch("CREATE TRIGGER interrupt_sync BEFORE INSERT ON inference_stream_checkpoints BEGIN SELECT RAISE(ABORT, 'injected synchronization failure'); END;")?;
    if materializer
        .sync_stream(&store, &request(origin, through, &rules, &limits, &range))
        .is_ok()
    {
        return Err("stream checkpoint failure ignored".into());
    }
    let (claims, closed, reasons): (i64, i64, i64) = materializer.connection.query_row(
        "SELECT (SELECT COUNT(*) FROM inference_stream_claims), (SELECT COUNT(*) FROM inference_stream_claims WHERE closed_sequence IS NOT NULL), (SELECT COUNT(*) FROM inference_stream_closures)",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if claims != 1 || closed != 0 || reasons != 0 {
        return Err("checkpoint failure left partial close/discovery publication".into());
    }
    let pages: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_active_pages",
        [],
        |row| row.get(0),
    )?;
    let roots: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_stream_roots",
        [],
        |row| row.get(0),
    )?;
    if pages != 1 || roots != 1 {
        return Err("failed checkpoint left partial active pages or roots".into());
    }
    materializer
        .connection
        .execute_batch("DROP TRIGGER interrupt_sync")?;
    let receipt =
        materializer.sync_stream(&store, &request(origin, through, &rules, &limits, &range))?;
    if receipt.added_claims != 1
        || receipt.affected_claims != 1
        || receipt.closure_reasons != 2
        || receipt.active_claims != 1
    {
        return Err("mixed closure/discovery effect counts incorrect".into());
    }
    verify_active(&materializer, &store, origin, prior, &rules, &limits)?;
    verify_active(&materializer, &store, origin, through, &rules, &limits)?;
    Ok(())
}

#[test]
fn changed_configuration_corrupt_matched_payload_and_checkpoint_reject_without_publication()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("integrity.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, generation, through) = test_fixture::fixture(&mut store)?;
    let limits = test_fixture::limits();
    let mut changed_rule = rule.clone();
    changed_rule.version = "2".to_owned();
    let rules = [rule];
    let range = bounds();
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation,
            rules: &rules,
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
    )?;
    let origin = materializer.begin_stream(baseline, 10, 16_384, 100, 4096)?;
    if materializer
        .sync_stream(
            &store,
            &request(origin, through, &[changed_rule], &limits, &range),
        )
        .is_ok()
    {
        return Err("stream accepted changed rule configuration".into());
    }
    let claim = materializer
        .claims(baseline, 1, 16_384)?
        .into_iter()
        .next()
        .ok_or("missing baseline claim")?;
    let reference = claim.premises.last().ok_or("missing premise")?;
    let mut plan = materializer
        .connection
        .prepare(&format!("EXPLAIN QUERY PLAN {POSTINGS_SQL}"))?;
    let details = plan
        .query_map(
            params![
                origin.stream.to_hex(),
                bincode::serialize(&reference.fact)?,
                reference.valid_from.0.to_hex()
            ],
            |row| row.get::<_, String>(3),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !details
        .iter()
        .any(|detail| detail.contains("inference_stream_premise_postings"))
    {
        return Err("actual stream posting query did not use premise index".into());
    }
    drop(plan);
    materializer.connection.execute(
        "UPDATE inference_stream_claims SET payload_digest = 'corrupt' WHERE stream = ?1",
        [origin.stream.to_hex()],
    )?;
    if materializer
        .sync_stream(&store, &request(origin, through, &rules, &limits, &range))
        .is_ok()
        || materializer.stream_checkpoint(origin, through)?.is_some()
    {
        return Err("corrupt matched witness published a checkpoint".into());
    }
    materializer.connection.execute(
        "UPDATE inference_stream_claims SET payload_digest = ?2 WHERE stream = ?1",
        params![
            origin.stream.to_hex(),
            blake3::hash(&bincode::serialize(&claim)?)
                .to_hex()
                .to_string()
        ],
    )?;
    let checkpoint =
        materializer.sync_stream(&store, &request(origin, through, &rules, &limits, &range))?;
    if checkpoint.affected_claims != 1 {
        return Err("repaired witness did not synchronize".into());
    }
    materializer.connection.execute("UPDATE inference_stream_checkpoints SET active_claims = 1 WHERE stream = ?1 AND generation = ?2", params![origin.stream.to_hex(), through.0.to_hex()])?;
    if materializer.stream_checkpoint(origin, through).is_ok() {
        return Err("corrupt checkpoint count accepted".into());
    }
    Ok(())
}
