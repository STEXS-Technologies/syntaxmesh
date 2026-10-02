//! Persistent fact-tree mutation and root publication for SQLite.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "benchmark-instrumentation")]
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use syntaxmesh_core::{
    Edge, EdgeDirection, EdgeId, GenerationId, GraphDelta, GraphSnapshot, Node, NodeId, Provenance,
    StableId,
};
use syntaxmesh_store::{
    HistoricalEdgePage, IncidenceMutation, MAX_HISTORICAL_EDGE_PAGE_SIZE, PersistentFactKey,
    PersistentFactMutation, PersistentFactNode, PersistentFactTree, PersistentFactTreeApply,
    PersistentFactTreeCache, PersistentFactTreeError, PersistentFactTreeWalker,
    PersistentIncidenceApply, PersistentIncidenceIndex, PersistentIncidencePageWalker, StoreError,
    graph_snapshot_root_v2,
};

use super::{
    EDGE_FACT, FILE_FACT, NODE_FACT, PROVENANCE_FACT, decode, decode_tree_fact, encode,
    encode_tree_fact, read_generation_history_entry, read_manifest, read_many_with_params,
    sql_error, stable_id,
};

const INCIDENT_EDGE_NODE_BATCH_SIZE: usize = 128;

pub(super) fn snapshot_tree_mutations(
    snapshot: &GraphSnapshot,
    schema_version: u32,
) -> Result<Vec<PersistentFactMutation>, StoreError> {
    let mut mutations = BTreeMap::new();
    for file in &snapshot.files {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(FILE_FACT, file.file_id.0)?,
            file,
            schema_version,
        )?;
    }
    for item in &snapshot.provenance {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(PROVENANCE_FACT, item.id.0)?,
            item,
            schema_version,
        )?;
    }
    for node in &snapshot.nodes {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(NODE_FACT, node.id.0)?,
            node,
            schema_version,
        )?;
    }
    for edge in &snapshot.edges {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(EDGE_FACT, edge.id.0)?,
            edge,
            schema_version,
        )?;
    }
    Ok(mutations.into_values().collect())
}

