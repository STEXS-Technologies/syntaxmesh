//! Temporal fact versions and persistent fact-tree storage for Turso.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "benchmark-instrumentation")]
use std::time::Instant;

use syntaxmesh_core::{
    Edge, EdgeDirection, EdgeId, GenerationHistoryEntry, GenerationId, GraphDelta, GraphSnapshot,
    Node, NodeId, Provenance, StableId,
};
use syntaxmesh_store::{
    HistoricalEdgePage, IncidenceMutation, MAX_HISTORICAL_EDGE_PAGE_SIZE, PersistentFactKey,
    PersistentFactMutation, PersistentFactNode, PersistentFactTree, PersistentFactTreeApply,
    PersistentFactTreeCache, PersistentFactTreeError, PersistentFactTreeWalker,
    PersistentIncidenceApply, PersistentIncidenceIndex, PersistentIncidencePageWalker, StoreError,
};

use super::{
    EDGE_FACT, FILE_FACT, NODE_FACT, PROVENANCE_FACT, TursoStoreError, decode_blob,
    graph_snapshot_root_v2, read_many_prepared, read_many_with_params,
};

const PERSISTENT_PAGE_BATCH_SIZE: usize = 100;

mod node_page_cache;
mod node_scan;
pub(super) use node_scan::visit_historical_nodes;
#[cfg(feature = "benchmark-instrumentation")]
mod node_read_profile;
#[cfg(feature = "benchmark-instrumentation")]
use node_read_profile::{NodeReadProfile, ReadStage};

#[cfg(test)]
mod point_plan_tests;

pub(super) const HISTORICAL_POINT_QUERY: &str = "SELECT payload FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence = (SELECT MAX(valid_from_sequence) FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence <= ?3) AND (valid_until_sequence IS NULL OR valid_until_sequence > ?3) LIMIT 1";

pub(super) async fn read_temporal_snapshot(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<GraphSnapshot, TursoStoreError> {
    let mut sequence_rows = connection
        .query(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("locate historical generation: {error}"))
        })?;
    let sequence_row = sequence_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read historical generation: {error}")))?
        .ok_or({
            TursoStoreError::Store(StoreError::StaleBase {
                expected: Some(generation),
                actual: None,
            })
        })?;
    let sequence = match sequence_row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("read historical sequence: {error}")))?
    {
        turso::Value::Integer(value) => value,
        turso::Value::Null
        | turso::Value::Real(_)
        | turso::Value::Text(_)
        | turso::Value::Blob(_) => {
            return Err(TursoStoreError::Snapshot(
                "historical sequence is not an integer".to_owned(),
            ));
        }
    };
    let snapshot = GraphSnapshot {
        files: read_many_with_params(connection, "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id", (FILE_FACT, sequence)).await?,
        provenance: read_many_with_params(connection, "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id", (PROVENANCE_FACT, sequence)).await?,
        nodes: read_many_with_params(connection, "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id", (NODE_FACT, sequence)).await?,
        edges: read_many_with_params(connection, "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id", (EDGE_FACT, sequence)).await?,
    };
    let history = read_many_with_params::<GenerationHistoryEntry>(
        connection,
        "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
        [generation.0.0.to_vec()],
    )
    .await?;
    let entry = history.first().ok_or({
        TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })
    })?;
    let root = snapshot_root_for_schema(generation, &snapshot, entry.manifest.schema_version)?;
    if root != entry.manifest.graph_root {
        return Err(TursoStoreError::Snapshot(
            "temporal snapshot root does not match generation manifest".to_owned(),
        ));
    }
    Ok(snapshot)
}

fn temporal_tree_key(
    fact_kind: i64,
    fact_id: StableId,
) -> Result<PersistentFactKey, TursoStoreError> {
    let fact_kind = u8::try_from(fact_kind).map_err(|error| {
        TursoStoreError::Snapshot(format!("invalid temporal fact kind: {error}"))
    })?;
    Ok(PersistentFactKey { fact_kind, fact_id })
}

fn add_temporal_tree_mutation(
    mutations: &mut BTreeMap<PersistentFactKey, PersistentFactMutation>,
    key: PersistentFactKey,
    value: Option<Vec<u8>>,
) {
    let mutation = value.map_or(PersistentFactMutation::Remove { key }, |value| {
        PersistentFactMutation::Upsert { key, value }
    });
    mutations.insert(key, mutation);
}

fn add_temporal_tree_fact_mutation<T: serde::Serialize>(
    mutations: &mut BTreeMap<PersistentFactKey, PersistentFactMutation>,
    key: PersistentFactKey,
    fact: &T,
    schema_version: u32,
) -> Result<(), TursoStoreError> {
    let value = encode_tree_fact(fact, schema_version)?;
    let mutation = if schema_version == 1 {
        PersistentFactMutation::Upsert { key, value }
    } else {
        let canonical_value = serde_json::to_vec(fact)
            .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
        let value_commitment = StableId::derive("persistent-fact-value-v1", &[&canonical_value]);
        PersistentFactMutation::UpsertWithCommitment {
            key,
            value,
            value_commitment,
        }
    };
    mutations.insert(key, mutation);
    Ok(())
}

pub(super) fn merge_persistent_mutations(
    base: Vec<PersistentFactMutation>,
    delta: Vec<PersistentFactMutation>,
) -> Vec<PersistentFactMutation> {
    let mut merged = BTreeMap::new();
    for mutation in base.into_iter().chain(delta) {
        merged.insert(mutation.key(), mutation);
    }
    merged.into_values().collect()
}

pub(super) fn snapshot_tree_mutations(
    snapshot: &GraphSnapshot,
    schema_version: u32,
) -> Result<Vec<PersistentFactMutation>, TursoStoreError> {
    let mut mutations = BTreeMap::new();
    for file in &snapshot.files {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(FILE_FACT, file.file_id.0)?,
            file,
            schema_version,
        )?;
    }
    for item in &snapshot.provenance {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(PROVENANCE_FACT, item.id.0)?,
            item,
            schema_version,
        )?;
    }
    for node in &snapshot.nodes {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(NODE_FACT, node.id.0)?,
            node,
            schema_version,
        )?;
    }
    for edge in &snapshot.edges {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(EDGE_FACT, edge.id.0)?,
            edge,
            schema_version,
        )?;
    }
    Ok(mutations.into_values().collect())
}

