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
    if !point_plan.contains("inference_contract_evaluations_at")
        || !history_plan.contains("inference_contract_evaluations_history")
    {
        return Err(format!("contract result reads did not use their dedicated temporal indexes: point={point_plan}; history={history_plan}").into());
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
            .contract_history(&history_query(repository, worktree, None, 10))?
            .items
            .len()
            != 3
    {
        return Err("reopened materializer changed historical contract results".into());
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
    Ok(())
}
