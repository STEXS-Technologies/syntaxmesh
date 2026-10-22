use super::{invoke, text};
use std::path::Path;
use std::result::Result;
use std::{error::Error, fs};
use syntaxmesh_core::{GenerationId, NodeId};
use syntaxmesh_inference::{Limits, Rule};
use syntaxmesh_inference_sqlite::{ContractHistoryQuery, InferenceMaterializer};
use syntaxmesh_query::Query;
use syntaxmesh_store::{FileGraphStore, GraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

#[path = "../../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
mod fixture;

fn write_contracts(path: &Path, version: &str, expectation: &str) -> Result<(), Box<dyn Error>> {
    let configuration = serde_json::json!({
        "version": 1,
        "contracts": [{
            "id": "target-not-in-concept",
            "version": version,
            "origin": "declared",
            "source": NodeId::derive(&[b"target"]).0.to_hex(),
            "target": NodeId::derive(&[b"concept"]).0.to_hex(),
            "relation": {"kind": "external", "namespace": "example", "relation": "instance_of"},
            "expectation": expectation,
        }],
        "limits": {"contracts": 4, "input_bytes": 16384, "edge_visits": 100, "results": 4},
        "history_scan_budget": 100,
        "output_bytes": 16384,
    });
    fs::write(path, serde_json::to_vec(&configuration)?)?;
    Ok(())
}

fn prepare<S: GraphStore>(
    mut store: S,
) -> Result<(GenerationId, GenerationId, GenerationId), Box<dyn Error>> {
    let (rule, declared, retracted) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, declared, retracted)?;
    fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
    let first = store
        .manifest(declared)?
        .parent
        .ok_or("fixture has no first generation")?;
    Ok((first, declared, retracted))
}

