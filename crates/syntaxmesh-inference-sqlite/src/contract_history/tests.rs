use super::*;
use crate::test_fixture;
use syntaxmesh_core::{NodeId, RelationKind};
use syntaxmesh_inference::{ContractLimits, ContractOrigin, ContractOutcome, RelationExpectation};
use syntaxmesh_store::InMemoryGraphStore;

const LIMITS: ContractLimits = ContractLimits {
    contracts: 4,
    input_bytes: 16_384,
    edge_visits: 100,
    results: 4,
};

fn contracts() -> [Contract; 2] {
    let entry = NodeId::derive(&[b"entry"]);
    let writer = NodeId::derive(&[b"writer"]);
    let target = NodeId::derive(&[b"target"]);
    let concept = NodeId::derive(&[b"concept"]);
    [
        Contract {
            id: "entry-calls-writer".to_owned(),
            version: "1".to_owned(),
            origin: ContractOrigin::Declared,
            source: entry,
            target: writer,
            relation: RelationKind::Calls,
            expectation: RelationExpectation::Required,
        },
        Contract {
            id: "target-not-in-concept".to_owned(),
            version: "1".to_owned(),
            origin: ContractOrigin::Documented,
            source: target,
            target: concept,
            relation: RelationKind::External {
                namespace: "example".to_owned(),
                relation: "instance_of".to_owned(),
            },
            expectation: RelationExpectation::Forbidden,
        },
    ]
}

fn request<'contracts>(
    generation: GenerationId,
    contracts: &'contracts [Contract],
    output_bytes: usize,
) -> ContractHistoryRequest<'contracts> {
    ContractHistoryRequest {
        generation,
        contracts,
        limits: &LIMITS,
        history_scan_budget: 100,
        output_bytes,
    }
}

fn history_query(
    repository: syntaxmesh_core::RepositoryId,
    worktree: syntaxmesh_core::WorktreeId,
    after: Option<ContractHistoryCursor>,
    limit: usize,
) -> ContractHistoryQuery {
    ContractHistoryQuery {
        repository,
        worktree,
        contract_id: "target-not-in-concept".to_owned(),
        contract_version: "1".to_owned(),
        after,
        limit,
    }
}