pub(super) async fn delta_tree_mutations(
    connection: &turso::Connection,
    delta: &GraphDelta,
    parent_sequence: Option<i64>,
    schema_version: u32,
    cascaded_edge_ids: Option<&std::collections::BTreeSet<EdgeId>>,
) -> Result<Vec<PersistentFactMutation>, TursoStoreError> {
    let mut mutations = BTreeMap::new();
    if let Some(edge_ids) = cascaded_edge_ids {
        for edge_id in edge_ids {
            add_temporal_tree_mutation(
                &mut mutations,
                temporal_tree_key(EDGE_FACT, edge_id.0)?,
                None,
            );
        }
    } else if let Some(sequence) = parent_sequence {
        for removed_nodes in delta.remove_nodes.chunks(128) {
            let values = (1..=removed_nodes.len())
                .map(|parameter| format!("(?{parameter})"))
                .collect::<Vec<_>>()
                .join(", ");
            let fact_kind_parameter = removed_nodes.len().checked_add(1).ok_or_else(|| {
                TursoStoreError::Snapshot("temporal query parameter count overflow".to_owned())
            })?;
            let sequence_parameter = fact_kind_parameter.checked_add(1).ok_or_else(|| {
                TursoStoreError::Snapshot("temporal query parameter count overflow".to_owned())
            })?;
            let sql = format!(
                "WITH removed(node_id) AS (VALUES {values}) \
                 SELECT fact.payload FROM removed CROSS JOIN syntaxmesh_fact_versions AS fact INDEXED BY syntaxmesh_fact_versions_source_idx \
                 WHERE fact.fact_kind = ?{fact_kind_parameter} AND fact.source_id = removed.node_id \
                   AND fact.valid_from_sequence <= ?{sequence_parameter} \
                   AND (fact.valid_until_sequence IS NULL OR fact.valid_until_sequence > ?{sequence_parameter}) \
                 UNION ALL \
                 SELECT fact.payload FROM removed CROSS JOIN syntaxmesh_fact_versions AS fact INDEXED BY syntaxmesh_fact_versions_target_idx \
                 WHERE fact.fact_kind = ?{fact_kind_parameter} AND fact.target_id = removed.node_id \
                   AND fact.valid_from_sequence <= ?{sequence_parameter} \
                   AND (fact.valid_until_sequence IS NULL OR fact.valid_until_sequence > ?{sequence_parameter})"
            );
            let removed_node_ids = removed_nodes
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            let mut parameters = removed_nodes
                .iter()
                .map(|node_id| turso::Value::Blob(node_id.0.0.to_vec()))
                .collect::<Vec<_>>();
            parameters.push(turso::Value::Integer(EDGE_FACT));
            parameters.push(turso::Value::Integer(sequence));
            let mut rows = connection
                .query(sql, turso::params_from_iter(parameters))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("read incident historical edges: {error}"))
                })?;
            while let Some(row) = rows.next().await.map_err(|error| {
                TursoStoreError::Backend(format!("read incident historical edge row: {error}"))
            })? {
                let edge: Edge = decode_blob(
                    row.get_value(0).map_err(|error| {
                        TursoStoreError::Backend(format!(
                            "read incident historical edge payload: {error}"
                        ))
                    })?,
                    "incident historical edge",
                )?;
                if !removed_node_ids.contains(&edge.source)
                    && !removed_node_ids.contains(&edge.target)
                {
                    return Err(TursoStoreError::Snapshot(
                        "incident temporal edge endpoint is invalid".to_owned(),
                    ));
                }
                add_temporal_tree_mutation(
                    &mut mutations,
                    temporal_tree_key(EDGE_FACT, edge.id.0)?,
                    None,
                );
            }
        }
    }
    for file_id in &delta.removed_files {
        add_temporal_tree_mutation(
            &mut mutations,
            temporal_tree_key(FILE_FACT, file_id.0)?,
            None,
        );
    }
    for edge_id in &delta.remove_edges {
        add_temporal_tree_mutation(
            &mut mutations,
            temporal_tree_key(EDGE_FACT, edge_id.0)?,
            None,
        );
    }
    for node_id in &delta.remove_nodes {
        add_temporal_tree_mutation(
            &mut mutations,
            temporal_tree_key(NODE_FACT, node_id.0)?,
            None,
        );
    }
    for file in &delta.changed_files {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(FILE_FACT, file.file_id.0)?,
            file,
            schema_version,
        )?;
    }
    for item in &delta.upsert_provenance {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(PROVENANCE_FACT, item.id.0)?,
            item,
            schema_version,
        )?;
    }
    for node in &delta.upsert_nodes {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(NODE_FACT, node.id.0)?,
            node,
            schema_version,
        )?;
    }
    for edge in &delta.upsert_edges {
        add_temporal_tree_fact_mutation(
            &mut mutations,
            temporal_tree_key(EDGE_FACT, edge.id.0)?,
            edge,
            schema_version,
        )?;
    }
    Ok(mutations.into_values().collect())
}