pub(super) fn delta_tree_mutations(
    connection: &Connection,
    delta: &GraphDelta,
    parent_sequence: Option<i64>,
    schema_version: u32,
    cascaded_edge_ids: Option<&BTreeSet<syntaxmesh_core::EdgeId>>,
) -> Result<Vec<PersistentFactMutation>, StoreError> {
    let mut mutations = BTreeMap::new();
    if let Some(edge_ids) = cascaded_edge_ids {
        for edge_id in edge_ids {
            add_tree_mutation(&mut mutations, tree_key(EDGE_FACT, edge_id.0)?, None);
        }
    } else if let Some(sequence) = parent_sequence {
        for removed_nodes in delta.remove_nodes.chunks(INCIDENT_EDGE_NODE_BATCH_SIZE) {
            let values = (1..=removed_nodes.len())
                .map(|parameter| format!("(?{parameter})"))
                .collect::<Vec<_>>()
                .join(", ");
            let fact_kind_parameter = removed_nodes.len().checked_add(1).ok_or_else(|| {
                StoreError::Backend("incident-edge parameter count overflow".to_owned())
            })?;
            let sequence_parameter = fact_kind_parameter.checked_add(1).ok_or_else(|| {
                StoreError::Backend("incident-edge parameter count overflow".to_owned())
            })?;
            let sql = format!(
                "WITH removed(node_id) AS (VALUES {values}) \
                 SELECT fact.fact_id, fact.payload FROM removed CROSS JOIN syntaxmesh_fact_versions AS fact INDEXED BY syntaxmesh_fact_versions_source_idx \
                 WHERE fact.fact_kind = ?{fact_kind_parameter} AND fact.source_id = removed.node_id \
                   AND fact.valid_from_sequence <= ?{sequence_parameter} \
                   AND (fact.valid_until_sequence IS NULL OR fact.valid_until_sequence > ?{sequence_parameter}) \
                 UNION ALL \
                 SELECT fact.fact_id, fact.payload FROM removed CROSS JOIN syntaxmesh_fact_versions AS fact INDEXED BY syntaxmesh_fact_versions_target_idx \
                 WHERE fact.fact_kind = ?{fact_kind_parameter} AND fact.target_id = removed.node_id \
                   AND fact.valid_from_sequence <= ?{sequence_parameter} \
                   AND (fact.valid_until_sequence IS NULL OR fact.valid_until_sequence > ?{sequence_parameter})"
            );
            let removed_node_ids = removed_nodes.iter().copied().collect::<BTreeSet<_>>();
            let mut parameters = removed_nodes
                .iter()
                .map(|node_id| SqlValue::Blob(node_id.0.0.to_vec()))
                .collect::<Vec<_>>();
            parameters.push(SqlValue::Integer(EDGE_FACT));
            parameters.push(SqlValue::Integer(sequence));
            let mut statement = connection.prepare(&sql).map_err(sql_error)?;
            let rows = statement
                .query_map(params_from_iter(parameters), |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(sql_error)?;
            for row in rows {
                let (id, payload) = row.map_err(sql_error)?;
                let edge: Edge = super::decode(&payload)?;
                let edge_id = stable_id(&id)?;
                if edge.id.0 != edge_id
                    || (!removed_node_ids.contains(&edge.source)
                        && !removed_node_ids.contains(&edge.target))
                {
                    return Err(StoreError::Integrity(
                        "incident temporal edge identity or endpoint is invalid".to_owned(),
                    ));
                }
                add_tree_mutation(&mut mutations, tree_key(EDGE_FACT, edge_id)?, None);
            }
        }
    }
    for file_id in &delta.removed_files {
        add_tree_mutation(&mut mutations, tree_key(FILE_FACT, file_id.0)?, None);
    }
    for edge_id in &delta.remove_edges {
        add_tree_mutation(&mut mutations, tree_key(EDGE_FACT, edge_id.0)?, None);
    }
    for node_id in &delta.remove_nodes {
        add_tree_mutation(&mut mutations, tree_key(NODE_FACT, node_id.0)?, None);
    }
    for file in &delta.changed_files {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(FILE_FACT, file.file_id.0)?,
            file,
            schema_version,
        )?;
    }
    for item in &delta.upsert_provenance {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(PROVENANCE_FACT, item.id.0)?,
            item,
            schema_version,
        )?;
    }
    for node in &delta.upsert_nodes {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(NODE_FACT, node.id.0)?,
            node,
            schema_version,
        )?;
    }
    for edge in &delta.upsert_edges {
        add_tree_fact_mutation(
            &mut mutations,
            tree_key(EDGE_FACT, edge.id.0)?,
            edge,
            schema_version,
        )?;
    }
    Ok(mutations.into_values().collect())
}

fn tree_key(fact_kind: i64, fact_id: StableId) -> Result<PersistentFactKey, StoreError> {
    let fact_kind = u8::try_from(fact_kind)
        .map_err(|error| StoreError::Integrity(format!("invalid temporal fact kind: {error}")))?;
    Ok(PersistentFactKey { fact_kind, fact_id })
}

fn add_tree_mutation(
    mutations: &mut BTreeMap<PersistentFactKey, PersistentFactMutation>,
    key: PersistentFactKey,
    value: Option<Vec<u8>>,
) {
    let mutation = value.map_or(PersistentFactMutation::Remove { key }, |value| {
        PersistentFactMutation::Upsert { key, value }
    });
    mutations.insert(key, mutation);
}

