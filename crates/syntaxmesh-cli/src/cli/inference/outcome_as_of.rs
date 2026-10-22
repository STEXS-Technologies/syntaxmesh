use super::{CliError, file_engine, write_stdout};
use crate::cli::parse_generation_id;
use std::path::Path;
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference_sqlite::InferenceMaterializer;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store_turso::TursoGraphStore;

pub(in crate::cli) fn outcome_as_of(
    turso: bool,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "inference-outcome-as-of[-turso] <canonical-store> <inference.sqlite> <generation> <contract-id> <contract-version>".to_owned(),
        )
    };
    let canonical = arguments.next().ok_or_else(usage)?;
    let database = arguments.next().ok_or_else(usage)?;
    let generation = parse_generation_id(&arguments.next().ok_or_else(usage)?)?;
    let contract_id = arguments.next().ok_or_else(usage)?;
    let contract_version = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() || contract_id.is_empty() || contract_version.is_empty() {
        return Err(usage());
    }
    let canonical = Path::new(&canonical);
    let database = Path::new(&database);
    if !canonical.is_file() || !database.is_file() {
        return Err(CliError::Usage(
            "contract outcome lookup requires existing canonical and migrated inference stores"
                .to_owned(),
        ));
    }
    drop(
        InferenceMaterializer::open(database)
            .map_err(|error| CliError::Integrity(error.to_string()))?,
    );
    let result = if turso {
        let store = TursoGraphStore::open(canonical)?;
        let manifest = store.latest_generation().ok_or_else(|| {
            CliError::Usage("Turso database has no published generation".to_owned())
        })?;
        let engine =
            SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
        engine.contract_outcome_as_of(database, generation, &contract_id, &contract_version)?
    } else {
        let (engine, _) = file_engine(canonical)?;
        engine.contract_outcome_as_of(database, generation, &contract_id, &contract_version)?
    };
    let freshness = result
        .evaluation
        .as_ref()
        .map_or("unavailable", |evaluation| {
            if evaluation.evaluation.generation == result.target_generation {
                "current"
            } else {
                "stale"
            }
        });
    let output = serde_json::json!({
        "version": 1,
        "target_generation": result.target_generation.0.to_hex(),
        "target_sequence": result.target_sequence,
        "freshness": freshness,
        "evaluation_generation": result.evaluation.as_ref().map(|item| item.evaluation.generation.0.to_hex()),
        "evaluation_sequence": result.evaluation.as_ref().map(|item| item.generation_sequence),
        "evaluation": result.evaluation,
        "outcome_episode": result.episode,
        "temporal_claim": "observed_evaluation_generations_only",
    });
    write_stdout(&serde_json::to_string(&output).map_err(CliError::Encode)?)
}