pub(super) async fn current_projection_cascade_edge_ids(
    connection: &turso::Connection,
    removed_nodes: &[NodeId],
) -> Result<std::collections::BTreeSet<EdgeId>, TursoStoreError> {
    let mut edge_ids = std::collections::BTreeSet::new();
    for node_batch in removed_nodes.chunks(128) {
        let values = (1..=node_batch.len())
            .map(|parameter| format!("(?{parameter})"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "WITH removed(node_id) AS (VALUES {values}) \
             SELECT edge.id FROM removed CROSS JOIN syntaxmesh_edges AS edge INDEXED BY syntaxmesh_edges_source_idx \
             WHERE edge.source = removed.node_id \
             UNION \
             SELECT edge.id FROM removed CROSS JOIN syntaxmesh_edges AS edge INDEXED BY syntaxmesh_edges_target_idx \
             WHERE edge.target = removed.node_id"
        );
        let parameters = node_batch
            .iter()
            .map(|node_id| turso::Value::Blob(node_id.0.0.to_vec()))
            .collect::<Vec<_>>();
        let mut rows = connection
            .query(sql, turso::params_from_iter(parameters))
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read current incident edge IDs: {error}"))
            })?;
        while let Some(row) = rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read current incident edge ID row: {error}"))
        })? {
            let edge_id = match row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("read current incident edge ID: {error}"))
            })? {
                turso::Value::Blob(bytes) => {
                    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                        TursoStoreError::Snapshot(format!(
                            "current incident edge ID has {} bytes; expected 32",
                            bytes.len()
                        ))
                    })?;
                    EdgeId(StableId(bytes))
                }
                value @ (turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Text(_)) => {
                    return Err(TursoStoreError::Snapshot(format!(
                        "current incident edge ID has invalid storage type: {value:?}"
                    )));
                }
            };
            edge_ids.insert(edge_id);
        }
    }
    Ok(edge_ids)
}

pub(super) async fn read_parent_sequence(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<i64, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("locate persistent-root parent: {error}"))
        })?;
    let row = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read persistent-root parent: {error}")))?
        .ok_or_else(|| {
            TursoStoreError::Snapshot("persistent-root parent is absent from history".to_owned())
        })?;
    row.get(0).map_err(|error| {
        TursoStoreError::Backend(format!("decode persistent-root parent sequence: {error}"))
    })
}

pub(super) async fn read_persistent_root(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<Option<StableId>, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT root_id FROM syntaxmesh_temporal_roots WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("locate persistent generation root: {error}"))
        })?;
    let row = rows
        .next()
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read persistent generation root: {error}"))
        })?
        .ok_or_else(|| {
            TursoStoreError::Snapshot("generation is missing its persistent fact root".to_owned())
        })?;
    match row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("read persistent root ID: {error}")))?
    {
        turso::Value::Null => Ok(None),
        turso::Value::Blob(bytes) => {
            let id: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                TursoStoreError::Snapshot(format!(
                    "persistent root ID is not 32 bytes: got {}",
                    bytes.len()
                ))
            })?;
            Ok(Some(StableId(id)))
        }
        value @ turso::Value::Integer(_)
        | value @ turso::Value::Real(_)
        | value @ turso::Value::Text(_) => Err(TursoStoreError::Snapshot(format!(
            "persistent root ID has invalid storage type: {value:?}"
        ))),
    }
}

pub(super) async fn read_incidence_root(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<Option<StableId>, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT root_id FROM syntaxmesh_temporal_incidence_roots WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("locate incidence root: {error}")))?;
    let row = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read incidence root: {error}")))?
        .ok_or_else(|| {
            TursoStoreError::Snapshot("generation is missing its incidence root".to_owned())
        })?;
    match row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode incidence root: {error}")))?
    {
        turso::Value::Null => Ok(None),
        turso::Value::Blob(bytes) => {
            let id: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                TursoStoreError::Snapshot(format!(
                    "incidence root ID is not 32 bytes: {}",
                    bytes.len()
                ))
            })?;
            Ok(Some(StableId(id)))
        }
        value @ (turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_)) => {
            Err(TursoStoreError::Snapshot(format!(
                "incidence root has invalid storage type: {value:?}"
            )))
        }
    }
}

pub(super) async fn historical_search_nodes(
    connection: &turso::Connection,
    schema_cache: &super::history_schema_cache::HistorySchemaCache,
    generation: GenerationId,
    text: &str,
    limit: usize,
) -> Result<Vec<Node>, TursoStoreError> {
    let text = text.to_lowercase();
    historical_nodes_matching(connection, schema_cache, generation, None, limit, |node| {
        node.name.to_lowercase().contains(&text)
    })
    .await
}

pub(super) async fn historical_nodes_page(
    connection: &turso::Connection,
    schema_cache: &super::history_schema_cache::HistorySchemaCache,
    generation: GenerationId,
    after: Option<NodeId>,
    limit: usize,
) -> Result<syntaxmesh_store::HistoricalNodePage, TursoStoreError> {
    if limit == 0 || limit > syntaxmesh_store::MAX_HISTORICAL_NODE_PAGE_SIZE {
        return Err(TursoStoreError::Store(StoreError::InvalidPageLimit));
    }
    let mut items = historical_nodes_matching(
        connection,
        schema_cache,
        generation,
        after,
        limit.saturating_add(1),
        |_node| true,
    )
    .await?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(syntaxmesh_store::HistoricalNodePage { items, has_more })
}