fn add_tree_fact_mutation<T: serde::Serialize>(
    mutations: &mut BTreeMap<PersistentFactKey, PersistentFactMutation>,
    key: PersistentFactKey,
    fact: &T,
    schema_version: u32,
) -> Result<(), StoreError> {
    let value = encode_tree_fact(fact, schema_version)?;
    let mutation = if schema_version == 1 {
        PersistentFactMutation::Upsert { key, value }
    } else {
        let canonical_value = serde_json::to_vec(fact).map_err(|error| {
            StoreError::Backend(format!("encode canonical tree commitment: {error}"))
        })?;
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

pub(super) fn read_parent_sequence(
    connection: &Connection,
    generation: GenerationId,
) -> Result<i64, StoreError> {
    connection
        .query_row(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| {
            StoreError::Integrity("generation parent is missing from retained history".to_owned())
        })
}

pub(super) fn read_persistent_root(
    connection: &Connection,
    generation: GenerationId,
) -> Result<Option<StableId>, StoreError> {
    let root = connection
        .query_row(
            "SELECT root_id FROM syntaxmesh_temporal_roots WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| {
            StoreError::Integrity("generation is missing its persistent fact root".to_owned())
        })?;
    root.as_deref().map(stable_id).transpose()
}

fn apply_persistent_mutations(
    connection: &Connection,
    old_root: Option<StableId>,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, StoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let profile_enabled = std::env::var_os("SYNTAXMESH_SQLITE_PROFILE").is_some();
    #[cfg(feature = "benchmark-instrumentation")]
    let mutation_started = profile_enabled.then(Instant::now);
    #[cfg(feature = "benchmark-instrumentation")]
    let mut page_load_elapsed = Duration::ZERO;
    #[cfg(feature = "benchmark-instrumentation")]
    let mut page_loads = 0_usize;
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
                    #[cfg(feature = "benchmark-instrumentation")]
                    let page_load_started = profile_enabled.then(Instant::now);
                    let payload = connection
                        .query_row(
                            "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                            [id.0.as_slice()],
                            |row| row.get::<_, Vec<u8>>(0),
                        )
                        .optional()
                        .map_err(sql_error)?
                        .ok_or_else(|| {
                            StoreError::Integrity(
                                "persistent fact root references a missing page".to_owned(),
                            )
                        })?;
                    let node: PersistentFactNode = super::decode(&payload)?;
                    cache
                        .insert_loaded(id, node)
                        .map_err(|error| persistent_tree_error(&error))?;
                    #[cfg(feature = "benchmark-instrumentation")]
                    if let Some(started) = page_load_started {
                        page_loads = page_loads.saturating_add(1);
                        page_load_elapsed = page_load_elapsed.saturating_add(started.elapsed());
                    }
                }
                Err(error) => return Err(persistent_tree_error(&error)),
            }
        }
    };
    #[cfg(feature = "benchmark-instrumentation")]
    let mutation_elapsed = mutation_started.map(|started| started.elapsed());
    #[cfg(feature = "benchmark-instrumentation")]
    let reachable_started = profile_enabled.then(Instant::now);
    let dirty_pages = cache.take_reachable_dirty(new_root);
    #[cfg(feature = "benchmark-instrumentation")]
    let reachable_elapsed = reachable_started.map(|started| started.elapsed());
    #[cfg(feature = "benchmark-instrumentation")]
    let dirty_page_count = dirty_pages.len();
    #[cfg(feature = "benchmark-instrumentation")]
    let mut encode_elapsed = Duration::ZERO;
    #[cfg(feature = "benchmark-instrumentation")]
    let mut insert_elapsed = Duration::ZERO;
    let mut insert_page = connection
        .prepare(
            "INSERT OR IGNORE INTO syntaxmesh_temporal_tree_pages (page_id, left_page, right_page, payload) VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(sql_error)?;
    for (id, node) in dirty_pages {
        #[cfg(feature = "benchmark-instrumentation")]
        let encode_started = profile_enabled.then(Instant::now);
        let payload = encode(&node)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = encode_started {
            encode_elapsed = encode_elapsed.saturating_add(started.elapsed());
        }
        #[cfg(feature = "benchmark-instrumentation")]
        let insert_started = profile_enabled.then(Instant::now);
        insert_page
            .execute(params![
                id.0.as_slice(),
                node.left.map(|child| child.0.to_vec()),
                node.right.map(|child| child.0.to_vec()),
                payload,
            ])
            .map_err(sql_error)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = insert_started {
            insert_elapsed = insert_elapsed.saturating_add(started.elapsed());
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    if profile_enabled {
        for (stage, elapsed) in [
            ("persistent_tree_mutation_work", mutation_elapsed),
            ("persistent_tree_page_load", Some(page_load_elapsed)),
            ("persistent_tree_reachable_dirty", reachable_elapsed),
            ("persistent_tree_page_encode", Some(encode_elapsed)),
            ("persistent_tree_page_insert", Some(insert_elapsed)),
        ] {
            if let Some(elapsed) = elapsed {
                eprintln!(
                    "sqlite_stage stage={stage} elapsed_us={}",
                    elapsed.as_micros()
                );
            }
        }
        eprintln!(
            "sqlite_tree_metrics loaded_pages={} cache_page_lookups={} dirty_pages={dirty_page_count}",
            page_loads,
            cache.page_lookup_count(),
        );
    }
    Ok(new_root)
}

pub(super) fn persistent_tree_error(error: &PersistentFactTreeError) -> StoreError {
    StoreError::Integrity(format!("persistent fact tree error: {error:?}"))
}

pub(super) fn publish_persistent_root(
    connection: &Connection,
    sequence: i64,
    generation: GenerationId,
    parent: Option<GenerationId>,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, StoreError> {
    if parent.is_none() && sequence != 1 {
        return Err(StoreError::Integrity(
            "non-initial generation has no persistent-root parent".to_owned(),
        ));
    }
    publish_persistent_root_from_base(connection, sequence, generation, parent, mutations)
}

/// Publishes a complete snapshot as a fresh tree root, used when changing the
/// persistent-root encoding. Unlike an incremental publication, this does not
/// inherit the previous root and is valid after generation one.
pub(super) fn publish_rebuilt_persistent_root(
    connection: &Connection,
    sequence: i64,
    generation: GenerationId,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, StoreError> {
    publish_persistent_root_from_base(connection, sequence, generation, None, mutations)
}

pub(super) fn read_incidence_root(
    connection: &Connection,
    generation: GenerationId,
) -> Result<Option<StableId>, StoreError> {
    let root = connection
        .query_row(
            "SELECT root_id FROM syntaxmesh_temporal_incidence_roots WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| {
            StoreError::Integrity("generation is missing its persistent incidence root".to_owned())
        })?;
    root.as_deref().map(stable_id).transpose()
}

pub(super) fn historical_search_nodes(
    connection: &Connection,
    generation: GenerationId,
    text: &str,
    limit: usize,
) -> Result<Vec<Node>, StoreError> {
    let text = text.to_lowercase();
    historical_nodes_matching(connection, generation, None, limit, |node| {
        node.name.to_lowercase().contains(&text)
    })
}

pub(super) fn historical_nodes_page(
    connection: &Connection,
    generation: GenerationId,
    after: Option<NodeId>,
    limit: usize,
) -> Result<syntaxmesh_store::HistoricalNodePage, StoreError> {
    if limit == 0 || limit > syntaxmesh_store::MAX_HISTORICAL_NODE_PAGE_SIZE {
        return Err(StoreError::InvalidPageLimit);
    }
    let mut items = historical_nodes_matching(
        connection,
        generation,
        after,
        limit.saturating_add(1),
        |_node| true,
    )?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(syntaxmesh_store::HistoricalNodePage { items, has_more })
}

fn historical_nodes_matching(
    connection: &Connection,
    generation: GenerationId,
    after: Option<NodeId>,
    limit: usize,
    predicate: impl Fn(&Node) -> bool,
) -> Result<Vec<Node>, StoreError> {
    let entry =
        read_generation_history_entry(connection, generation)?.ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })?;
    let root = read_persistent_root(connection, generation)?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    let after = PersistentFactKey {
        fact_kind: u8::try_from(if after.is_some() {
            NODE_FACT
        } else {
            NODE_FACT - 1
        })
        .map_err(|error| StoreError::Integrity(error.to_string()))?,
        fact_id: after.map_or(StableId([u8::MAX; 32]), |node| node.0),
    };
    let mut walker = syntaxmesh_store::PersistentFactTreeRangeWalker::new(root, Some(after));
    let mut cache = PersistentFactTreeCache::default();
    let mut matches = Vec::new();
    loop {
        match walker.next(&cache) {
            Ok(Some(page)) => {
                if i64::from(page.key.fact_kind) != NODE_FACT {
                    break;
                }
                let node: Node =
                    decode_tree_fact(page.value_payload(), entry.manifest.schema_version)?;
                if node.id.0 != page.key.fact_id {
                    return Err(StoreError::Integrity(
                        "historical search node identity differs from tree key".to_owned(),
                    ));
                }
                if predicate(&node) {
                    matches.push(node);
                }
                cache = PersistentFactTreeCache::default();
                if matches.len() == limit {
                    break;
                }
            }
            Ok(None) => break,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                let payload = connection
                    .query_row(
                        "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                        [id.0.as_slice()],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| {
                        StoreError::Integrity(
                            "historical search references a missing page".to_owned(),
                        )
                    })?;
                cache
                    .insert_loaded(id, decode(&payload)?)
                    .map_err(|error| persistent_tree_error(&error))?;
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        }
    }
    Ok(matches)
}

