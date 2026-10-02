use std::path::Path;

use syntaxmesh_api_model::HistoricalNeighborCursor;
use syntaxmesh_core::{EdgeDirection, GenerationId, LineageEndpoint, NodeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_query::ConsequenceTraceRequest;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use super::{CliError, file_engine, turso_engine, write_stdout};

pub(super) fn node(snapshot: &Path, id: NodeId) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    node_engine(&engine, generation, id)
}

pub(super) fn node_turso(database: &Path, id: NodeId) -> Result<(), CliError> {
    if let Some(endpoint) = syntaxmesh_ownership_host::OwnerEndpoint::discover(database)? {
        return write_node(&super::attachment::node(&endpoint, id, None)?, None);
    }
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    node_engine(&engine, generation, id)
}

pub(super) fn node_at(
    snapshot: &Path,
    generation: GenerationId,
    id: NodeId,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    node_at_engine(&engine, generation, id)
}

pub(super) fn node_at_turso(
    database: &Path,
    generation: GenerationId,
    id: NodeId,
) -> Result<(), CliError> {
    if let Some(endpoint) = syntaxmesh_ownership_host::OwnerEndpoint::discover(database)? {
        return write_node(
            &super::attachment::node(&endpoint, id, Some(generation))?,
            Some(generation),
        );
    }
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    node_at_engine(&engine, generation, id)
}

fn node_at_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    id: NodeId,
) -> Result<(), CliError> {
    let node = engine
        .query(generation)
        .historical_node(id)?
        .ok_or_else(|| {
            CliError::Usage(format!(
                "node {} was not present in generation {}",
                id.0.to_hex(),
                generation.0.to_hex()
            ))
        })?;
    write_node(&node, Some(generation))
}

pub(super) fn node_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    id: NodeId,
) -> Result<(), CliError> {
    let node = engine
        .query(generation)
        .node(id)?
        .ok_or_else(|| CliError::Usage(format!("node {} was not found", id.0.to_hex())))?;
    write_node(&node, None)
}

fn write_node(
    node: &syntaxmesh_core::Node,
    generation: Option<GenerationId>,
) -> Result<(), CliError> {
    let prefix = generation
        .map(|id| format!("generation={}\t", id.0.to_hex()))
        .unwrap_or_default();
    write_stdout(&format!(
        "{prefix}{}\t{:?}\t{}\tsource={:?}\tprovenance={}",
        node.name,
        node.kind,
        node.id.0.to_hex(),
        node.source,
        node.provenance.0.to_hex()
    ))
}

pub(super) fn neighbors(snapshot: &Path, id: NodeId) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    neighbors_engine(&engine, generation, id)
}

pub(super) fn neighbors_turso(database: &Path, id: NodeId) -> Result<(), CliError> {
    if let Some(endpoint) = syntaxmesh_ownership_host::OwnerEndpoint::discover(database)? {
        return write_neighbors(super::attachment::neighbors(&endpoint, id)?);
    }
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    neighbors_engine(&engine, generation, id)
}

pub(super) fn historical_neighbors(
    snapshot: &Path,
    generation: GenerationId,
    endpoint: NodeId,
    direction: EdgeDirection,
    limit: usize,
    after_edge: Option<syntaxmesh_core::EdgeId>,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    historical_neighbors_engine(&engine, generation, endpoint, direction, limit, after_edge)
}

pub(super) fn historical_neighbors_turso(
    database: &Path,
    generation: GenerationId,
    endpoint: NodeId,
    direction: EdgeDirection,
    limit: usize,
    after_edge: Option<syntaxmesh_core::EdgeId>,
) -> Result<(), CliError> {
    if let Some(owner) = syntaxmesh_ownership_host::OwnerEndpoint::discover(database)? {
        let records = super::attachment::historical_neighbors(
            &owner, generation, endpoint, direction, limit, after_edge,
        )?
        .into_records();
        return write_historical_neighbor_records(records);
    }
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    historical_neighbors_engine(&engine, generation, endpoint, direction, limit, after_edge)
}

#[derive(Clone, Copy)]
pub(super) struct HistoricalNeighborhoodLimits {
    pub max_hops: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_scanned_edges: usize,
    pub max_result_bytes: usize,
}

