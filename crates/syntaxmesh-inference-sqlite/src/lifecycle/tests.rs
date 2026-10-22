use super::*;
use crate::RebuildRequest;

use crate::test_fixture as fixture;

#[test]
fn multiple_closures_keep_earliest_time_and_all_bounds_fail_atomically() -> Result<()> {
    use syntaxmesh_core::{EdgeId, GraphDelta, IndexRunId};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("bounded.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, retained, current) = fixture::fixture(&mut store)?;
    let limits = fixture::limits();
    let rules = [rule];
    let origin = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation: retained,
            rules: &rules,
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
    )?;
    let claim = materializer
        .claims(origin, 1, 16_384)?
        .into_iter()
        .next()
        .ok_or("missing claim")?;
    let later = GenerationId::derive(&[b"second-premise-closure"]);
    store.apply_delta(GraphDelta {
        repository: origin.selection.repository,
        worktree: origin.selection.worktree,
        run_id: IndexRunId::derive(&[b"second-closure-run"]),
        expected_base: Some(current),
        next_generation: later,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: vec![EdgeId::derive(&[b"writes"])],
    })?;
    let before = materializer.claim_lifecycle(origin, claim.id, 2)?;
    let encoded_bytes = usize::try_from(bincode::serialized_size(&claim)?)?;
    for case in 0..6 {
        let mut bounds = InvalidationLimits {
            generations: 2,
            versions: 2,
            postings: 4,
            claims: 1,
        };
        let mut evidence = 2;
        let mut bytes = encoded_bytes;
        match case {
            0 => bounds.generations = 1,
            1 => bounds.versions = 1,
            2 => bounds.postings = 3,
            3 => bounds.claims = 0,
            4 => evidence = 1,
            _ => bytes = encoded_bytes.checked_sub(1).ok_or("zero claim bytes")?,
        }
        if materializer
            .invalidate(
                &store,
                &LifecycleRequest {
                    origin,
                    through: later,
                    limits: &bounds,
                    evidence_budget: evidence,
                    claim_bytes: bytes,
                },
            )
            .is_ok()
            || materializer.claim_lifecycle(origin, claim.id, 2)? != before
            || materializer.lifecycle_receipt(origin, later)?.is_some()
        {
            return Err("exhausted lifecycle budget published partial coverage or reasons".into());
        }
    }
    let bounds = InvalidationLimits {
        generations: 2,
        versions: 2,
        postings: 4,
        claims: 1,
    };
    let closures = Query::new(&store, later).inference_premise_closures(retained, &bounds)?;
    if closures.len() != 2
        || closures
            .values()
            .map(|closure| closure.generation_sequence)
            .min()
            != Some(store.generation_sequence(current)?)
    {
        return Err("closure feed lost earliest accepted coordinate".into());
    }
    let receipt = materializer.invalidate(
        &store,
        &LifecycleRequest {
            origin,
            through: later,
            limits: &bounds,
            evidence_budget: 2,
            claim_bytes: encoded_bytes,
        },
    )?;
    let lifecycle = materializer.claim_lifecycle(origin, claim.id, 2)?;
    if receipt.affected_claims != 1
        || receipt.closure_reasons != 2
        || lifecycle.valid_until.map(|until| until.generation) != Some(current)
        || lifecycle
            .reasons
            .iter()
            .any(|reason| closures.get(&reason.premise) != Some(&reason.closed_at))
        || materializer.claim_lifecycle(origin, claim.id, 1).is_ok()
    {
        return Err("multiple closure lifecycle differs from exact canonical oracle".into());
    }
    let reference = claim.premises.first().ok_or("missing premise")?;
    let mut statement = materializer
        .connection
        .prepare(&format!("EXPLAIN QUERY PLAN {POSTINGS_SQL}"))?;
    let details = statement
        .query_map(
            params![
                origin.operation.to_hex(),
                bincode::serialize(&reference.fact)?,
                reference.valid_from.0.to_hex()
            ],
            |row| row.get::<_, String>(3),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !details
        .iter()
        .any(|detail| detail.contains("inference_premise_postings"))
    {
        return Err("lifecycle posting lookup did not seek dependency index".into());
    }
    Ok(())
}

#[test]
fn direct_lifecycle_retains_exact_closure_reasons_and_does_not_revive_restored_versions()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("inference.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, retained, current) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, retained, current)?;
    let limits = fixture::limits();
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
    let origin = materializer.rebuild(&store, &rebuild)?;
    let claim = materializer
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
    let before = materializer.claim_lifecycle(origin, claim.id, 10)?;
    if before.valid_until.is_some() || before.checked_through.generation != retained {
        return Err("unprocessed future was marked invalid".into());
    }
    materializer.connection.execute_batch("CREATE TRIGGER interrupt_lifecycle BEFORE UPDATE ON inference_lifecycle BEGIN SELECT RAISE(ABORT, 'injected lifecycle failure'); END;")?;
    if materializer.invalidate(&store, &request).is_ok()
        || materializer.claim_lifecycle(origin, claim.id, 10)? != before
    {
        return Err("failed effect changed lifecycle".into());
    }
    materializer
        .connection
        .execute_batch("DROP TRIGGER interrupt_lifecycle")?;
    let accepted = materializer.invalidate(&store, &request)?;
    if materializer.invalidate(&store, &request)? != accepted
        || accepted.affected_claims != 1
        || accepted.closure_reasons != 1
    {
        return Err("non-idempotent closure publication".into());
    }
    let closed = materializer.claim_lifecycle(origin, claim.id, 1)?;
    if closed.valid_until.map(|until| until.generation) != Some(current)
        || closed.reasons.first().is_none_or(|reason| {
            !claim.premises.contains(&reason.premise) || reason.closed_at.generation != current
        })
    {
        return Err("invalidated interval or reason does not match exact premise".into());
    }
    fixture::verify_restoration(&mut store, &rule, retained, current)?;
    let through = store
        .current_generation(origin.selection.repository, origin.selection.worktree)?
        .ok_or("missing restored selection")?
        .generation;
    let restored_request = LifecycleRequest { through, ..request };
    materializer.invalidate(&store, &restored_request)?;
    let retained_closure = materializer.claim_lifecycle(origin, claim.id, 10)?;
    if retained_closure.valid_until != closed.valid_until
        || retained_closure.reasons != closed.reasons
        || retained_closure.checked_through.generation != through
    {
        return Err("restoration revived old derivation or lost coverage".into());
    }
    if materializer
        .invalidate(
            &store,
            &LifecycleRequest {
                through: GenerationId::derive(&[b"restored-declaration"]),
                ..restored_request
            },
        )
        .is_ok()
    {
        return Err("lifecycle coverage regressed".into());
    }
    if materializer.invalidate(
        &store,
        &LifecycleRequest {
            through: current,
            ..restored_request
        },
    )? != accepted
        || materializer.lifecycle_receipt(origin, current)? != Some(accepted)
        || materializer.claim_lifecycle(origin, claim.id, 10)? != retained_closure
    {
        return Err("historical completed retry was lost or rewound coverage".into());
    }
    drop(materializer);
    let reopened = InferenceMaterializer::open(&path)?;
    if reopened.claim_lifecycle(origin, claim.id, 10)? != retained_closure
        || reopened.lifecycle_receipt(origin, current)? != Some(accepted)
    {
        return Err("reopen changed claim lifecycle".into());
    }
    Ok(())
}