async fn historical_nodes_matching(
    connection: &turso::Connection,
    schema_cache: &super::history_schema_cache::HistorySchemaCache,
    generation: GenerationId,
    after: Option<NodeId>,
    limit: usize,
    predicate: impl Fn(&Node) -> bool,
) -> Result<Vec<Node>, TursoStoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let mut profile = NodeReadProfile::new();
    #[cfg(feature = "benchmark-instrumentation")]
    let schema_started = profile.timer();
    let schema_version =
        super::history_schema_cache::read_schema(connection, schema_cache, generation).await?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.record(ReadStage::Schema, schema_started);
    #[cfg(feature = "benchmark-instrumentation")]
    let root_started = profile.timer();
    let root = read_persistent_root(connection, generation).await?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.record(ReadStage::Root, root_started);
    if limit == 0 {
        return Ok(Vec::new());
    }
    let after = PersistentFactKey {
        fact_kind: u8::try_from(if after.is_some() {
            NODE_FACT
        } else {
            NODE_FACT - 1
        })
        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
        fact_id: after.map_or(StableId([u8::MAX; 32]), |node| node.0),
    };
    let mut walker = syntaxmesh_store::PersistentFactTreeRangeWalker::new(root, Some(after));
    let mut cache = node_page_cache::NodePageCache::default();
    let mut matches = Vec::new();
    let mut page_statement = connection
        .prepare("SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1")
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("prepare historical node page lookup: {error}"))
        })?;
    loop {
        match walker.next(cache.pages()) {
            Ok(Some(page)) => {
                if i64::from(page.key.fact_kind) != NODE_FACT {
                    break;
                }
                #[cfg(feature = "benchmark-instrumentation")]
                let decode_started = profile.timer();
                let node: Node = decode_tree_fact(page.value_payload(), schema_version)?;
                #[cfg(feature = "benchmark-instrumentation")]
                profile.record(ReadStage::DecodeNode, decode_started);
                if node.id.0 != page.key.fact_id {
                    return Err(TursoStoreError::Snapshot(
                        "historical search node identity differs from tree key".to_owned(),
                    ));
                }
                if predicate(&node) {
                    matches.push(node);
                }
                cache.yielded();
                if matches.len() == limit {
                    break;
                }
            }
            Ok(None) => break,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                #[cfg(feature = "benchmark-instrumentation")]
                let sql_started = profile.timer();
                let page =
                    read_many_prepared::<PersistentFactNode>(&mut page_statement, [id.0.to_vec()])
                        .await?
                        .pop()
                        .ok_or_else(|| {
                            TursoStoreError::Snapshot(
                                "historical search references a missing page".to_owned(),
                            )
                        })?;
                #[cfg(feature = "benchmark-instrumentation")]
                profile.record(ReadStage::SqlPage, sql_started);
                #[cfg(feature = "benchmark-instrumentation")]
                let validation_started = profile.timer();
                cache
                    .insert_loaded(id, page)
                    .map_err(|error| persistent_tree_error(&error))?;
                #[cfg(feature = "benchmark-instrumentation")]
                profile.record(ReadStage::ValidatePage, validation_started);
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.finish(matches.len());
    Ok(matches)
}

pub(super) async fn historical_incident_edge_page(
    connection: &turso::Connection,
    generation: GenerationId,
    endpoint: NodeId,
    direction: EdgeDirection,
    after: Option<EdgeId>,
    limit: usize,
) -> Result<HistoricalEdgePage, TursoStoreError> {
    let sequence = super::history_sequence_for_generation(connection, generation).await?;
    #[cfg(feature = "benchmark-instrumentation")]
    let mut sql_read_statements = 1_usize;
    if limit == 0 || limit > MAX_HISTORICAL_EDGE_PAGE_SIZE {
        return Err(TursoStoreError::Store(StoreError::InvalidPageLimit));
    }
    let incidence_root = read_incidence_root(connection, generation).await?;
    #[cfg(feature = "benchmark-instrumentation")]
    {
        sql_read_statements = sql_read_statements.saturating_add(1);
    }
    let mut cache = PersistentFactTreeCache::default();
    let outgoing = direction == EdgeDirection::Outgoing;
    let mut page_statement = connection
        .prepare("SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1")
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("prepare incidence page lookup: {error}"))
        })?;
    let mut endpoint_lookup =
        PersistentIncidenceIndex::endpoint_lookup(incidence_root, endpoint, outgoing);
    let inner_root = loop {
        match endpoint_lookup.resolve(&cache) {
            Ok(root) => break root,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    sql_read_statements = sql_read_statements.saturating_add(1);
                }
                let mut pages =
                    read_many_prepared::<PersistentFactNode>(&mut page_statement, [id.0.to_vec()])
                        .await?;
                let page = pages.pop().ok_or_else(|| {
                    TursoStoreError::Snapshot("incidence root references a missing page".to_owned())
                })?;
                cache
                    .insert_loaded(id, page)
                    .map_err(|error| persistent_tree_error(&error))?;
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        }
    };
    let Some(inner_root) = inner_root else {
        let page_result = HistoricalEdgePage::new(Vec::new(), false);
        #[cfg(feature = "benchmark-instrumentation")]
        let page_result =
            page_result.with_read_metrics(syntaxmesh_store::HistoricalEdgeReadMetrics {
                sql_read_statements,
                incidence_index_page_lookups: cache.page_lookup_count(),
                incidence_index_pages_loaded: cache.loaded_page_count(),
                ..syntaxmesh_store::HistoricalEdgeReadMetrics::default()
            });
        return Ok(page_result);
    };
    let fetch_count = limit.checked_add(1).ok_or_else(|| {
        TursoStoreError::Snapshot("historical edge page limit overflows usize".to_owned())
    })?;
    let mut edge_walker = PersistentIncidencePageWalker::new(Some(inner_root), after);
    let mut edge_ids = Vec::with_capacity(fetch_count);
    while edge_ids.len() < fetch_count {
        match edge_walker.next(&cache) {
            Ok(Some(edge_id)) => edge_ids.push(edge_id),
            Ok(None) => break,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    sql_read_statements = sql_read_statements.saturating_add(1);
                }
                let mut pages =
                    read_many_prepared::<PersistentFactNode>(&mut page_statement, [id.0.to_vec()])
                        .await?;
                let page = pages.pop().ok_or_else(|| {
                    TursoStoreError::Snapshot("incidence set references a missing page".to_owned())
                })?;
                cache
                    .insert_loaded(id, page)
                    .map_err(|error| persistent_tree_error(&error))?;
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        }
    }
    let has_more = edge_ids.len() > limit;
    let mut items = Vec::with_capacity(edge_ids.len().min(limit));
    let mut edge_statement = connection
        .prepare(HISTORICAL_POINT_QUERY)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("prepare historical edge hydration: {error}"))
        })?;
    for edge_id in edge_ids.into_iter().take(limit) {
        #[cfg(feature = "benchmark-instrumentation")]
        {
            sql_read_statements = sql_read_statements.saturating_add(1);
        }
        let mut edges = read_many_prepared::<Edge>(
            &mut edge_statement,
            (EDGE_FACT, edge_id.0.0.to_vec(), sequence),
        )
        .await?;
        let edge = edges.pop().ok_or_else(|| {
            TursoStoreError::Snapshot(format!(
                "incidence index refers to inactive historical edge {edge_id:?}"
            ))
        })?;
        if edge.id != edge_id
            || (outgoing && edge.source != endpoint)
            || (!outgoing && edge.target != endpoint)
        {
            return Err(TursoStoreError::Snapshot(
                "incidence index disagrees with historical edge payload".to_owned(),
            ));
        }
        items.push(edge);
    }
    #[cfg(feature = "benchmark-instrumentation")]
    let metrics = syntaxmesh_store::HistoricalEdgeReadMetrics {
        sql_read_statements,
        incidence_index_page_lookups: cache.page_lookup_count(),
        incidence_index_pages_loaded: cache.loaded_page_count(),
        edge_payload_rows_fetched: items.len(),
        ..syntaxmesh_store::HistoricalEdgeReadMetrics::default()
    };
    let page_result = HistoricalEdgePage::new(items, has_more);
    #[cfg(feature = "benchmark-instrumentation")]
    let page_result = page_result.with_read_metrics(metrics);
    Ok(page_result)
}

