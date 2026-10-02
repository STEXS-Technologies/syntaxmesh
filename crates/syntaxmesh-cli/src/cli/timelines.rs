use std::path::Path;

use syntaxmesh_core::{
    AcceptanceTime, AcceptedGenerationCursor, ChangeEventCorrelationCursor, FactRef, GenerationId,
    ObservationTime, ObservedFactCursor,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use super::{CliError, file_engine, turso_engine, write_stdout};

pub(super) fn change_correlations(
    snapshot: &Path,
    source_generation: GenerationId,
    fact: FactRef,
    after: Option<GenerationId>,
    limit: usize,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    change_correlations_engine(&engine, source_generation, fact, after, limit)
}

pub(super) fn change_correlations_turso(
    database: &Path,
    source_generation: GenerationId,
    fact: FactRef,
    after: Option<GenerationId>,
    limit: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    change_correlations_engine(&engine, source_generation, fact, after, limit)
}

fn change_correlations_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    source_generation: GenerationId,
    fact: FactRef,
    after: Option<GenerationId>,
    limit: usize,
) -> Result<(), CliError> {
    let query = engine.query(source_generation);
    let source_event = match query.change_event()? {
        Some(syntaxmesh_api_model::TemporalRecord::ChangeEvent { event, .. }) => event.id,
        _ => return Err(CliError::Query(super::QueryError::InvalidCorrelationAnchor)),
    };
    let cursor = after.map(|after_generation| ChangeEventCorrelationCursor {
        source_event,
        after_generation,
    });
    for record in query.change_event_correlations(fact, cursor, limit)? {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn observed_between(
    snapshot: &Path,
    from: ObservationTime,
    until: ObservationTime,
    after: Option<ObservedFactCursor>,
    limit: usize,
) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    observed_between_engine(&engine, generation, from, until, after, limit)
}

pub(super) fn observed_between_turso(
    database: &Path,
    from: ObservationTime,
    until: ObservationTime,
    after: Option<ObservedFactCursor>,
    limit: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    observed_between_engine(&engine, generation, from, until, after, limit)
}

fn observed_between_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    from: ObservationTime,
    until: ObservationTime,
    after: Option<ObservedFactCursor>,
    limit: usize,
) -> Result<(), CliError> {
    for record in engine
        .query(generation)
        .observed_between(from, until, after, limit)?
    {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn accepted_between(
    snapshot: &Path,
    from: AcceptanceTime,
    until: AcceptanceTime,
    after: Option<AcceptedGenerationCursor>,
    limit: usize,
) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    accepted_between_engine(&engine, generation, from, until, after, limit)
}

pub(super) fn accepted_between_turso(
    database: &Path,
    from: AcceptanceTime,
    until: AcceptanceTime,
    after: Option<AcceptedGenerationCursor>,
    limit: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    accepted_between_engine(&engine, generation, from, until, after, limit)
}

fn accepted_between_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    from: AcceptanceTime,
    until: AcceptanceTime,
    after: Option<AcceptedGenerationCursor>,
    limit: usize,
) -> Result<(), CliError> {
    for record in engine
        .query(generation)
        .accepted_between(from, until, after, limit)?
    {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}
