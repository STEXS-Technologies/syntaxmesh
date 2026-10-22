use super::{CliError, write_stdout};
use std::io::Read;
use std::path::Path;
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference_sqlite::{
    ContractHistoryCursor, ContractHistoryQuery, InferenceMaterializer,
};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::FileGraphStore;
use syntaxmesh_store_turso::TursoGraphStore;

pub(in crate::cli) fn contract_history(
    turso: bool,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
        "inference-contract-history[-turso] <canonical-store> <inference.sqlite> <contract-id> <contract-version> <limit> [cursor.json]".to_owned(),
    )
    };
    let canonical = arguments.next().ok_or_else(usage)?;
    let database = arguments.next().ok_or_else(usage)?;
    let contract_id = arguments.next().ok_or_else(usage)?;
    let contract_version = arguments.next().ok_or_else(usage)?;
    let limit = arguments
        .next()
        .ok_or_else(usage)?
        .parse::<usize>()
        .map_err(|error| CliError::Usage(format!("invalid page limit: {error}")))?;
    let cursor_path = arguments.next();
    if arguments.next().is_some() || contract_id.is_empty() || contract_version.is_empty() {
        return Err(usage());
    }
    let canonical = Path::new(&canonical);
    let database = Path::new(&database);
    if !canonical.is_file() || !database.is_file() {
        return Err(CliError::Usage(
            "contract history requires existing canonical and migrated inference stores".to_owned(),
        ));
    }
    let after = cursor_path
        .map(|path| {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(16 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 16 * 1024 {
                return Err(CliError::Usage(
                    "contract history cursor exceeds 16 KiB".to_owned(),
                ));
            }
            serde_json::from_slice::<ContractHistoryCursor>(&bytes).map_err(CliError::Encode)
        })
        .transpose()?;
    drop(
        InferenceMaterializer::open(database)
            .map_err(|error| CliError::Integrity(error.to_string()))?,
    );

    let page = if turso {
        let store = TursoGraphStore::open(canonical)?;
        let manifest = store.latest_generation().ok_or_else(|| {
            CliError::Usage("Turso database has no published generation".to_owned())
        })?;
        let engine =
            SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
        engine.contract_history(
            database,
            &ContractHistoryQuery {
                repository: manifest.repository,
                worktree: manifest.worktree,
                contract_id,
                contract_version,
                after,
                limit,
            },
        )?
    } else {
        let store = FileGraphStore::open(canonical)?;
        let manifest = store
            .latest_generation()
            .ok_or_else(|| CliError::Usage("snapshot has no published generation".to_owned()))?;
        let engine =
            SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
        engine.contract_history(
            database,
            &ContractHistoryQuery {
                repository: manifest.repository,
                worktree: manifest.worktree,
                contract_id,
                contract_version,
                after,
                limit,
            },
        )?
    };
    let output = serde_json::json!({
        "version": 1,
        "temporal_claim": "retained_evaluation_generations_only",
        "items": page.items,
        "next_cursor": page.next_cursor,
    });
    write_stdout(&serde_json::to_string(&output).map_err(CliError::Encode)?)
}