pub(super) async fn publish_incidence_root(
    connection: &turso::Connection,
    sequence: i64,
    generation: GenerationId,
    parent: Option<GenerationId>,
    changes: &[IncidenceMutation],
) -> Result<Option<StableId>, TursoStoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let mut profile_started = std::env::var_os("SYNTAXMESH_TURSO_PROFILE").map(|_| Instant::now());
    let old_root = match parent {
        Some(parent) => read_incidence_root(connection, parent).await?,
        None => None,
    };
    let mut cache = PersistentFactTreeCache::default();
    let (root, dirty_roots) = if parent.is_none() {
        let (root, nested_roots) = PersistentIncidenceIndex::apply(None, changes, &mut cache)
            .map_err(|error| persistent_tree_error(&error))?;
        let dirty_roots = root.into_iter().chain(nested_roots).collect::<Vec<_>>();
        (root, dirty_roots)
    } else {
        let mut apply = PersistentIncidenceApply::new(old_root, changes);
        loop {
            match apply.advance(&mut cache) {
                Ok(true) => {}
                Ok(false) => break,
                Err(PersistentFactTreeError::MissingPage(id)) => {
                    let mut pages = read_many_with_params::<PersistentFactNode>(
                        connection,
                        "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                        [id.0.to_vec()],
                    )
                    .await?;
                    let page = pages.pop().ok_or_else(|| {
                        TursoStoreError::Snapshot(
                            "incidence root references a missing page".to_owned(),
                        )
                    })?;
                    cache
                        .insert_loaded(id, page)
                        .map_err(|error| persistent_tree_error(&error))?;
                }
                Err(error) => return Err(persistent_tree_error(&error)),
            }
        }
        let root = apply.root();
        let dirty_roots = apply.dirty_roots().collect::<Vec<_>>();
        (root, dirty_roots)
    };
    #[cfg(feature = "benchmark-instrumentation")]
    mark_turso_stage("persistent_incidence_tree_apply", &mut profile_started);
    persist_dirty_pages(
        connection,
        cache.take_reachable_dirty_roots(dirty_roots),
        "incidence",
    )
    .await?;
    #[cfg(feature = "benchmark-instrumentation")]
    mark_turso_stage(
        "persistent_incidence_page_persistence",
        &mut profile_started,
    );
    connection.execute(
        "INSERT INTO syntaxmesh_temporal_incidence_roots (sequence, generation, root_id) VALUES (?1, ?2, ?3)",
        (sequence, generation.0.0.to_vec(), root.map(|id| id.0.to_vec())),
    ).await.map_err(|error| TursoStoreError::Backend(format!("publish incidence root: {error}")))?;
    #[cfg(feature = "benchmark-instrumentation")]
    mark_turso_stage("persistent_incidence_root_row", &mut profile_started);
    Ok(root)
}

#[cfg(feature = "benchmark-instrumentation")]
fn mark_turso_stage(stage: &str, started: &mut Option<Instant>) {
    if let Some(started) = started {
        eprintln!(
            "turso_stage stage={stage} elapsed_us={}",
            started.elapsed().as_micros()
        );
        *started = Instant::now();
    }
}

pub(super) fn snapshot_incidence_changes(snapshot: &GraphSnapshot) -> Vec<IncidenceMutation> {
    snapshot
        .edges
        .iter()
        .map(|edge| IncidenceMutation {
            edge: edge.id,
            source: edge.source,
            target: edge.target,
            present: true,
        })
        .collect()
}

pub(super) async fn delta_incidence_changes(
    connection: &turso::Connection,
    delta: &GraphDelta,
    cascaded_edge_ids: &BTreeSet<EdgeId>,
) -> Result<Vec<IncidenceMutation>, TursoStoreError> {
    let mut removed = delta.remove_edges.iter().copied().collect::<BTreeSet<_>>();
    removed.extend(cascaded_edge_ids.iter().copied());
    removed.extend(delta.upsert_edges.iter().map(|edge| edge.id));
    let mut changes = Vec::new();
    for edge_id in removed {
        let mut edges = read_many_with_params::<Edge>(
            connection,
            "SELECT payload FROM syntaxmesh_edges WHERE id = ?1",
            [edge_id.0.0.to_vec()],
        )
        .await?;
        if let Some(edge) = edges.pop() {
            if edge.id != edge_id {
                return Err(TursoStoreError::Snapshot(
                    "edge row ID disagrees with payload during incidence update".to_owned(),
                ));
            }
            changes.push(IncidenceMutation {
                edge: edge.id,
                source: edge.source,
                target: edge.target,
                present: false,
            });
        }
    }
    changes.extend(delta.upsert_edges.iter().map(|edge| IncidenceMutation {
        edge: edge.id,
        source: edge.source,
        target: edge.target,
        present: true,
    }));
    Ok(changes)
}

