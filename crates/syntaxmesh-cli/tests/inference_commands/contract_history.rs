use super::{fixture, invoke, text};
use std::path::Path;
use std::result::Result;
use std::{error::Error, path::PathBuf};
use syntaxmesh_core::{NodeId, RelationKind};
use syntaxmesh_inference::{Contract, ContractLimits, ContractOrigin, RelationExpectation};
use syntaxmesh_inference_sqlite::{ContractHistoryRequest, InferenceMaterializer};
use syntaxmesh_store::{FileGraphStore, GraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

fn prepare_and_catch_up<S: GraphStore>(
    mut store: S,
    canonical: &Path,
    downstream: &Path,
    command: &str,
) -> Result<(), Box<dyn Error>> {
    let (rule, declared, retracted) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, declared, retracted)?;
    fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
    let latest_generation = store
        .generation_history()?
        .last()
        .ok_or("fixture has no current generation after restoration")?
        .manifest
        .generation;
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
    InferenceMaterializer::migrate(downstream)?;
    let baseline = InferenceMaterializer::open(downstream)?.retain_contract_history(
        &store,
        &ContractHistoryRequest {
            generation: first,
            contracts: &contracts,
            limits: &limits,
            history_scan_budget: 100,
            output_bytes: 16_384,
        },
    )?;
    drop(store);
    let rejected = invoke(&[
        command,
        text(canonical)?,
        text(downstream)?,
        &retracted.0.to_hex(),
        "1",
        "100",
        "16384",
        "100",
        "16384",
    ])?;
    if rejected.status.success()
        || InferenceMaterializer::open(downstream)?
            .latest_contract_set(manifest.repository, manifest.worktree)?
            .is_none_or(|set| set.watermark != baseline)
    {
        return Err("CLI catch-up budget rejection changed the retained head".into());
    }

    let output = invoke(&[
        command,
        text(canonical)?,
        text(downstream)?,
        &retracted.0.to_hex(),
        "2",
        "100",
        "16384",
        "100",
        "16384",
    ])?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let first_generation = first.0.to_hex();
    let retracted_generation = retracted.0.to_hex();
    if receipt.get("version").and_then(serde_json::Value::as_u64) != Some(1)
        || receipt
            .get("starting_generation")
            .and_then(serde_json::Value::as_str)
            != Some(first_generation.as_str())
        || receipt
            .get("through_generation")
            .and_then(serde_json::Value::as_str)
            != Some(retracted_generation.as_str())
        || receipt
            .get("generations_advanced")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
    {
        return Err(format!("CLI returned incorrect contract catch-up receipt: {receipt}").into());
    }
    let materializer = InferenceMaterializer::open(downstream)?;
    let history =
        materializer.contract_history(&syntaxmesh_inference_sqlite::ContractHistoryQuery {
            repository: manifest.repository,
            worktree: manifest.worktree,
            contract_id: "target-not-in-concept".to_owned(),
            contract_version: "1".to_owned(),
            after: None,
            limit: 10,
        })?;
    let generations = history
        .items
        .iter()
        .map(|item| item.evaluation.generation)
        .collect::<Vec<_>>();
    if generations != [first, declared, retracted] {
        return Err("CLI catch-up did not retain ordered results for every successor".into());
    }
    let history_command = if command.ends_with("-turso") {
        "inference-contract-history-turso"
    } else {
        "inference-contract-history"
    };
    let first_page = invoke(&[
        history_command,
        text(canonical)?,
        text(downstream)?,
        "target-not-in-concept",
        "1",
        "2",
    ])?;
    if !first_page.status.success() {
        return Err(String::from_utf8_lossy(&first_page.stderr)
            .into_owned()
            .into());
    }
    let first_page_json: serde_json::Value = serde_json::from_slice(&first_page.stdout)?;
    let expected_first_items = serde_json::to_value(
        history
            .items
            .get(..2)
            .ok_or("retained history fixture has fewer than two results")?,
    )?;
    let next_cursor = first_page_json
        .get("next_cursor")
        .filter(|cursor| !cursor.is_null())
        .ok_or("first contract-history page has no continuation cursor")?;
    if first_page_json
        .get("version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || first_page_json
            .get("temporal_claim")
            .and_then(serde_json::Value::as_str)
            != Some("retained_evaluation_generations_only")
        || first_page_json
            .get("items")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            != Some(2)
        || first_page_json.get("items") != Some(&expected_first_items)
    {
        return Err("contract-history CLI returned an invalid first page".into());
    }
    let cursor_file = canonical
        .parent()
        .ok_or("canonical fixture has no parent")?
        .join(format!("contract-history-cursor-{command}.json"));
    std::fs::write(&cursor_file, serde_json::to_vec(next_cursor)?)?;
    let second_page = invoke(&[
        history_command,
        text(canonical)?,
        text(downstream)?,
        "target-not-in-concept",
        "1",
        "2",
        text(&cursor_file)?,
    ])?;
    if !second_page.status.success() {
        return Err(String::from_utf8_lossy(&second_page.stderr)
            .into_owned()
            .into());
    }
    let second_page_json: serde_json::Value = serde_json::from_slice(&second_page.stdout)?;
    let expected_second_items = serde_json::to_value(
        history
            .items
            .get(2..)
            .ok_or("retained history fixture has no final result")?,
    )?;
    if second_page_json
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        != Some(1)
        || second_page_json.get("next_cursor") != Some(&serde_json::Value::Null)
        || second_page_json.get("items") != Some(&expected_second_items)
    {
        return Err("contract-history CLI did not return the final bounded page".into());
    }
    let mismatched_cursor = invoke(&[
        history_command,
        text(canonical)?,
        text(downstream)?,
        "different-contract",
        "1",
        "2",
        text(&cursor_file)?,
    ])?;
    if mismatched_cursor.status.success() {
        return Err("contract-history CLI accepted a cursor for another contract".into());
    }
    let violation_command = if command.ends_with("-turso") {
        "inference-violation-history-turso"
    } else {
        "inference-violation-history"
    };
    let violations = invoke(&[
        violation_command,
        text(canonical)?,
        text(downstream)?,
        "target-not-in-concept",
        "1",
        "10",
    ])?;
    if !violations.status.success() {
        return Err(String::from_utf8_lossy(&violations.stderr)
            .into_owned()
            .into());
    }
    let violations: serde_json::Value = serde_json::from_slice(&violations.stdout)?;
    if violations
        .get("temporal_claim")
        .and_then(serde_json::Value::as_str)
        != Some("contradicted_evaluation_generations_only")
        || violations
            .get("items")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            != Some(1)
        || violations
            .pointer("/items/0/outcome")
            .and_then(serde_json::Value::as_str)
            != Some("Contradicted")
    {
        return Err("violation-history CLI did not isolate observed contradictions".into());
    }
    let as_of_command = if command.ends_with("-turso") {
        "inference-result-as-of-turso"
    } else {
        "inference-result-as-of"
    };
    let exact = invoke(&[
        as_of_command,
        text(canonical)?,
        text(downstream)?,
        &retracted_generation,
        "target-not-in-concept",
        "1",
    ])?;
    let stale = invoke(&[
        as_of_command,
        text(canonical)?,
        text(downstream)?,
        &latest_generation.0.to_hex(),
        "target-not-in-concept",
        "1",
    ])?;
    let unavailable = invoke(&[
        as_of_command,
        text(canonical)?,
        text(downstream)?,
        &latest_generation.0.to_hex(),
        "not-retained",
        "1",
    ])?;
    for command_output in [&exact, &stale, &unavailable] {
        if !command_output.status.success() {
            return Err(String::from_utf8_lossy(&command_output.stderr)
                .into_owned()
                .into());
        }
    }
    let exact_result: serde_json::Value = serde_json::from_slice(&exact.stdout)?;
    let stale_result: serde_json::Value = serde_json::from_slice(&stale.stdout)?;
    let unavailable_result: serde_json::Value = serde_json::from_slice(&unavailable.stdout)?;
    if exact_result
        .get("freshness")
        .and_then(serde_json::Value::as_str)
        != Some("current")
        || exact_result
            .get("evaluation_generation")
            .and_then(serde_json::Value::as_str)
            != Some(retracted_generation.as_str())
        || stale_result
            .get("freshness")
            .and_then(serde_json::Value::as_str)
            != Some("stale")
        || stale_result
            .get("evaluation_generation")
            .and_then(serde_json::Value::as_str)
            != Some(retracted_generation.as_str())
        || unavailable_result
            .get("freshness")
            .and_then(serde_json::Value::as_str)
            != Some("unavailable")
        || unavailable_result.get("evaluation") != Some(&serde_json::Value::Null)
        || InferenceMaterializer::open(downstream)?
            .contract_history(&syntaxmesh_inference_sqlite::ContractHistoryQuery {
                repository: manifest.repository,
                worktree: manifest.worktree,
                contract_id: "target-not-in-concept".to_owned(),
                contract_version: "1".to_owned(),
                after: None,
                limit: 10,
            })?
            .items
            .len()
            != 3
    {
        return Err("as-of CLI did not distinguish current, stale, and unavailable results".into());
    }
    Ok(())
}

#[test]
fn file_and_turso_cli_catch_up_uses_retained_contracts_and_bounds_work()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let file_canonical: PathBuf = directory.path().join("contract-catch-up.snapshot");
    let file_downstream = directory.path().join("contract-catch-up.sqlite");
    prepare_and_catch_up(
        FileGraphStore::open(&file_canonical)?,
        &file_canonical,
        &file_downstream,
        "inference-catch-up",
    )?;

    let turso_canonical = directory.path().join("contract-catch-up.db");
    let turso_downstream = directory.path().join("contract-catch-up-turso.sqlite");
    TursoGraphStore::migrate(&turso_canonical)?;
    prepare_and_catch_up(
        TursoGraphStore::open(&turso_canonical)?,
        &turso_canonical,
        &turso_downstream,
        "inference-catch-up-turso",
    )
}
