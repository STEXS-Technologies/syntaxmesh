use std::path::Path;

use syntaxmesh_core::{FactRef, FactVersionRef, GenerationId, NodeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use super::{CliError, file_engine, turso_engine, write_stdout};

pub(super) fn history(snapshot: &Path, id: NodeId) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    history_engine(&engine, generation, id)
}

pub(super) fn history_turso(database: &Path, id: NodeId) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    history_engine(&engine, generation, id)
}

fn history_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    id: NodeId,
) -> Result<(), CliError> {
    for version in engine.query(generation).history(id)? {
        write_stdout(&version.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn fact_history(snapshot: &Path, fact: FactRef) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    fact_history_engine(&engine, generation, fact)
}

pub(super) fn fact_history_turso(database: &Path, fact: FactRef) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    fact_history_engine(&engine, generation, fact)
}

fn fact_history_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    fact: FactRef,
) -> Result<(), CliError> {
    for version in engine.query(generation).fact_history(fact)? {
        write_stdout(&version.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn fact_lineage(snapshot: &Path, fact: FactRef) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    fact_lineage_engine(&engine, generation, fact)
}

pub(super) fn fact_lineage_turso(database: &Path, fact: FactRef) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    fact_lineage_engine(&engine, generation, fact)
}

fn fact_lineage_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    fact: FactRef,
) -> Result<(), CliError> {
    for link in engine.query(generation).fact_lineage(fact)? {
        write_stdout(&link.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn fact_version_origin(
    snapshot: &Path,
    version: FactVersionRef,
) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    fact_version_origin_engine(&engine, generation, version)
}

pub(super) fn fact_version_origin_turso(
    database: &Path,
    version: FactVersionRef,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    fact_version_origin_engine(&engine, generation, version)
}

pub(super) fn fact_version_retirement(
    snapshot: &Path,
    version: FactVersionRef,
) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    fact_version_retirement_engine(&engine, generation, version)
}

pub(super) fn fact_version_retirement_turso(
    database: &Path,
    version: FactVersionRef,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    fact_version_retirement_engine(&engine, generation, version)
}

fn fact_version_retirement_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    version: FactVersionRef,
) -> Result<(), CliError> {
    let record = engine
        .query(generation)
        .fact_version_retirement(version)?
        .ok_or_else(|| {
            CliError::Usage(format!(
                "no retained retirement event for fact version {version:?}"
            ))
        })?;
    write_stdout(&record.to_json_line().map_err(CliError::Encode)?)
}

fn fact_version_origin_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    version: FactVersionRef,
) -> Result<(), CliError> {
    let record = engine
        .query(generation)
        .fact_version_introduction(version)?
        .ok_or_else(|| {
            CliError::Usage(format!(
                "no retained introduction event for fact version {version:?}"
            ))
        })?;
    write_stdout(&record.to_json_line().map_err(CliError::Encode)?)
}