async fn apply_persistent_mutations(
    connection: &turso::Connection,
    old_root: Option<StableId>,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, TursoStoreError> {
    let mut cache = PersistentFactTreeCache::default();
    let new_root = if old_root.is_none() {
        PersistentFactTree::apply(old_root, mutations, &mut cache)
            .map_err(|error| persistent_tree_error(&error))?
    } else {
        let mut update = PersistentFactTreeApply::new(old_root, mutations);
        loop {
            match update.advance(&mut cache) {
                Ok(true) => {}
                Ok(false) => break update.root(),
                Err(PersistentFactTreeError::MissingPage(id)) => {
                    let mut pages = read_many_with_params::<PersistentFactNode>(
                        connection,
                        "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                        [id.0.to_vec()],
                    )
                    .await?;
                    let page = pages.pop().ok_or_else(|| {
                        TursoStoreError::Snapshot(
                            "persistent root references a missing page".to_owned(),
                        )
                    })?;
                    cache
                        .insert_loaded(id, page)
                        .map_err(|error| persistent_tree_error(&error))?;
                }
                Err(error) => return Err(persistent_tree_error(&error)),
            }
        }
    };
    persist_dirty_pages(
        connection,
        cache.take_reachable_dirty(new_root),
        "shared temporal",
    )
    .await?;
    Ok(new_root)
}

async fn persist_dirty_pages(
    connection: &turso::Connection,
    dirty_pages: Vec<(StableId, PersistentFactNode)>,
    label: &str,
) -> Result<(), TursoStoreError> {
    if dirty_pages.is_empty() {
        return Ok(());
    }
    const SINGLE_INSERT: &str = "INSERT OR IGNORE INTO syntaxmesh_temporal_tree_pages (page_id, left_page, right_page, payload) VALUES (?1, ?2, ?3, ?4)";
    if dirty_pages.len() < PERSISTENT_PAGE_BATCH_SIZE {
        let mut statement = connection.prepare(SINGLE_INSERT).await.map_err(|error| {
            TursoStoreError::Backend(format!("prepare {label} page insert: {error}"))
        })?;
        for (id, node) in dirty_pages {
            statement
                .execute((
                    id.0.to_vec(),
                    node.left.map(|child| child.0.to_vec()),
                    node.right.map(|child| child.0.to_vec()),
                    bincode::serialize(&node)
                        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                ))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("persist {label} page: {error}"))
                })?;
        }
        return Ok(());
    }

    let full_batch_sql = format!(
        "INSERT OR IGNORE INTO syntaxmesh_temporal_tree_pages (page_id, left_page, right_page, payload) VALUES {}",
        page_values_clause(PERSISTENT_PAGE_BATCH_SIZE)?
    );
    let mut full_batch_statement = connection.prepare(&full_batch_sql).await.map_err(|error| {
        TursoStoreError::Backend(format!("prepare batched {label} page insert: {error}"))
    })?;
    for pages in dirty_pages.chunks(PERSISTENT_PAGE_BATCH_SIZE) {
        let mut parameters = Vec::with_capacity(pages.len().saturating_mul(4));
        for (id, node) in pages {
            parameters.extend([
                turso::Value::Blob(id.0.to_vec()),
                node.left.map_or(turso::Value::Null, |child| {
                    turso::Value::Blob(child.0.to_vec())
                }),
                node.right.map_or(turso::Value::Null, |child| {
                    turso::Value::Blob(child.0.to_vec())
                }),
                turso::Value::Blob(
                    bincode::serialize(node)
                        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                ),
            ]);
        }
        if pages.len() == PERSISTENT_PAGE_BATCH_SIZE {
            full_batch_statement
                .execute(turso::params_from_iter(parameters))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("persist {label} page batch: {error}"))
                })?;
        } else {
            let tail_sql = format!(
                "INSERT OR IGNORE INTO syntaxmesh_temporal_tree_pages (page_id, left_page, right_page, payload) VALUES {}",
                page_values_clause(pages.len())?
            );
            connection
                .execute(&tail_sql, turso::params_from_iter(parameters))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("persist {label} page tail batch: {error}"))
                })?;
        }
    }
    Ok(())
}

fn page_values_clause(row_count: usize) -> Result<String, TursoStoreError> {
    let mut parameter = 1_usize;
    let mut rows = Vec::with_capacity(row_count);
    for _ in 0..row_count {
        let mut columns = Vec::with_capacity(4);
        for _ in 0..4 {
            columns.push(format!("?{parameter}"));
            parameter = parameter.checked_add(1).ok_or_else(|| {
                TursoStoreError::Snapshot("persistent-page bind index overflow".to_owned())
            })?;
        }
        rows.push(format!("({})", columns.join(", ")));
    }
    Ok(rows.join(", "))
}

pub(super) fn persistent_tree_error(error: &PersistentFactTreeError) -> TursoStoreError {
    TursoStoreError::Snapshot(format!("persistent fact tree error: {error:?}"))
}