fn run_authoring_lifecycle(
    canonical: &Path,
    downstream: &Path,
    command_suffix: &str,
    first: GenerationId,
    declared: GenerationId,
    retracted: GenerationId,
    directory: &Path,
) -> Result<(), Box<dyn Error>> {
    let configuration_v1 = directory.join(format!("contracts-v1-{command_suffix}.json"));
    let conflicting_v1 = directory.join(format!("contracts-conflict-{command_suffix}.json"));
    let configuration_v2 = directory.join(format!("contracts-v2-{command_suffix}.json"));
    let stale_v3 = directory.join(format!("contracts-stale-{command_suffix}.json"));
    write_contracts(&configuration_v1, "1", "forbidden")?;
    write_contracts(&conflicting_v1, "1", "required")?;
    write_contracts(&configuration_v2, "2", "required")?;
    write_contracts(&stale_v3, "3", "forbidden")?;
    InferenceMaterializer::migrate(downstream)?;
    let author = format!("inference-retain-contracts{command_suffix}");
    let catch_up = format!("inference-catch-up{command_suffix}");
    let first_id = first.0.to_hex();
    let declared_id = declared.0.to_hex();
    let retracted_id = retracted.0.to_hex();
    let args = |generation: &str, config: &Path| -> Result<Vec<String>, Box<dyn Error>> {
        Ok(vec![
            author.clone(),
            text(canonical)?.to_owned(),
            text(downstream)?.to_owned(),
            generation.to_owned(),
            text(config)?.to_owned(),
        ])
    };
    let initial = invoke(
        &args(&first_id, &configuration_v1)?
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    if !initial.status.success() {
        return Err(String::from_utf8_lossy(&initial.stderr).into_owned().into());
    }
    let initial_receipt: serde_json::Value = serde_json::from_slice(&initial.stdout)?;
    if initial_receipt
        .get("contract_count")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || initial_receipt
            .get("evaluations")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            != Some(1)
    {
        return Err("initial CLI contract authoring did not return its retained evaluation".into());
    }
    let retry = invoke(
        &args(&first_id, &configuration_v1)?
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    if !retry.status.success() {
        return Err(String::from_utf8_lossy(&retry.stderr).into_owned().into());
    }
    let retry_receipt: serde_json::Value = serde_json::from_slice(&retry.stdout)?;
    if retry_receipt.get("operation") != initial_receipt.get("operation") {
        return Err("identical same-generation contract authoring was not idempotent".into());
    }
    let conflicting = invoke(
        &args(&first_id, &conflicting_v1)?
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    if conflicting.status.success()
        || InferenceMaterializer::open(downstream)?
            .latest_contract_set(
                syntaxmesh_core::RepositoryId::derive(&[b"inference-repo"]),
                syntaxmesh_core::WorktreeId::derive(&[b"inference-worktree"]),
            )?
            .is_none_or(|set| {
                set.contracts.first().is_none_or(|contract| {
                    contract.version != "1"
                        || contract.expectation
                            != syntaxmesh_inference::RelationExpectation::Forbidden
                })
            })
    {
        return Err("same-generation conflicting contract set changed the authoring head".into());
    }
    let replacement = invoke(
        &args(&declared_id, &configuration_v2)?
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    if !replacement.status.success() {
        return Err(String::from_utf8_lossy(&replacement.stderr)
            .into_owned()
            .into());
    }
    let backwards = invoke(
        &args(&first_id, &stale_v3)?
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    if backwards.status.success() {
        return Err("older-generation contract set replacement was accepted".into());
    }
    let caught_up = invoke(&[
        catch_up.as_str(),
        text(canonical)?,
        text(downstream)?,
        &retracted_id,
        "2",
        "100",
        "16384",
        "100",
        "16384",
    ])?;
    if !caught_up.status.success() {
        return Err(String::from_utf8_lossy(&caught_up.stderr)
            .into_owned()
            .into());
    }
    let materializer = InferenceMaterializer::open(downstream)?;
    let repository = syntaxmesh_core::RepositoryId::derive(&[b"inference-repo"]);
    let worktree = syntaxmesh_core::WorktreeId::derive(&[b"inference-worktree"]);
    let old_history = materializer.contract_history(&ContractHistoryQuery {
        repository,
        worktree,
        contract_id: "target-not-in-concept".to_owned(),
        contract_version: "1".to_owned(),
        after: None,
        limit: 10,
    })?;
    let new_history = materializer.contract_history(&ContractHistoryQuery {
        repository,
        worktree,
        contract_id: "target-not-in-concept".to_owned(),
        contract_version: "2".to_owned(),
        after: None,
        limit: 10,
    })?;
    if old_history.items.len() != 1
        || old_history
            .items
            .first()
            .map(|item| item.evaluation.generation)
            != Some(first)
        || new_history.items.len() != 2
        || new_history
            .items
            .first()
            .map(|item| item.evaluation.generation)
            != Some(declared)
        || new_history
            .items
            .get(1)
            .map(|item| item.evaluation.generation)
            != Some(retracted)
        || new_history.items.get(1).map(|item| item.evaluation.outcome)
            != Some(syntaxmesh_inference::ContractOutcome::InsufficientEvidence)
    {
        return Err(
            "later authored contract set did not replace only the catch-up baseline".into(),
        );
    }
    Ok(())
}

#[test]
fn file_and_turso_cli_author_contract_sets_with_versioned_history() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let file_store_path = directory.path().join("authored-contracts.snapshot");
    let file_generations = prepare(FileGraphStore::open(&file_store_path)?)?;
    run_authoring_lifecycle(
        &file_store_path,
        &directory.path().join("authored-contracts.sqlite"),
        "",
        file_generations.0,
        file_generations.1,
        file_generations.2,
        directory.path(),
    )?;

    let turso_store_path = directory.path().join("authored-contracts.db");
    TursoGraphStore::migrate(&turso_store_path)?;
    let turso_generations = prepare(TursoGraphStore::open(&turso_store_path)?)?;
    run_authoring_lifecycle(
        &turso_store_path,
        &directory.path().join("authored-contracts-turso.sqlite"),
        "-turso",
        turso_generations.0,
        turso_generations.1,
        turso_generations.2,
        directory.path(),
    )
}