pub(super) fn historical_neighborhood(
    snapshot: &Path,
    generation: GenerationId,
    seeds: &[NodeId],
    limits: HistoricalNeighborhoodLimits,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    historical_neighborhood_engine(&engine, generation, seeds, limits)
}

pub(super) fn historical_neighborhood_turso(
    database: &Path,
    generation: GenerationId,
    seeds: &[NodeId],
    limits: HistoricalNeighborhoodLimits,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    historical_neighborhood_engine(&engine, generation, seeds, limits)
}

fn historical_neighborhood_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    seeds: &[NodeId],
    limits: HistoricalNeighborhoodLimits,
) -> Result<(), CliError> {
    for record in engine.query(generation).historical_neighborhood_records(
        seeds,
        limits.max_hops,
        limits.max_nodes,
        limits.max_edges,
        limits.max_scanned_edges,
        limits.max_result_bytes,
    )? {
        write_stdout(&record.to_json_line().map_err(|error| {
            CliError::Usage(format!("serialize historical-neighborhood record: {error}"))
        })?)?;
    }
    Ok(())
}

fn historical_neighbors_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    endpoint: NodeId,
    direction: EdgeDirection,
    limit: usize,
    after_edge: Option<syntaxmesh_core::EdgeId>,
) -> Result<(), CliError> {
    let after = after_edge.map(|after_edge| HistoricalNeighborCursor {
        generation,
        endpoint,
        direction,
        after_edge,
    });
    write_historical_neighbor_records(
        engine
            .query(generation)
            .historical_neighbor_records(endpoint, direction, limit, after)?,
    )
}

fn write_historical_neighbor_records(
    records: Vec<syntaxmesh_api_model::TemporalRecord>,
) -> Result<(), CliError> {
    for record in records {
        write_stdout(&record.to_json_line().map_err(|error| {
            CliError::Usage(format!("serialize historical-neighbor record: {error}"))
        })?)?;
    }
    Ok(())
}

pub(super) fn parse_edge_direction(value: &str) -> Result<EdgeDirection, CliError> {
    match value {
        "outgoing" => Ok(EdgeDirection::Outgoing),
        "incoming" => Ok(EdgeDirection::Incoming),
        _ => Err(CliError::Usage(
            "edge direction must be outgoing or incoming".to_owned(),
        )),
    }
}

fn neighbors_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    id: NodeId,
) -> Result<(), CliError> {
    let query = engine.query(generation);
    write_neighbors(query.neighbors(id)?)
}

fn write_neighbors(
    items: Vec<(syntaxmesh_core::Edge, syntaxmesh_core::Node)>,
) -> Result<(), CliError> {
    for (edge, node) in items {
        write_stdout(&format!(
            "{:?}\t{}\t{}\t{:?}",
            edge.relation,
            node.name,
            node.id.0.to_hex(),
            node.source
        ))?;
    }
    Ok(())
}

pub(super) fn path(
    snapshot: &Path,
    start: NodeId,
    target: NodeId,
    max_hops: usize,
) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    path_engine(&engine, generation, start, target, max_hops)
}

pub(super) fn path_turso(
    database: &Path,
    start: NodeId,
    target: NodeId,
    max_hops: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    path_engine(&engine, generation, start, target, max_hops)
}

fn path_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    start: NodeId,
    target: NodeId,
    max_hops: usize,
) -> Result<(), CliError> {
    let query = engine.query(generation);
    if let Some(path) = query.path(start, target, max_hops)? {
        for id in path {
            if let Some(node) = query.node(id)? {
                write_stdout(&format!(
                    "{}\t{}\tsource={:?}",
                    node.name,
                    node.id.0.to_hex(),
                    node.source
                ))?;
            }
        }
    } else {
        write_stdout("no path within hop limit")?;
    }
    Ok(())
}

pub(super) fn subgraph(
    snapshot: &Path,
    seed: NodeId,
    max_hops: usize,
    max_nodes: usize,
    max_edges: usize,
) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    subgraph_engine(&engine, generation, seed, max_hops, max_nodes, max_edges)
}

pub(super) fn subgraph_turso(
    database: &Path,
    seed: NodeId,
    max_hops: usize,
    max_nodes: usize,
    max_edges: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    subgraph_engine(&engine, generation, seed, max_hops, max_nodes, max_edges)
}