pub(super) async fn publish_persistent_root(
    connection: &turso::Connection,
    sequence: i64,
    generation: GenerationId,
    parent: Option<GenerationId>,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, TursoStoreError> {
    if parent.is_none() && sequence != 1 {
        return Err(TursoStoreError::Snapshot(
            "non-initial generation has no persistent-root parent".to_owned(),
        ));
    }
    publish_persistent_root_from_base(connection, sequence, generation, parent, mutations).await
}

/// Publishes a complete snapshot as a new tree root after a root encoding
/// transition. It intentionally does not inherit the prior root.
pub(super) async fn publish_rebuilt_persistent_root(
    connection: &turso::Connection,
    sequence: i64,
    generation: GenerationId,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, TursoStoreError> {
    publish_persistent_root_from_base(connection, sequence, generation, None, mutations).await
}

async fn publish_persistent_root_from_base(
    connection: &turso::Connection,
    sequence: i64,
    generation: GenerationId,
    parent: Option<GenerationId>,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, TursoStoreError> {
    let old_root = match parent {
        Some(parent) => read_persistent_root(connection, parent).await?,
        None => None,
    };
    let root = apply_persistent_mutations(connection, old_root, mutations).await?;
    connection
        .execute(
            "INSERT INTO syntaxmesh_temporal_roots (sequence, generation, root_id) VALUES (?1, ?2, ?3)",
            (sequence, generation.0.0.to_vec(), root.map(|id| id.0.to_vec())),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("publish persistent generation root: {error}")))?;
    Ok(root)
}

pub(super) async fn read_persistent_snapshot(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<GraphSnapshot, TursoStoreError> {
    let history = read_many_with_params::<GenerationHistoryEntry>(
        connection,
        "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
        [generation.0.0.to_vec()],
    )
    .await?;
    let entry = history.first().ok_or({
        TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })
    })?;
    let schema_version = entry.manifest.schema_version;
    let root = read_persistent_root(connection, generation).await?;
    let mut snapshot = GraphSnapshot::default();
    let mut cache = PersistentFactTreeCache::default();
    if let Some(root_id) = root {
        let sql = "WITH RECURSIVE reachable(page_id) AS (\
            SELECT ?1 \
            UNION \
            SELECT page.left_page \
            FROM reachable AS current \
            JOIN syntaxmesh_temporal_tree_pages AS page ON page.page_id = current.page_id \
            WHERE page.left_page IS NOT NULL \
            UNION \
            SELECT page.right_page \
            FROM reachable AS current \
            JOIN syntaxmesh_temporal_tree_pages AS page ON page.page_id = current.page_id \
            WHERE page.right_page IS NOT NULL\
        ) \
        SELECT page.page_id, page.payload \
        FROM reachable JOIN syntaxmesh_temporal_tree_pages AS page USING (page_id)";
        let mut rows = connection
            .query(sql, [root_id.0.to_vec()])
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read persistent fact pages: {error}"))
            })?;
        while let Some(row) = rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read persistent fact page row: {error}"))
        })? {
            let id = match row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("read persistent page ID: {error}"))
            })? {
                turso::Value::Blob(id) => StableId(id.try_into().map_err(|id: Vec<u8>| {
                    TursoStoreError::Snapshot(format!(
                        "persistent page ID is not 32 bytes: {}",
                        id.len()
                    ))
                })?),
                turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Text(_) => {
                    return Err(TursoStoreError::Snapshot(
                        "persistent page ID is not a blob".to_owned(),
                    ));
                }
            };
            let page = match row.get_value(1).map_err(|error| {
                TursoStoreError::Backend(format!("read persistent page payload: {error}"))
            })? {
                turso::Value::Blob(payload) => bincode::deserialize::<PersistentFactNode>(&payload)
                    .map_err(|error| {
                        TursoStoreError::Snapshot(format!("decode persistent fact page: {error}"))
                    })?,
                turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Text(_) => {
                    return Err(TursoStoreError::Snapshot(
                        "persistent page payload is not a blob".to_owned(),
                    ));
                }
            };
            cache
                .insert_loaded(id, page)
                .map_err(|error| persistent_tree_error(&error))?;
        }
    }
    let mut walker = PersistentFactTreeWalker::new(root);
    loop {
        let (id, page) = match walker.next(&cache) {
            Ok(Some(page)) => page,
            Ok(None) => break,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "persistent fact root references missing page {id:?}"
                )));
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        };
        if page.id() != id {
            return Err(TursoStoreError::Snapshot(
                "persistent fact page content address mismatch".to_owned(),
            ));
        }
        match i64::from(page.key.fact_kind) {
            FILE_FACT => {
                let fact: syntaxmesh_core::FileVersion =
                    decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.file_id.0 != page.key.fact_id {
                    return Err(TursoStoreError::Snapshot(
                        "persistent file key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.files.push(fact);
            }
            PROVENANCE_FACT => {
                let fact: Provenance = decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.id.0 != page.key.fact_id {
                    return Err(TursoStoreError::Snapshot(
                        "persistent provenance key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.provenance.push(fact);
            }
            NODE_FACT => {
                let fact: Node = decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.id.0 != page.key.fact_id {
                    return Err(TursoStoreError::Snapshot(
                        "persistent node key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.nodes.push(fact);
            }
            EDGE_FACT => {
                let fact: Edge = decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.id.0 != page.key.fact_id {
                    return Err(TursoStoreError::Snapshot(
                        "persistent edge key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.edges.push(fact);
            }
            _ => {
                return Err(TursoStoreError::Snapshot(
                    "persistent fact page has an unknown family".to_owned(),
                ));
            }
        }
    }
    snapshot.files.sort_by_key(|fact| fact.file_id);
    snapshot.provenance.sort_by_key(|fact| fact.id);
    snapshot.nodes.sort_by_key(|fact| fact.id);
    snapshot.edges.sort_by_key(|fact| fact.id);
    Ok(snapshot)
}

pub(super) async fn read_temporal_nodes_by_ids(
    connection: &turso::Connection,
    generation: GenerationId,
    ids: &[NodeId],
) -> Result<Vec<Node>, TursoStoreError> {
    let sequence = read_parent_sequence(connection, generation).await?;
    let unique = ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(HISTORICAL_POINT_QUERY)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("prepare historical node batch: {error}"))
        })?;
    let mut nodes = Vec::<Node>::new();
    for id in &unique {
        nodes.extend(
            super::read_many_prepared::<Node>(
                &mut statement,
                (NODE_FACT, id.0.0.to_vec(), sequence),
            )
            .await?,
        );
    }
    let mut previous = None;
    for node in &nodes {
        if !unique.contains(&node.id) || previous.is_some_and(|id| node.id <= id) {
            return Err(TursoStoreError::Snapshot(
                "historical node batch has foreign or unordered IDs".to_owned(),
            ));
        }
        previous = Some(node.id);
    }
    Ok(nodes)
}

pub(super) async fn read_temporal_node(
    connection: &turso::Connection,
    generation: GenerationId,
    id: NodeId,
) -> Result<Option<Node>, TursoStoreError> {
    read_temporal_evidence(connection, generation, NODE_FACT, id.0).await
}

pub(super) async fn read_temporal_evidence<T: serde::de::DeserializeOwned>(
    connection: &turso::Connection,
    generation: GenerationId,
    kind: i64,
    id: syntaxmesh_core::StableId,
) -> Result<Option<T>, TursoStoreError> {
    let mut sequence_rows = connection
        .query(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("locate historical generation: {error}"))
        })?;
    let Some(sequence_row) = sequence_rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read historical generation: {error}"))
    })?
    else {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        }));
    };
    let sequence = match sequence_row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("read historical sequence: {error}")))?
    {
        turso::Value::Integer(value) => value,
        turso::Value::Null
        | turso::Value::Real(_)
        | turso::Value::Text(_)
        | turso::Value::Blob(_) => {
            return Err(TursoStoreError::Snapshot(
                "historical sequence is not an integer".to_owned(),
            ));
        }
    };
    let mut nodes = read_many_with_params(
        connection,
        HISTORICAL_POINT_QUERY,
        (kind, id.0.to_vec(), sequence),
    )
    .await?;
    Ok(nodes.pop())
}

