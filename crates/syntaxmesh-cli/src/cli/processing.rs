use super::{CliError, file_engine, parse_generation_id, turso_engine, write_stdout};
use std::path::Path;
use syntaxmesh_core::GenerationId;
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

pub(super) fn run(arguments: impl Iterator<Item = String>, turso: bool) -> Result<(), CliError> {
    let arguments = arguments.collect::<Vec<_>>();
    let [store, generation] = arguments.as_slice() else {
        return Err(CliError::Usage(
            "processing-coverage[-turso] requires <store> <current|generation-id>".to_owned(),
        ));
    };
    let selected = if generation == "current" {
        None
    } else {
        Some(parse_generation_id(generation)?)
    };
    if turso {
        let lease = syntaxmesh_ownership_host::WriterLease::acquire(Path::new(store))?;
        let (engine, current) = turso_engine(&lease)?;
        report(&engine, selected.unwrap_or(current))
    } else {
        let (engine, current) = file_engine(Path::new(store))?;
        report(&engine, selected.unwrap_or(current))
    }
}

fn report<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
) -> Result<(), CliError> {
    let coverage = engine.source_processing_coverage_at(generation)?;
    let completed = coverage
        .completed
        .iter()
        .map(|id| id.0.to_hex())
        .collect::<Vec<_>>();
    let unclassified = coverage
        .unclassified
        .iter()
        .map(|id| id.0.to_hex())
        .collect::<Vec<_>>();
    let syntax_failed = coverage.syntax_failed.iter().map(|(id, diagnostic)| serde_json::json!({
        "file_id": id.0.to_hex(), "message": diagnostic.message(), "truncated": diagnostic.truncated()
    })).collect::<Vec<_>>();
    write_stdout(
        &serde_json::to_string(&serde_json::json!({"schema_version": 1,
            "generation": coverage.generation.0.to_hex(), "completed": completed,
            "syntax_failed": syntax_failed, "unclassified": unclassified
        }))
        .map_err(CliError::Encode)?,
    )
}