fn subgraph_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    seed: NodeId,
    max_hops: usize,
    max_nodes: usize,
    max_edges: usize,
) -> Result<(), CliError> {
    for record in
        engine
            .query(generation)
            .export_subgraph(&[seed], max_hops, max_nodes, max_edges)?
    {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

fn consequence_neighborhood_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    seed: LineageEndpoint,
    max_hops: usize,
    max_endpoints: usize,
    max_edges: usize,
    max_scanned_incidences: usize,
) -> Result<(), CliError> {
    let neighborhood = engine.query(generation).consequence_neighborhood(
        &[seed],
        max_hops,
        max_endpoints,
        max_edges,
        max_scanned_incidences,
    )?;
    for item in &neighborhood.edges {
        let record = syntaxmesh_api_model::TemporalRecord::ConsequenceEdge {
            schema_version: syntaxmesh_api_model::TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: syntaxmesh_api_model::TemporalQueryMode::HistoricalConclusion,
            generation,
            depth: u64::try_from(item.depth).map_err(|error| {
                CliError::Usage(format!("edge depth does not fit u64: {error}"))
            })?,
            edge: item.edge.clone(),
        };
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    let footer = syntaxmesh_api_model::TemporalRecord::ConsequenceNeighborhoodFooter {
        schema_version: syntaxmesh_api_model::TEMPORAL_EXPORT_SCHEMA_VERSION,
        query_mode: syntaxmesh_api_model::TemporalQueryMode::HistoricalConclusion,
        generation,
        endpoints: u64::try_from(neighborhood.endpoints.len()).map_err(|error| {
            CliError::Usage(format!("endpoint count does not fit u64: {error}"))
        })?,
        edges: u64::try_from(neighborhood.edges.len())
            .map_err(|error| CliError::Usage(format!("edge count does not fit u64: {error}")))?,
        scanned_incidences: u64::try_from(neighborhood.scanned_incidences).map_err(|error| {
            CliError::Usage(format!("scanned incidence count does not fit u64: {error}"))
        })?,
        truncated: neighborhood.truncated,
    };
    write_stdout(&footer.to_json_line().map_err(CliError::Encode)?)
}

pub(super) fn consequence_neighborhood_file(
    snapshot: &Path,
    generation: GenerationId,
    seed: LineageEndpoint,
    max_hops: usize,
    max_endpoints: usize,
    max_edges: usize,
    max_scanned_incidences: usize,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    consequence_neighborhood_engine(
        &engine,
        generation,
        seed,
        max_hops,
        max_endpoints,
        max_edges,
        max_scanned_incidences,
    )
}

pub(super) fn consequence_neighborhood_turso(
    database: &Path,
    generation: GenerationId,
    seed: LineageEndpoint,
    max_hops: usize,
    max_endpoints: usize,
    max_edges: usize,
    max_scanned_incidences: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    consequence_neighborhood_engine(
        &engine,
        generation,
        seed,
        max_hops,
        max_endpoints,
        max_edges,
        max_scanned_incidences,
    )
}

fn consequence_trace_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    request: &ConsequenceTraceRequest,
    max_result_bytes: usize,
) -> Result<(), CliError> {
    for record in engine
        .query(request.until_generation)
        .consequence_trace_records(request, max_result_bytes)?
    {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

pub(super) fn consequence_trace_file(
    snapshot: &Path,
    request: &ConsequenceTraceRequest,
    max_result_bytes: usize,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    consequence_trace_engine(&engine, request, max_result_bytes)
}

pub(super) fn consequence_trace_turso(
    database: &Path,
    request: &ConsequenceTraceRequest,
    max_result_bytes: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    consequence_trace_engine(&engine, request, max_result_bytes)
}

pub(super) fn impact(snapshot: &Path, text: &str) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    impact_engine(&engine, generation, text)
}

pub(super) fn impact_turso(database: &Path, text: &str) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    impact_engine(&engine, generation, text)
}

fn impact_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    text: &str,
) -> Result<(), CliError> {
    let query = engine.query(generation);
    for target in query.search(text, 100)? {
        write_stdout(&format!("target={}\t{}", target.name, target.id.0.to_hex()))?;
        for impacted in query.impact(target.id, 100)? {
            if let Some(node) = query.node(impacted)? {
                write_stdout(&format!("  {}\t{}", node.name, node.id.0.to_hex()))?;
            }
        }
    }
    Ok(())
}