fn snapshot_root(
    generation: GenerationId,
    snapshot: &GraphSnapshot,
) -> Result<[u8; 32], TursoStoreError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&generation.0.0);
    for node in &snapshot.nodes {
        hasher.update(&serde_json::to_vec(node).map_err(|error| {
            TursoStoreError::Snapshot(format!("serialize historical node root: {error}"))
        })?);
    }
    for edge in &snapshot.edges {
        hasher.update(&serde_json::to_vec(edge).map_err(|error| {
            TursoStoreError::Snapshot(format!("serialize historical edge root: {error}"))
        })?);
    }
    Ok(*hasher.finalize().as_bytes())
}

pub(super) fn snapshot_root_for_schema(
    generation: GenerationId,
    snapshot: &GraphSnapshot,
    schema_version: u32,
) -> Result<[u8; 32], TursoStoreError> {
    match schema_version {
        1 => snapshot_root(generation, snapshot),
        2 => graph_snapshot_root_v2(generation, snapshot).map_err(TursoStoreError::Store),
        version => Err(TursoStoreError::Snapshot(format!(
            "unsupported generation root schema version {version}"
        ))),
    }
}

fn encode_tree_fact<T: serde::Serialize>(
    fact: &T,
    schema_version: u32,
) -> Result<Vec<u8>, TursoStoreError> {
    match schema_version {
        1 => bincode::serialize(fact).map_err(|error| TursoStoreError::Snapshot(error.to_string())),
        2 => {
            const COMPACT_TREE_FACT_MAGIC: &[u8; 4] = b"SMB1";
            let mut payload = COMPACT_TREE_FACT_MAGIC.to_vec();
            payload.extend_from_slice(
                &bincode::serialize(fact)
                    .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
            );
            Ok(payload)
        }
        version => Err(TursoStoreError::Snapshot(format!(
            "unsupported persistent tree value schema version {version}"
        ))),
    }
}

pub(super) fn decode_tree_fact<T: serde::de::DeserializeOwned>(
    payload: &[u8],
    schema_version: u32,
) -> Result<T, TursoStoreError> {
    match schema_version {
        1 => bincode::deserialize(payload)
            .map_err(|error| TursoStoreError::Snapshot(error.to_string())),
        2 => {
            const COMPACT_TREE_FACT_MAGIC: &[u8; 4] = b"SMB1";
            payload.strip_prefix(COMPACT_TREE_FACT_MAGIC).map_or_else(
                || {
                    serde_json::from_slice(payload)
                        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))
                },
                |compact_payload| {
                    bincode::deserialize(compact_payload)
                        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))
                },
            )
        }
        version => Err(TursoStoreError::Snapshot(format!(
            "unsupported persistent tree value schema version {version}"
        ))),
    }
}

pub(super) async fn read_graph_at(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<GraphSnapshot, TursoStoreError> {
    let snapshot = read_persistent_snapshot(connection, generation).await?;
    let history = read_many_with_params::<GenerationHistoryEntry>(
        connection,
        "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
        [generation.0.0.to_vec()],
    )
    .await?;
    let entry = history.first().ok_or({
        TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })
    })?;
    if snapshot_root_for_schema(generation, &snapshot, entry.manifest.schema_version)?
        != entry.manifest.graph_root
    {
        return Err(TursoStoreError::Snapshot(
            "persistent historical graph root differs from generation manifest".to_owned(),
        ));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_page_batch_placeholders_are_contiguous() -> Result<(), TursoStoreError> {
        let placeholders = page_values_clause(2)?;
        if placeholders != "(?1, ?2, ?3, ?4), (?5, ?6, ?7, ?8)" {
            return Err(TursoStoreError::Snapshot(format!(
                "persistent page batch placeholders were not contiguous: {placeholders}"
            )));
        }
        Ok(())
    }

    #[test]
    fn schema_two_tree_decoder_reads_legacy_json_and_compact_bincode() -> Result<(), TursoStoreError>
    {
        let fact = vec![1_u64, 256, u64::MAX];
        let legacy_json = serde_json::to_vec(&fact)
            .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
        let compact = encode_tree_fact(&fact, 2)?;
        if decode_tree_fact::<Vec<u64>>(&legacy_json, 2)? != fact
            || decode_tree_fact::<Vec<u64>>(&compact, 2)? != fact
        {
            return Err(TursoStoreError::Snapshot(
                "schema-v2 tree decoder did not accept both payload formats".to_owned(),
            ));
        }
        Ok(())
    }
}