#[test]
fn retained_contract_results_are_exact_indexed_and_restart_safe() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("contract-history.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = InMemoryGraphStore::new();
    let (_rule, declared, retracted_generation) = test_fixture::fixture(&mut store)?;
    let first = GenerationId::derive(&[b"first"]);
    let contracts = contracts();

    materializer.connection.execute_batch(
        "CREATE TRIGGER interrupt_contract_publish BEFORE INSERT ON inference_contract_evaluations
         BEGIN SELECT RAISE(ABORT, 'injected contract publication failure'); END;",
    )?;
    if materializer
        .retain_contract_history(&store, &request(first, &contracts, 16_384))
        .is_ok()
    {
        return Err("injected partial contract publication succeeded".into());
    }
    let partial_sets: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_contract_evaluation_sets",
        [],
        |row| row.get(0),
    )?;
    if partial_sets != 0 {
        return Err("failed contract publication retained its batch receipt".into());
    }
    materializer
        .connection
        .execute_batch("DROP TRIGGER interrupt_contract_publish")?;

    if materializer
        .retain_contract_history(&store, &request(first, &contracts, 1))
        .is_ok()
    {
        return Err("undersized contract-history output budget published".into());
    }
    let retained =
        materializer.retain_contract_history(&store, &request(first, &contracts, 16_384))?;
    let introduced =
        materializer.retain_contract_history(&store, &request(declared, &contracts, 16_384))?;
    let retracted = materializer
        .retain_contract_history(&store, &request(retracted_generation, &contracts, 16_384))?;
    if materializer.retain_contract_history(&store, &request(first, &contracts, 16_384))?
        != retained
    {
        return Err("identical contract history retry changed its receipt".into());
    }
    if retained.result_count != 2
        || introduced.generation_sequence <= retained.generation_sequence
        || retracted.generation_sequence <= introduced.generation_sequence
    {
        return Err("contract history receipts lost accepted-generation order".into());
    }

    let repository = retained.repository;
    let worktree = retained.worktree;
    let first_result = materializer
        .contract_result_at(repository, worktree, first, "target-not-in-concept", "1")?
        .ok_or("missing first contract result")?;
    let declared_result = materializer
        .contract_result_at(repository, worktree, declared, "target-not-in-concept", "1")?
        .ok_or("missing declared contract result")?;
    let retracted_result = materializer
        .contract_result_at(
            repository,
            worktree,
            retracted_generation,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("missing retracted contract result")?;
    if first_result.evaluation.outcome != ContractOutcome::InsufficientEvidence
        || declared_result.evaluation.outcome != ContractOutcome::Contradicted
        || retracted_result.evaluation.outcome != ContractOutcome::InsufficientEvidence
    {
        return Err("materialized history did not preserve generation-scoped outcomes".into());
    }
    let first_episode = materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository,
            worktree,
            evaluation_sequence: first_result.generation_sequence,
            evaluation_generation: first,
            expected_outcome: first_result.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .ok_or("missing first outcome episode")?;
    let declared_episode = materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository,
            worktree,
            evaluation_sequence: declared_result.generation_sequence,
            evaluation_generation: declared,
            expected_outcome: declared_result.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .ok_or("missing contradicted outcome episode")?;
    let retracted_episode = materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository,
            worktree,
            evaluation_sequence: retracted_result.generation_sequence,
            evaluation_generation: retracted_generation,
            expected_outcome: retracted_result.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .ok_or("missing restored indeterminate outcome episode")?;
    if first_episode.outcome != ContractOutcome::InsufficientEvidence
        || first_episode.first_generation != first
        || first_episode.last_generation != first
        || declared_episode.outcome != ContractOutcome::Contradicted
        || declared_episode.first_generation != declared
        || retracted_episode.outcome != ContractOutcome::InsufficientEvidence
        || retracted_episode.first_generation != retracted_generation
    {
        return Err("outcome episodes did not retain observed transition boundaries".into());
    }
    let first_timeline = materializer.contract_outcome_timeline(&ContractOutcomeTimelineQuery {
        repository,
        worktree,
        contract_id: "target-not-in-concept".to_owned(),
        contract_version: "1".to_owned(),
        after: None,
        limit: 1,
    })?;
    if first_timeline.items != [first_episode.clone()]
        || first_timeline
            .next_cursor
            .as_ref()
            .map(|cursor| cursor.first_sequence)
            != Some(first_episode.first_sequence)
    {
        return Err("outcome timeline first page or cursor was incorrect".into());
    }
    let second_timeline =
        materializer.contract_outcome_timeline(&ContractOutcomeTimelineQuery {
            repository,
            worktree,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
            after: first_timeline.next_cursor,
            limit: 2,
        })?;
    if second_timeline.items != [declared_episode.clone(), retracted_episode]
        || second_timeline.next_cursor.is_some()
    {
        return Err("outcome timeline continuation lost transition order".into());
    }
    if materializer
        .contract_outcome_timeline(&ContractOutcomeTimelineQuery {
            repository,
            worktree,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
            after: Some(ContractOutcomeTimelineCursor {
                repository,
                worktree,
                contract_id: "another-contract".to_owned(),
                contract_version: "1".to_owned(),
                first_sequence: first_episode.first_sequence,
            }),
            limit: 1,
        })
        .is_ok()
    {
        return Err("cross-contract outcome timeline cursor was accepted".into());
    }
    materializer
        .connection
        .execute("DELETE FROM inference_contract_outcome_episodes", [])?;
    InferenceMaterializer::backfill_outcome_episodes(&materializer.connection)?;
    let backfilled_episode = materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository,
            worktree,
            evaluation_sequence: declared_result.generation_sequence,
            evaluation_generation: declared,
            expected_outcome: declared_result.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .ok_or("migration backfill omitted a retained outcome episode")?;
    if backfilled_episode != declared_episode {
        return Err("outcome episode migration backfill differs from live projection".into());
    }
    let before_first = materializer.contract_result_at_or_before(
        repository,
        worktree,
        retained.generation_sequence.saturating_sub(1),
        "target-not-in-concept",
        "1",
    )?;
    let at_declared = materializer
        .contract_result_at_or_before(
            repository,
            worktree,
            introduced.generation_sequence,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("as-of lookup missed its latest eligible result")?;
    let before_retracted = materializer
        .contract_result_at_or_before(
            repository,
            worktree,
            introduced.generation_sequence,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("as-of lookup missed the result before a later retained result")?;
    let at_retracted = materializer
        .contract_result_at_or_before(
            repository,
            worktree,
            retracted.generation_sequence,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("as-of lookup missed its latest retained result")?;
    if before_first.is_some()
        || at_declared.evaluation.generation != declared
        || before_retracted.evaluation.generation != declared
        || at_retracted.evaluation.generation != retracted_generation
    {
        return Err(
            "as-of contract lookup returned an unavailable, future, or incorrect result".into(),
        );
    }
    let latest_set = materializer
        .latest_contract_set(repository, worktree)?
        .ok_or("missing latest retained contract set")?;
    if latest_set.watermark != retracted || latest_set.contracts != contracts {
        return Err("latest retained contract set changed its source declarations".into());
    }
    let point_plan = materializer
        .connection
        .prepare(
            "EXPLAIN QUERY PLAN SELECT generation_sequence, payload, payload_digest
         FROM inference_contract_evaluations WHERE repository = ?1 AND worktree = ?2
         AND generation = ?3 AND contract_id = ?4 AND contract_version = ?5",
        )?
        .query_map(rusqlite::params!["", "", "", "", ""], |row| {
            row.get::<_, String>(3)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join(" ");
    let history_plan = materializer
        .connection
        .prepare(
            "EXPLAIN QUERY PLAN SELECT generation_sequence, generation, payload, payload_digest
         FROM inference_contract_evaluations WHERE repository = ?1 AND worktree = ?2
         AND contract_id = ?3 AND contract_version = ?4 AND generation_sequence > ?5
         ORDER BY generation_sequence, generation LIMIT ?6",
        )?
        .query_map(rusqlite::params!["", "", "", "", 0_i64, 1_i64], |row| {
            row.get::<_, String>(3)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join(" ");
    let as_of_plan = materializer
        .connection
        .prepare(
            "EXPLAIN QUERY PLAN SELECT generation_sequence, generation, evaluation_id, payload, payload_digest
             FROM inference_contract_evaluations WHERE repository = ?1 AND worktree = ?2
             AND contract_id = ?3 AND contract_version = ?4 AND generation_sequence <= ?5
             ORDER BY generation_sequence DESC, generation DESC LIMIT 1",
        )?
        .query_map(
            rusqlite::params!["", "", "", "", 1_i64],
            |row| row.get::<_, String>(3),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join(" ");
    let episode_plan = materializer
        .connection
        .prepare(
            "EXPLAIN QUERY PLAN SELECT outcome, first_generation, first_sequence, ended_sequence
             FROM inference_contract_outcome_episodes WHERE repository = ?1 AND worktree = ?2
             AND contract_id = ?3 AND contract_version = ?4 AND first_sequence <= ?5
             AND (ended_sequence IS NULL OR ended_sequence > ?5)
             ORDER BY first_sequence DESC LIMIT 1",
        )?
        .query_map(rusqlite::params!["", "", "", "", 1_i64], |row| {
            row.get::<_, String>(3)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join(" ");
    if !point_plan.contains("inference_contract_evaluations_at")
        || !history_plan.contains("inference_contract_evaluations_history")
        || !as_of_plan.contains("inference_contract_evaluations_history")
        || !episode_plan.contains("inference_contract_outcome_episodes_as_of")
    {
        return Err(format!("contract result reads did not use their dedicated temporal indexes: point={point_plan}; history={history_plan}; as_of={as_of_plan}; episode={episode_plan}").into());
    }
    let first_page =
        materializer.contract_history(&history_query(repository, worktree, None, 1))?;
    let cursor = first_page
        .next_cursor
        .ok_or("missing contract-history continuation")?;
    let second_page = materializer.contract_history(&history_query(
        repository,
        worktree,
        Some(cursor.clone()),
        1,
    ))?;
    let second_cursor = second_page
        .next_cursor
        .clone()
        .ok_or("missing second contract-history continuation")?;
    let third_page = materializer.contract_history(&history_query(
        repository,
        worktree,
        Some(second_cursor),
        1,
    ))?;
    let first_generation = first_page
        .items
        .first()
        .map(|item| item.evaluation.generation);
    let second_generation = second_page
        .items
        .first()
        .map(|item| item.evaluation.generation);
    let third_generation = third_page
        .items
        .first()
        .map(|item| item.evaluation.generation);
    if first_page.items.len() != 1
        || second_page.items.len() != 1
        || third_page.items.len() != 1
        || third_page.next_cursor.is_some()
        || first_generation != Some(first)
        || second_generation != Some(declared)
        || third_generation != Some(retracted_generation)
    {
        return Err("contract history keyset paging skipped or reordered results".into());
    }
    let mut foreign_cursor = cursor;
    foreign_cursor.contract_id = "different-contract".to_owned();
    if materializer
        .contract_history(&history_query(
            repository,
            worktree,
            Some(foreign_cursor),
            1,
        ))
        .is_ok()
    {
        return Err("cross-contract history cursor was accepted".into());
    }
    drop(materializer);

    let reopened = InferenceMaterializer::open(&path)?;
    if reopened.contract_result_at(repository, worktree, first, "target-not-in-concept", "1")?
        != Some(first_result)
        || reopened
            .contract_result_at_or_before(
                repository,
                worktree,
                introduced.generation_sequence,
                "target-not-in-concept",
                "1",
            )?
            .is_none_or(|item| item.evaluation.generation != declared)
        || reopened
            .contract_history(&history_query(repository, worktree, None, 10))?
            .items
            .len()
            != 3
    {
        return Err("reopened materializer changed historical contract results".into());
    }
    if reopened
        .latest_contract_set(repository, worktree)?
        .is_none_or(|set| set.watermark != retracted || set.contracts != contracts)
    {
        return Err("reopened latest contract set changed its integrity or declarations".into());
    }
    let operation = retained.operation.to_hex();
    reopened.connection.execute(
        "UPDATE inference_contract_evaluations SET payload = X'00' WHERE operation = ?1 AND contract_id = 'target-not-in-concept'",
        [&operation],
    )?;
    if reopened
        .contract_result_at(repository, worktree, first, "target-not-in-concept", "1")
        .is_ok()
    {
        return Err("tampered retained contract result passed checksum validation".into());
    }
    if reopened
        .contract_result_at_or_before(
            repository,
            worktree,
            retained.generation_sequence,
            "target-not-in-concept",
            "1",
        )
        .is_ok()
    {
        return Err("as-of lookup accepted a tampered retained result".into());
    }
    reopened.connection.execute(
        "UPDATE inference_contract_evaluations SET payload = X'00' WHERE operation = ?1 AND contract_id = 'target-not-in-concept'",
        [retracted.operation.to_hex()],
    )?;
    if reopened.latest_contract_set(repository, worktree).is_ok() {
        return Err("tampered latest contract set passed aggregate integrity validation".into());
    }
    Ok(())
}

#[test]
fn outcome_episodes_restart_after_omission_and_backfill_from_complete_sets() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory
        .path()
        .join("outcome-episode-reintroduction.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = InMemoryGraphStore::new();
    let (_rule, declared, retracted) = test_fixture::fixture(&mut store)?;
    let first = store
        .manifest(declared)?
        .parent
        .ok_or("fixture has no baseline generation")?;
    let contracts = contracts();
    let replacement = [contracts[0].clone()];
    let initial =
        materializer.retain_contract_history(&store, &request(first, &contracts, 16_384))?;
    let omitted =
        materializer.retain_contract_history(&store, &request(declared, &replacement, 16_384))?;
    materializer.retain_contract_history(&store, &request(retracted, &contracts, 16_384))?;
    let prior = materializer
        .contract_result_at(
            initial.repository,
            initial.worktree,
            first,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("missing pre-omission result")?;
    let prior_episode = materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository: initial.repository,
            worktree: initial.worktree,
            evaluation_sequence: prior.generation_sequence,
            evaluation_generation: first,
            expected_outcome: prior.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .ok_or("missing pre-omission episode")?;
    if materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository: initial.repository,
            worktree: initial.worktree,
            evaluation_sequence: omitted.generation_sequence,
            evaluation_generation: declared,
            expected_outcome: prior.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .is_some()
    {
        return Err("omitted contract outcome episode remained open".into());
    }
    let reintroduced = materializer
        .contract_result_at(
            initial.repository,
            initial.worktree,
            retracted,
            "target-not-in-concept",
            "1",
        )?
        .ok_or("missing reintroduced result")?;
    let reintroduced_episode = materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository: initial.repository,
            worktree: initial.worktree,
            evaluation_sequence: reintroduced.generation_sequence,
            evaluation_generation: retracted,
            expected_outcome: reintroduced.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .ok_or("missing reintroduced episode")?;
    if prior_episode.first_generation != first
        || reintroduced_episode.first_generation != retracted
        || reintroduced_episode.outcome != prior_episode.outcome
    {
        return Err("contract reintroduction did not begin a distinct observed episode".into());
    }
    materializer
        .connection
        .execute("DELETE FROM inference_contract_outcome_episodes", [])?;
    InferenceMaterializer::backfill_outcome_episodes(&materializer.connection)?;
    let backfilled = materializer
        .contract_outcome_episode_at(&ContractOutcomeEpisodeQuery {
            repository: initial.repository,
            worktree: initial.worktree,
            evaluation_sequence: reintroduced.generation_sequence,
            evaluation_generation: retracted,
            expected_outcome: reintroduced.evaluation.outcome,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
        })?
        .ok_or("migration backfill omitted the reintroduced episode")?;
    if backfilled != reintroduced_episode {
        return Err(
            "migration backfill did not preserve omission/reintroduction boundaries".into(),
        );
    }
    Ok(())
}

#[test]
fn latest_contract_set_rejects_multiple_sets_at_the_same_generation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("ambiguous-contract-history.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = InMemoryGraphStore::new();
    let (_rule, _, _) = test_fixture::fixture(&mut store)?;
    let generation = GenerationId::derive(&[b"first"]);
    let contracts = contracts();
    materializer.retain_contract_history(&store, &request(generation, &contracts, 16_384))?;
    let mut additional = contracts[0].clone();
    additional.id = "independent-contract-set".to_owned();
    materializer.retain_contract_history(
        &store,
        &request(generation, std::slice::from_ref(&additional), 16_384),
    )?;
    let manifest = store.manifest(generation)?;
    if materializer
        .latest_contract_set(manifest.repository, manifest.worktree)
        .is_ok()
    {
        return Err("ambiguous same-generation contract sets were not rejected".into());
    }
    Ok(())
}
