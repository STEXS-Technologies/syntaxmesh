use super::{CliError, file_engine, write_stdout};
use crate::cli::{contract_config, parse_generation_id};
use std::path::Path;
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference::{ContractEvaluation, ContractOutcome};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store_turso::TursoGraphStore;

pub(in crate::cli) fn check_contracts(
    turso: bool,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "inference-contract-check[-turso] <canonical-store> <generation> <contracts.json> --fail-on <outcome[,outcome...]>".to_owned(),
        )
    };
    let canonical = arguments.next().ok_or_else(usage)?;
    let generation = parse_generation_id(&arguments.next().ok_or_else(usage)?)?;
    let config_path = arguments.next().ok_or_else(usage)?;
    if arguments.next().as_deref() != Some("--fail-on") {
        return Err(usage());
    }
    let fail_on_arg = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let fail_on = parse_fail_on(&fail_on_arg)?;
    let canonical = Path::new(&canonical);
    if !canonical.is_file() {
        return Err(CliError::Usage(
            "contract check requires an existing canonical store".to_owned(),
        ));
    }
    let configuration = contract_config::load(Path::new(&config_path))?;
    let contracts = configuration.contracts()?;
    let limits = configuration.limits();
    let evaluations = if turso {
        let store = TursoGraphStore::open(canonical)?;
        let manifest = store.latest_generation().ok_or_else(|| {
            CliError::Usage("Turso database has no published generation".to_owned())
        })?;
        let engine =
            SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
        engine.query(generation).evaluate_contracts(
            &contracts,
            &limits,
            configuration.history_scan_budget(),
        )?
    } else {
        let (engine, _) = file_engine(canonical)?;
        engine.query(generation).evaluate_contracts(
            &contracts,
            &limits,
            configuration.history_scan_budget(),
        )?
    };
    let passed = !evaluations
        .iter()
        .any(|evaluation| fail_on.contains(&evaluation.outcome));
    let result = policy_result(generation, &fail_on, &evaluations, passed);
    let encoded = serde_json::to_string(&result).map_err(CliError::Encode)?;
    if encoded.len() > configuration.output_bytes() {
        return Err(CliError::Usage(
            "contract check output exceeds configured output_bytes".to_owned(),
        ));
    }
    write_stdout(&encoded)?;
    if passed {
        Ok(())
    } else {
        Err(CliError::ContractPolicyFailed)
    }
}

fn parse_fail_on(value: &str) -> Result<Vec<ContractOutcome>, CliError> {
    let outcomes = value
        .split(',')
        .map(|item| match item {
            "Supported" => Ok(ContractOutcome::Supported),
            "Contradicted" => Ok(ContractOutcome::Contradicted),
            "InsufficientEvidence" => Ok(ContractOutcome::InsufficientEvidence),
            _ => Err(CliError::Usage(format!(
                "unknown contract outcome {item:?}; expected Supported, Contradicted, or InsufficientEvidence"
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if outcomes.is_empty() {
        return Err(CliError::Usage(
            "--fail-on requires at least one contract outcome".to_owned(),
        ));
    }
    let mut unique = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        if unique.contains(&outcome) {
            return Err(CliError::Usage(
                "--fail-on outcomes must not be repeated".to_owned(),
            ));
        }
        unique.push(outcome);
    }
    Ok(unique)
}

fn policy_result(
    generation: syntaxmesh_core::GenerationId,
    fail_on: &[ContractOutcome],
    evaluations: &[ContractEvaluation],
    passed: bool,
) -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "generation": generation.0.to_hex(),
        "fail_on": fail_on,
        "passed": passed,
        "evaluations": evaluations,
    })
}
