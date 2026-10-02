use std::path::Path;

use syntaxmesh_core::{AcceptanceTime, GenerationId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use super::{CliError, file_engine, turso_engine, write_stdout};

pub(super) fn graph_at(snapshot: &Path, generation: GenerationId) -> Result<(), CliError> {
    let (engine, current) = file_engine(snapshot)?;
    graph_at_engine(&engine, current, generation)
}

pub(super) fn graph_at_turso(database: &Path, generation: GenerationId) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, current) = turso_engine(&ownership)?;
    graph_at_engine(&engine, current, generation)
}

fn graph_at_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    current: GenerationId,
    generation: GenerationId,
) -> Result<(), CliError> {
    for record in engine.query(current).export_graph_at(generation)? {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn graph_at_known_by(
    snapshot: &Path,
    generation: GenerationId,
    accepted_by: AcceptanceTime,
) -> Result<(), CliError> {
    let (engine, current) = file_engine(snapshot)?;
    graph_at_known_by_engine(&engine, current, generation, accepted_by)
}

pub(super) fn graph_at_known_by_turso(
    database: &Path,
    generation: GenerationId,
    accepted_by: AcceptanceTime,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, current) = turso_engine(&ownership)?;
    graph_at_known_by_engine(&engine, current, generation, accepted_by)
}

fn graph_at_known_by_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    current: GenerationId,
    generation: GenerationId,
    accepted_by: AcceptanceTime,
) -> Result<(), CliError> {
    for record in engine
        .query(current)
        .export_graph_at_known_by(generation, accepted_by)?
    {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn changes(
    snapshot: &Path,
    from: GenerationId,
    to: GenerationId,
) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    changes_engine(&engine, generation, from, to)
}

pub(super) fn changes_turso(
    database: &Path,
    from: GenerationId,
    to: GenerationId,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    changes_engine(&engine, generation, from, to)
}

fn changes_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    from: GenerationId,
    to: GenerationId,
) -> Result<(), CliError> {
    for change in engine.query(generation).changed_between(from, to)? {
        write_stdout(&change.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}
