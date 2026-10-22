use super::{CliError, WriterLease, file_engine, turso_engine, write_stdout};
use crate::cli::{parse_generation_id, parse_limit};
use std::path::Path;
use syntaxmesh_engine::ContractHistoryAdvanceRequest;
use syntaxmesh_inference::ContractLimits;
use syntaxmesh_inference_sqlite::InferenceMaterializer;

pub(in crate::cli) fn catch_up(
    turso: bool,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "inference-catch-up[-turso] <canonical-store> <inference.sqlite> <through-generation> <generation-budget> <history-scan-budget> <input-bytes> <edge-visits> <output-bytes-per-generation>".to_owned(),
        )
    };
    let canonical = arguments.next().ok_or_else(usage)?;
    let database = arguments.next().ok_or_else(usage)?;
    let through = arguments.next().ok_or_else(usage)?;
    let generation_budget = parse_limit(&arguments.next().ok_or_else(usage)?)?;
    let history_scan_budget = parse_limit(&arguments.next().ok_or_else(usage)?)?;
    let input_bytes = parse_limit(&arguments.next().ok_or_else(usage)?)?;
    let edge_visits = parse_limit(&arguments.next().ok_or_else(usage)?)?;
    let output_bytes_per_generation = parse_limit(&arguments.next().ok_or_else(usage)?)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let through_generation = parse_generation_id(&through)?;
    if [
        generation_budget,
        history_scan_budget,
        input_bytes,
        edge_visits,
        output_bytes_per_generation,
    ]
    .contains(&0)
    {
        return Err(CliError::Usage(
            "positive contract catch-up budgets required".to_owned(),
        ));
    }
    if !Path::new(&canonical).is_file() || !Path::new(&database).is_file() {
        return Err(CliError::Usage(
            "contract catch-up requires existing canonical and migrated inference stores"
                .to_owned(),
        ));
    }
    let canonical_lease = WriterLease::acquire(Path::new(&canonical))?;
    let downstream_lease = WriterLease::acquire(Path::new(&database))?;
    drop(
        InferenceMaterializer::open(downstream_lease.target())
            .map_err(|error| CliError::Integrity(error.to_string()))?,
    );
    let request = ContractHistoryAdvanceRequest {
        through_generation,
        generation_budget,
        history_scan_budget,
        output_bytes_per_generation,
        limits: ContractLimits {
            contracts: 1_000,
            input_bytes,
            edge_visits,
            results: 1_000,
        },
    };
    let receipt = if turso {
        let (mut engine, _) = turso_engine(&canonical_lease)?;
        engine.advance_contract_history(downstream_lease.target(), request)?
    } else {
        let (mut engine, _) = file_engine(canonical_lease.target())?;
        engine.advance_contract_history(downstream_lease.target(), request)?
    };
    let output = serde_json::json!({
        "version": 1,
        "starting_generation": receipt.starting_generation.0.to_hex(),
        "through_generation": receipt.through_generation.0.to_hex(),
        "starting_sequence": receipt.starting_sequence,
        "through_sequence": receipt.through_sequence,
        "generations_advanced": receipt.generations_advanced,
        "contract_count": receipt.contract_count,
        "last_operation": receipt.last_watermark.operation.to_hex(),
        "last_generation": receipt.last_watermark.generation.0.to_hex(),
    });
    write_stdout(&serde_json::to_string(&output).map_err(CliError::Encode)?)
}