pub(super) fn historical_incident_edge_page(
    connection: &Connection,
    generation: GenerationId,
    endpoint: NodeId,
    direction: EdgeDirection,
    after: Option<EdgeId>,
    limit: usize,
) -> Result<HistoricalEdgePage, StoreError> {
    let sequence = super::history_sequence(connection, generation)?;
    #[cfg(feature = "benchmark-instrumentation")]
    let mut sql_read_statements = 1_usize;
    if limit == 0 || limit > MAX_HISTORICAL_EDGE_PAGE_SIZE {
        return Err(StoreError::InvalidPageLimit);
    }
    let incidence_root = read_incidence_root(connection, generation)?;
    #[cfg(feature = "benchmark-instrumentation")]
    {
        sql_read_statements = sql_read_statements.saturating_add(1);
    }
    let mut cache = PersistentFactTreeCache::default();
    let outgoing = direction == EdgeDirection::Outgoing;
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
                let payload = connection
                    .query_row(
                        "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                        [id.0.as_slice()],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| {
                        StoreError::Integrity("incidence root references a missing page".to_owned())
                    })?;
                cache
                    .insert_loaded(id, decode(&payload)?)
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
        StoreError::Backend("historical edge page limit overflows usize".to_owned())
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
                let payload = connection
                    .query_row(
                        "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                        [id.0.as_slice()],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| {
                        StoreError::Integrity("incidence set references a missing page".to_owned())
                    })?;
                cache
                    .insert_loaded(id, decode(&payload)?)
                    .map_err(|error| persistent_tree_error(&error))?;
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        }
    }
    let has_more = edge_ids.len() > limit;
    let mut items = Vec::with_capacity(edge_ids.len().min(limit));
    for edge_id in edge_ids.into_iter().take(limit) {
        #[cfg(feature = "benchmark-instrumentation")]
        {
            sql_read_statements = sql_read_statements.saturating_add(1);
        }
        let mut edges = read_many_with_params::<Edge, _>(
            connection,
            "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence = (SELECT MAX(valid_from_sequence) FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence <= ?3) AND (valid_until_sequence IS NULL OR valid_until_sequence > ?3) LIMIT 1",
            params![EDGE_FACT, edge_id.0.0.as_slice(), sequence],
        )?;
        let edge = edges.pop().ok_or_else(|| {
            StoreError::Integrity(format!(
                "incidence index refers to inactive historical edge {edge_id:?}"
            ))
        })?;
        if edge.id != edge_id
            || (outgoing && edge.source != endpoint)
            || (!outgoing && edge.target != endpoint)
        {
            return Err(StoreError::Integrity(
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

pub(super) fn publish_incidence_root(
    connection: &Connection,
    sequence: i64,
    generation: GenerationId,
    parent: Option<GenerationId>,
    changes: &[IncidenceMutation],
) -> Result<Option<StableId>, StoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let profile_enabled = std::env::var_os("SYNTAXMESH_SQLITE_PROFILE").is_some();
    let old_root = parent
        .map(|parent_generation| read_incidence_root(connection, parent_generation))
        .transpose()?
        .flatten();
    let mut cache = PersistentFactTreeCache::default();
    #[cfg(feature = "benchmark-instrumentation")]
    let apply_started = profile_enabled.then(Instant::now);
    #[cfg(feature = "benchmark-instrumentation")]
    let mut page_load_elapsed = Duration::ZERO;
    #[cfg(feature = "benchmark-instrumentation")]
    let mut page_loads = 0_usize;
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
                    #[cfg(feature = "benchmark-instrumentation")]
                    let page_load_started = profile_enabled.then(Instant::now);
                    let payload = connection
                        .query_row(
                            "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                            [id.0.as_slice()],
                            |row| row.get::<_, Vec<u8>>(0),
                        )
                        .optional()
                        .map_err(sql_error)?
                        .ok_or_else(|| {
                            StoreError::Integrity(
                                "persistent incidence root references a missing page".to_owned(),
                            )
                        })?;
                    let node: PersistentFactNode = super::decode(&payload)?;
                    cache
                        .insert_loaded(id, node)
                        .map_err(|error| persistent_tree_error(&error))?;
                    #[cfg(feature = "benchmark-instrumentation")]
                    if let Some(started) = page_load_started {
                        page_loads = page_loads.saturating_add(1);
                        page_load_elapsed = page_load_elapsed.saturating_add(started.elapsed());
                    }
                }
                Err(error) => return Err(persistent_tree_error(&error)),
            }
        }
        let root = apply.root();
        let dirty_roots = apply.dirty_roots().collect::<Vec<_>>();
        (root, dirty_roots)
    };
    #[cfg(feature = "benchmark-instrumentation")]
    let apply_elapsed = apply_started.map(|started| started.elapsed());
    #[cfg(feature = "benchmark-instrumentation")]
    let dirty_roots_started = profile_enabled.then(Instant::now);
    let dirty_pages = cache.take_reachable_dirty_roots(dirty_roots);
    #[cfg(feature = "benchmark-instrumentation")]
    let dirty_roots_elapsed = dirty_roots_started.map(|started| started.elapsed());
    #[cfg(feature = "benchmark-instrumentation")]
    let dirty_page_count = dirty_pages.len();
    #[cfg(feature = "benchmark-instrumentation")]
    let mut encode_elapsed = Duration::ZERO;
    #[cfg(feature = "benchmark-instrumentation")]
    let mut insert_elapsed = Duration::ZERO;
    let mut insert_page = connection
        .prepare(
            "INSERT OR IGNORE INTO syntaxmesh_temporal_tree_pages (page_id, left_page, right_page, payload) VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(sql_error)?;
    for (id, node) in dirty_pages {
        #[cfg(feature = "benchmark-instrumentation")]
        let encode_started = profile_enabled.then(Instant::now);
        let payload = encode(&node)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = encode_started {
            encode_elapsed = encode_elapsed.saturating_add(started.elapsed());
        }
        #[cfg(feature = "benchmark-instrumentation")]
        let insert_started = profile_enabled.then(Instant::now);
        insert_page
            .execute(params![
                id.0.as_slice(),
                node.left.map(|child| child.0.to_vec()),
                node.right.map(|child| child.0.to_vec()),
                payload,
            ])
            .map_err(sql_error)?;
        #[cfg(feature = "benchmark-instrumentation")]
        if let Some(started) = insert_started {
            insert_elapsed = insert_elapsed.saturating_add(started.elapsed());
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    let root_record_started = profile_enabled.then(Instant::now);
    connection
        .execute(
            "INSERT INTO syntaxmesh_temporal_incidence_roots (sequence, generation, root_id) VALUES (?1, ?2, ?3)",
            params![sequence, generation.0.0.as_slice(), root.map(|id| id.0.to_vec())],
        )
        .map_err(sql_error)?;
    #[cfg(feature = "benchmark-instrumentation")]
    let root_record_elapsed = root_record_started.map(|started| started.elapsed());
    #[cfg(feature = "benchmark-instrumentation")]
    if profile_enabled {
        for (stage, elapsed) in [
            ("persistent_incidence_apply", apply_elapsed),
            ("persistent_incidence_page_load", Some(page_load_elapsed)),
            ("persistent_incidence_reachable_dirty", dirty_roots_elapsed),
            ("persistent_incidence_page_encode", Some(encode_elapsed)),
            ("persistent_incidence_page_insert", Some(insert_elapsed)),
            (
                "persistent_incidence_root_record_write",
                root_record_elapsed,
            ),
        ] {
            if let Some(elapsed) = elapsed {
                eprintln!(
                    "sqlite_stage stage={stage} elapsed_us={}",
                    elapsed.as_micros()
                );
            }
        }
        eprintln!(
            "sqlite_incidence_metrics mutations={} loaded_pages={} cache_page_lookups={} dirty_pages={dirty_page_count}",
            changes.len(),
            page_loads,
            cache.page_lookup_count(),
        );
    }
    Ok(root)
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

pub(super) fn delta_incidence_changes(
    connection: &Connection,
    delta: &GraphDelta,
    cascaded_edge_ids: &BTreeSet<syntaxmesh_core::EdgeId>,
) -> Result<Vec<IncidenceMutation>, StoreError> {
    let mut removed = delta.remove_edges.iter().copied().collect::<BTreeSet<_>>();
    removed.extend(cascaded_edge_ids.iter().copied());
    removed.extend(delta.upsert_edges.iter().map(|edge| edge.id));
    let mut changes = Vec::new();
    let mut statement = connection
        .prepare("SELECT payload FROM syntaxmesh_edges WHERE id = ?1")
        .map_err(sql_error)?;
    for edge_id in removed {
        let payload = statement
            .query_row([edge_id.0.0.as_slice()], |row| row.get::<_, Vec<u8>>(0))
            .optional()
            .map_err(sql_error)?;
        let Some(payload) = payload else {
            continue;
        };
        let edge: Edge = decode(&payload)?;
        if edge.id != edge_id {
            return Err(StoreError::Integrity(
                "current edge row ID disagrees with payload during incidence update".to_owned(),
            ));
        }
        changes.push(IncidenceMutation {
            edge: edge.id,
            source: edge.source,
            target: edge.target,
            present: false,
        });
    }
    changes.extend(delta.upsert_edges.iter().map(|edge| IncidenceMutation {
        edge: edge.id,
        source: edge.source,
        target: edge.target,
        present: true,
    }));
    Ok(changes)
}

fn publish_persistent_root_from_base(
    connection: &Connection,
    sequence: i64,
    generation: GenerationId,
    parent: Option<GenerationId>,
    mutations: &[PersistentFactMutation],
) -> Result<Option<StableId>, StoreError> {
    let old_root = parent
        .map(|parent| read_persistent_root(connection, parent))
        .transpose()?
        .flatten();
    let root = apply_persistent_mutations(connection, old_root, mutations)?;
    connection
        .execute(
            "INSERT INTO syntaxmesh_temporal_roots (sequence, generation, root_id) VALUES (?1, ?2, ?3)",
            params![sequence, generation.0.0.as_slice(), root.map(|id| id.0.to_vec())],
        )
        .map_err(sql_error)?;
    Ok(root)
}

pub(super) fn read_temporal_snapshot(
    connection: &Connection,
    generation: GenerationId,
) -> Result<GraphSnapshot, StoreError> {
    let sequence = connection
        .query_row(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_manifest(connection)?.map(|manifest| manifest.generation),
        })?;
    let snapshot = GraphSnapshot {
        files: read_many_with_params::<syntaxmesh_core::FileVersion, _>(
            connection,
            "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id",
            params![FILE_FACT, sequence],
        )?,
        provenance: read_many_with_params::<Provenance, _>(
            connection,
            "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id",
            params![PROVENANCE_FACT, sequence],
        )?,
        nodes: read_many_with_params::<Node, _>(
            connection,
            "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id",
            params![NODE_FACT, sequence],
        )?,
        edges: read_many_with_params::<Edge, _>(
            connection,
            "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND valid_from_sequence <= ?2 AND (valid_until_sequence IS NULL OR valid_until_sequence > ?2) ORDER BY fact_id",
            params![EDGE_FACT, sequence],
        )?,
    };
    let entry =
        read_generation_history_entry(connection, generation)?.ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })?;
    if snapshot_root_for_schema(generation, &snapshot, entry.manifest.schema_version)?
        != entry.manifest.graph_root
    {
        return Err(StoreError::Integrity(
            "temporal snapshot root does not match generation manifest".to_owned(),
        ));
    }
    Ok(snapshot)
}

