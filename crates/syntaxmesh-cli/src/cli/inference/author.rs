use super::{
    CliError, InferenceMaterializer, WriterLease, file_engine, turso_engine, write_stdout,
};
use crate::cli::{contract_config, parse_generation_id};
use std::path::Path;
use syntaxmesh_engine::ContractHistoryRetainRequest;
use syntaxmesh_inference_sqlite::RetainedContractEvaluation;

pub(in crate::cli) fn retain_contracts(
    turso: bool,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "inference-retain-contracts[-turso] <canonical-store> <inference.sqlite> <generation> <contracts.json>".to_owned(),
        )
    };
    let canonical = arguments.next().ok_or_else(usage)?;
    let database = arguments.next().ok_or_else(usage)?;
    let generation = parse_generation_id(&arguments.next().ok_or_else(usage)?)?;
    let config_path = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let canonical = Path::new(&canonical);
    let database = Path::new(&database);
    if !canonical.is_file() || !database.is_file() {
        return Err(CliError::Usage(
            "contract retention requires existing canonical and migrated inference stores"
                .to_owned(),
        ));
    }
    let configuration = contract_config::load(Path::new(&config_path))?;
    let contracts = configuration.contracts()?;
    let limits = configuration.limits();
    let request = ContractHistoryRetainRequest {
        generation,
        contracts: &contracts,
        limits: &limits,
        history_scan_budget: configuration.history_scan_budget(),
        output_bytes: configuration.output_bytes(),
    };
    let canonical_lease = WriterLease::acquire(canonical)?;
    let downstream_lease = WriterLease::acquire(database)?;
    drop(
        InferenceMaterializer::open(downstream_lease.target())
            .map_err(|error| CliError::Integrity(error.to_string()))?,
    );
    let receipt = if turso {
        let (mut engine, _) = turso_engine(&canonical_lease)?;
        engine.retain_contract_history(downstream_lease.target(), &request)?
    } else {
        let (mut engine, _) = file_engine(canonical_lease.target())?;
        engine.retain_contract_history(downstream_lease.target(), &request)?
    };
    let materializer = InferenceMaterializer::open(downstream_lease.target())
        .map_err(|error| CliError::Integrity(error.to_string()))?;
    let evaluations = contracts
        .iter()
        .map(|contract| {
            materializer
                .contract_result_at(
                    receipt.repository,
                    receipt.worktree,
                    generation,
                    &contract.id,
                    &contract.version,
                )
                .map_err(|error| CliError::Integrity(error.to_string()))?
                .ok_or_else(|| {
                    CliError::Integrity(
                        "retained contract evaluation missing after successful publication"
                            .to_owned(),
                    )
                })
        })
        .collect::<Result<Vec<RetainedContractEvaluation>, CliError>>()?;
    let output = serde_json::json!({
        "version": 1,
        "generation": receipt.generation.0.to_hex(),
        "generation_sequence": receipt.generation_sequence,
        "operation": receipt.operation.to_hex(),
        "contract_count": receipt.result_count,
        "evaluations": evaluations,
    });
    write_stdout(&serde_json::to_string(&output).map_err(CliError::Encode)?)
}
