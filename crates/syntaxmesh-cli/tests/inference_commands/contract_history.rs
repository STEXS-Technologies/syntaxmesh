use super::{invoke, text};
use std::path::Path;
use std::result::Result;
use std::{error::Error, path::PathBuf};
use syntaxmesh_core::{NodeId, RelationKind};
use syntaxmesh_inference::{
    Contract, ContractLimits, ContractOrigin, Limits, RelationExpectation, Rule,
};
use syntaxmesh_inference_sqlite::{ContractHistoryRequest, InferenceMaterializer};
use syntaxmesh_query::Query;
use syntaxmesh_store::{FileGraphStore, GraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

#[path = "../../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
mod fixture;

fn prepare_and_catch_up<S: GraphStore>(
    mut store: S,
    canonical: &Path,
    downstream: &Path,
    command: &str,
) -> Result<(), Box<dyn Error>> {
    let (rule, declared, retracted) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, declared, retracted)?;
    fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
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