pub(super) fn read_persistent_snapshot(
    connection: &Connection,
    generation: GenerationId,
) -> Result<GraphSnapshot, StoreError> {
    let history_entry =
        read_generation_history_entry(connection, generation)?.ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_manifest(connection)?.map(|manifest| manifest.generation),
        })?;
    let schema_version = history_entry.manifest.schema_version;
    let root = read_persistent_root(connection, generation)?;
    let mut snapshot = GraphSnapshot::default();
    let mut cache = PersistentFactTreeCache::default();
    if let Some(root_id) = root {
        let mut statement = connection
            .prepare(
                "WITH RECURSIVE reachable(page_id) AS (\
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
                FROM reachable JOIN syntaxmesh_temporal_tree_pages AS page USING (page_id)",
            )
            .map_err(sql_error)?;
        let pages = statement
            .query_map([root_id.0.as_slice()], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(sql_error)?;
        for page in pages {
            let (id, payload) = page.map_err(sql_error)?;
            let node: PersistentFactNode = decode(&payload)?;
            cache
                .insert_loaded(stable_id(&id)?, node)
                .map_err(|error| persistent_tree_error(&error))?;
        }
    }
    let mut walker = PersistentFactTreeWalker::new(root);
    loop {
        let (id, page) = match walker.next(&cache) {
            Ok(Some(page)) => page,
            Ok(None) => break,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                return Err(StoreError::Integrity(format!(
                    "persistent fact root references missing page {id:?}"
                )));
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        };
        if page.id() != id {
            return Err(StoreError::Integrity(
                "persistent fact page content address mismatch".to_owned(),
            ));
        }
        match page.key.fact_kind {
            kind if i64::from(kind) == FILE_FACT => {
                let fact: syntaxmesh_core::FileVersion =
                    decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.file_id.0 != page.key.fact_id {
                    return Err(StoreError::Integrity(
                        "persistent file key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.files.push(fact);
            }
            kind if i64::from(kind) == PROVENANCE_FACT => {
                let fact: Provenance = decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.id.0 != page.key.fact_id {
                    return Err(StoreError::Integrity(
                        "persistent provenance key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.provenance.push(fact);
            }
            kind if i64::from(kind) == NODE_FACT => {
                let fact: Node = decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.id.0 != page.key.fact_id {
                    return Err(StoreError::Integrity(
                        "persistent node key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.nodes.push(fact);
            }
            kind if i64::from(kind) == EDGE_FACT => {
                let fact: Edge = decode_tree_fact(page.value_payload(), schema_version)?;
                if fact.id.0 != page.key.fact_id {
                    return Err(StoreError::Integrity(
                        "persistent edge key differs from payload identity".to_owned(),
                    ));
                }
                snapshot.edges.push(fact);
            }
            _ => {
                return Err(StoreError::Integrity(
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

pub(super) fn read_graph_at(
    connection: &Connection,
    generation: GenerationId,
) -> Result<GraphSnapshot, StoreError> {
    let snapshot = read_persistent_snapshot(connection, generation)?;
    let entry =
        read_generation_history_entry(connection, generation)?.ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_manifest(connection)?.map(|manifest| manifest.generation),
        })?;
    if snapshot_root_for_schema(generation, &snapshot, entry.manifest.schema_version)?
        != entry.manifest.graph_root
    {
        return Err(StoreError::Integrity(
            "persistent historical graph root differs from generation manifest".to_owned(),
        ));
    }
    Ok(snapshot)
}

fn snapshot_root(
    generation: GenerationId,
    snapshot: &GraphSnapshot,
) -> Result<[u8; 32], StoreError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&generation.0.0);
    for node in &snapshot.nodes {
        hasher.update(
            &serde_json::to_vec(node).map_err(|error| StoreError::Backend(error.to_string()))?,
        );
    }
    for edge in &snapshot.edges {
        hasher.update(
            &serde_json::to_vec(edge).map_err(|error| StoreError::Backend(error.to_string()))?,
        );
    }
    Ok(*hasher.finalize().as_bytes())
}

pub(super) fn snapshot_root_for_schema(
    generation: GenerationId,
    snapshot: &GraphSnapshot,
    schema_version: u32,
) -> Result<[u8; 32], StoreError> {
    match schema_version {
        1 => snapshot_root(generation, snapshot),
        2 => graph_snapshot_root_v2(generation, snapshot),
        version => Err(StoreError::Unsupported(format!(
            "unsupported generation root schema version {version}"
        ))),
    }
}
