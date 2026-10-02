use super::*;
mod node_scan;
mod path_aliases;
use syntaxmesh_core::{
    ChangeSet, ChangeSetKind, ConsequenceEdge, ConsequenceEdgeId, FactVersionRef,
};
use syntaxmesh_core::{EvidenceClass, FileId, NodeKind, ProvenanceId, RelationKind};

pub(super) fn open_migrated(path: impl AsRef<Path>) -> Result<TursoGraphStore, TursoStoreError> {
    TursoGraphStore::migrate(path.as_ref())?;
    TursoGraphStore::open(path)
}

fn schema_v1_test_root(
    generation: GenerationId,
    nodes: &[Node],
    edges: &[Edge],
) -> Result<[u8; 32], TursoStoreError> {
    let mut bytes = Vec::new();
    let mut nodes = nodes.to_vec();
    nodes.sort_by_key(|node| node.id);
    let mut edges = edges.to_vec();
    edges.sort_by_key(|edge| edge.id);
    for node in nodes {
        bytes.extend_from_slice(&serde_json::to_vec(&node).map_err(|error| {
            TursoStoreError::Snapshot(format!("serialize schema-v1 test node: {error}"))
        })?);
    }
    for edge in edges {
        bytes.extend_from_slice(&serde_json::to_vec(&edge).map_err(|error| {
            TursoStoreError::Snapshot(format!("serialize schema-v1 test edge: {error}"))
        })?);
    }
    Ok(*blake3::hash(&[generation.0.0.as_slice(), bytes.as_slice()].concat()).as_bytes())
}

#[test]
fn incidence_roots_publish_and_backfill_through_registered_migration() -> Result<(), TursoStoreError>
{
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-incidence-backfill.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"incidence-repository"]);
    let worktree = WorktreeId::derive(&[b"incidence-worktree"]);
    let generation = GenerationId::derive(&[b"incidence-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"incidence-provenance"]),
        producer_namespace: "incidence-test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let source = Node {
        id: NodeId::derive(&[b"incidence-source"]),
        kind: NodeKind::Function,
        name: "source".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let target = Node {
        id: NodeId::derive(&[b"incidence-target"]),
        name: "target".to_owned(),
        ..source.clone()
    };
    let edge = Edge {
        id: EdgeId::derive(&[b"incidence-edge"]),
        source: source.id,
        target: target.id,
        relation: RelationKind::Calls,
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut store = open_migrated(&path)?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"incidence-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![source, target],
        upsert_edges: vec![edge],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let before = store.runtime.block_on(async {
        let mut roots = store
            .connection
            .query(
                "SELECT COUNT(*), COUNT(root_id) FROM syntaxmesh_temporal_incidence_roots",
                (),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let row = roots
            .next()
            .await
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?
            .ok_or_else(|| {
                TursoStoreError::Snapshot("incidence-root count is missing".to_owned())
            })?;
        Ok::<_, TursoStoreError>((
            row.get::<i64>(0)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
            row.get::<i64>(1)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
        ))
    })?;
    if before != (1, 1) {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso publication did not persist a non-empty incidence root: {before:?}"
        )));
    }
    store.runtime.block_on(async {
            store.connection.execute_batch(
                "DELETE FROM syntaxmesh_temporal_incidence_roots; DELETE FROM syntaxmesh_schema_migrations WHERE version > 21; UPDATE syntaxmesh_schema SET version = 21 WHERE id = 1;",
            ).await.map_err(|error| TursoStoreError::Backend(format!("prepare incidence migration fixture: {error}")))
        })?;
    drop(store);
    TursoGraphStore::migrate(&path)?;
    let reopened = TursoGraphStore::open(&path)?;
    let after = reopened.runtime.block_on(async {
        let mut roots = reopened
            .connection
            .query(
                "SELECT COUNT(*), COUNT(root_id) FROM syntaxmesh_temporal_incidence_roots",
                (),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let row = roots
            .next()
            .await
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?
            .ok_or_else(|| {
                TursoStoreError::Snapshot("backfilled incidence-root count is missing".to_owned())
            })?;
        Ok::<_, TursoStoreError>((
            row.get::<i64>(0)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
            row.get::<i64>(1)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
        ))
    })?;
    if after != (1, 1) || reopened.generation_history()?.len() != 1 {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso migration failed to backfill incidence root from retained history: {after:?}"
        )));
    }
    drop(reopened);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove incidence migration fixture: {error}"))
    })?;
    Ok(())
}

#[test]
fn turso_rebuilds_v2_generation_after_retained_v1_root() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-mixed-root-version.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"mixed-root-repository"]);
    let worktree = WorktreeId::derive(&[b"mixed-root-worktree"]);
    let first = GenerationId::derive(&[b"mixed-root-generation-one"]);
    let second = GenerationId::derive(&[b"mixed-root-generation-two"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"mixed-root-provenance"]),
        producer_namespace: "mixed-root-test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let first_node = Node {
        id: NodeId::derive(&[b"mixed-root-node-one"]),
        kind: NodeKind::Function,
        name: "first".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let second_node = Node {
        id: NodeId::derive(&[b"mixed-root-node-two"]),
        name: "second".to_owned(),
        ..first_node.clone()
    };
    let mut store = open_migrated(&path)?;
    let first_manifest = store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"mixed-root-run-one"]),
        expected_base: None,
        next_generation: first,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![first_node],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let first_snapshot = GraphSnapshot {
        files: store.files(first)?,
        provenance: store.provenance(first)?,
        nodes: store.nodes(first)?,
        edges: store.edges(first)?,
    };
    let legacy_manifest = GenerationManifest {
        schema_version: 1,
        graph_root: schema_v1_test_root(first, &first_snapshot.nodes, &first_snapshot.edges)?,
        ..first_manifest
    };
    let mut history = store.generation_history()?;
    let first_entry = history.first_mut().ok_or_else(|| {
        TursoStoreError::Snapshot("first generation has no history entry".to_owned())
    })?;
    first_entry.manifest = legacy_manifest.clone();
    let history_payload = bincode::serialize(first_entry)
        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
    let manifest_payload = bincode::serialize(&legacy_manifest)
        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
    store.runtime.block_on(async {
            store.connection.execute(
                "UPDATE syntaxmesh_manifest SET payload = ?1 WHERE id = 1",
                [manifest_payload.clone()],
            ).await.map_err(|error| TursoStoreError::Backend(format!("write legacy root manifest: {error}")))?;
            store.connection.execute(
                "UPDATE syntaxmesh_generation_history SET payload = ?1 WHERE sequence = 1",
                [history_payload],
            ).await.map_err(|error| TursoStoreError::Backend(format!("write legacy root history: {error}")))?;
            store.connection.execute(
                "UPDATE syntaxmesh_graph_checkpoints SET manifest = ?1 WHERE sequence = 1",
                [manifest_payload],
            ).await.map_err(|error| TursoStoreError::Backend(format!("write legacy root checkpoint: {error}")))?;
            store.connection.execute_batch(
                "DELETE FROM syntaxmesh_temporal_roots; DELETE FROM syntaxmesh_temporal_tree_pages;",
            ).await.map_err(|error| TursoStoreError::Backend(format!("clear v2 fixture root: {error}")))?;
            let mutations = snapshot_tree_mutations(&first_snapshot, 1)?;
            publish_persistent_root(&store.connection, 1, first, None, &mutations).await?;
            Ok::<(), TursoStoreError>(())
        })?;
    drop(store);

    let mut reopened = TursoGraphStore::open(&path)?;
    if reopened.manifest(first)? != legacy_manifest
        || reopened.historical_snapshot(first)? != first_snapshot
    {
        return Err(TursoStoreError::Snapshot(
            "Turso did not reopen the retained schema-v1 generation".to_owned(),
        ));
    }
    let second_manifest = reopened.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"mixed-root-run-two"]),
        expected_base: Some(first),
        next_generation: second,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: vec![second_node],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let second_snapshot = GraphSnapshot {
        files: reopened.files(second)?,
        provenance: reopened.provenance(second)?,
        nodes: reopened.nodes(second)?,
        edges: reopened.edges(second)?,
    };
    let history_versions = reopened
        .generation_history()?
        .iter()
        .map(|entry| entry.manifest.schema_version)
        .collect::<Vec<_>>();
    if second_manifest.schema_version != 2
        || history_versions != [1, 2]
        || reopened.historical_snapshot(first)? != first_snapshot
        || reopened.historical_snapshot(second)? != second_snapshot
        || syntaxmesh_store::graph_snapshot_root_v2(second, &second_snapshot)?
            != second_manifest.graph_root
    {
        return Err(TursoStoreError::Snapshot(
            "Turso did not preserve the mixed v1/v2 generation roots".to_owned(),
        ));
    }
    drop(reopened);
    let restarted = TursoGraphStore::open(&path)?;
    let old_snapshot = restarted.historical_snapshot(first)?;
    let new_snapshot = restarted.historical_snapshot(second)?;
    drop(restarted);
    std::fs::remove_file(&path)
        .map_err(|error| TursoStoreError::Backend(format!("remove mixed-root fixture: {error}")))?;
    if old_snapshot != first_snapshot || new_snapshot != second_snapshot {
        return Err(TursoStoreError::Snapshot(
            "Turso mixed v1/v2 roots did not survive a second restart".to_owned(),
        ));
    }
    Ok(())
}

async fn explain_node_resolution_query(
    connection: &turso::Connection,
) -> Result<String, TursoStoreError> {
    let mut rows = connection
            .query(
                "EXPLAIN QUERY PLAN SELECT payload FROM syntaxmesh_nodes WHERE node_kind IN (3, 15, 16, 17) ORDER BY id",
                (),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("explain module inventory query: {error}")))?;
    let mut details = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read module inventory query plan: {error}"))
    })? {
        details.push(row.get::<String>(3).map_err(|error| {
            TursoStoreError::Backend(format!("decode module inventory query plan: {error}"))
        })?);
    }
    Ok(details.join("; "))
}

#[test]
fn consequence_range_query_uses_both_endpoint_indexes() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-consequence-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let plan = store.runtime.block_on(async {
        let values = vec![
            turso::Value::Integer(10),
            turso::Value::Integer(2),
            turso::Value::Text("change_event".to_owned()),
            turso::Value::Blob(vec![1_u8; 32]),
            turso::Value::Null,
            turso::Value::Null,
            turso::Value::Text("change_event".to_owned()),
            turso::Value::Blob(vec![1_u8; 32]),
            turso::Value::Null,
            turso::Value::Null,
            turso::Value::Null,
            turso::Value::Integer(11),
        ];
        let mut rows = store
            .connection
            .query(
                &format!("EXPLAIN QUERY PLAN {CONSEQUENCE_RANGE_QUERY}"),
                values,
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("explain consequence range query: {error}"))
            })?;
        let mut details = Vec::new();
        while let Some(row) = rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read consequence range query plan: {error}"))
        })? {
            match row.get_value(3).map_err(|error| {
                TursoStoreError::Backend(format!("decode consequence query plan: {error}"))
            })? {
                turso::Value::Text(detail) => details.push(detail),
                other @ (turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Blob(_)) => {
                    return Err(TursoStoreError::Snapshot(format!(
                        "consequence query plan detail is not text: {other:?}"
                    )));
                }
            }
        }
        Ok::<_, TursoStoreError>(details.join("; "))
    })?;
    drop(store);
    drop(std::fs::remove_file(&path));

    if !plan.contains("syntaxmesh_consequence_source_idx")
        || !plan.contains("syntaxmesh_consequence_target_idx")
    {
        return Err(TursoStoreError::Snapshot(format!(
            "consequence range query does not use both endpoint indexes: {plan}"
        )));
    }
    Ok(())
}

#[test]
fn fact_history_page_uses_composite_history_index() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-fact-history-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let plan = store.runtime.block_on(async {
        let values = vec![
            turso::Value::Blob(vec![1_u8; 32]),
            turso::Value::Integer(FILE_FACT),
            turso::Value::Blob(vec![1_u8; 32]),
            turso::Value::Integer(1),
            turso::Value::Integer(3),
        ];
        let mut rows = store
            .connection
            .query(
                &format!("EXPLAIN QUERY PLAN {FACT_HISTORY_AFTER_PAGE_QUERY}"),
                values,
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("explain fact-history page: {error}"))
            })?;
        let mut details = Vec::new();
        while let Some(row) = rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read fact-history query plan: {error}"))
        })? {
            match row.get_value(3).map_err(|error| {
                TursoStoreError::Backend(format!("decode fact-history plan: {error}"))
            })? {
                turso::Value::Text(detail) => details.push(detail),
                other @ (turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Blob(_)) => {
                    return Err(TursoStoreError::Snapshot(format!(
                        "fact-history plan detail is not text: {other:?}"
                    )));
                }
            }
        }
        Ok::<_, TursoStoreError>(details.join("; "))
    })?;
    drop(store);
    drop(std::fs::remove_file(&path));
    if !plan.contains("syntaxmesh_fact_versions_identity_time_idx")
        && !plan.contains("sqlite_autoindex_syntaxmesh_fact_versions_1")
    {
        return Err(TursoStoreError::Snapshot(format!(
            "fact-history cursor query does not use the composite identity/sequence index: {plan}"
        )));
    }
    Ok(())
}

#[test]
fn generation_change_page_seeks_sequence_primary_key() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-generation-change-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let plan = store.runtime.block_on(async {
            let mut rows = store
                .connection
                .query(
                    "EXPLAIN QUERY PLAN SELECT sequence, payload FROM syntaxmesh_generation_history WHERE sequence > ?1 AND sequence <= ?2 ORDER BY sequence LIMIT ?3",
                    (1_i64, 4_i64, 2_i64),
                )
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("explain generation change page: {error}"))
                })?;
            let mut details = Vec::new();
            while let Some(row) = rows.next().await.map_err(|error| {
                TursoStoreError::Backend(format!("read generation change query plan: {error}"))
            })? {
                match row.get_value(3).map_err(|error| {
                    TursoStoreError::Backend(format!("decode generation change query plan: {error}"))
                })? {
                    turso::Value::Text(detail) => details.push(detail),
                    other @ (turso::Value::Null
                    | turso::Value::Integer(_)
                    | turso::Value::Real(_)
                    | turso::Value::Blob(_)) => {
                        return Err(TursoStoreError::Snapshot(format!(
                            "generation change query plan detail is not text: {other:?}"
                        )));
                    }
                }
            }
            Ok::<_, TursoStoreError>(details.join("; "))
        })?;
    drop(store);
    drop(std::fs::remove_file(&path));
    if !plan.contains("SEARCH syntaxmesh_generation_history USING INTEGER PRIMARY KEY") {
        return Err(TursoStoreError::Snapshot(format!(
            "generation change page does not seek its sequence primary key: {plan}"
        )));
    }
    Ok(())
}

#[test]
fn fact_version_generation_change_uses_start_and_end_indexes() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-fact-version-change-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let plan = store.runtime.block_on(async {
            let mut rows = store
                .connection
                .query(
                    "EXPLAIN QUERY PLAN SELECT valid_from_sequence FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence = ?3 UNION ALL SELECT valid_from_sequence FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_identity_end_idx WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_until_sequence = ?3 AND valid_from_sequence <> ?3 ORDER BY valid_from_sequence",
                    (NODE_FACT, vec![1_u8; 32], 2_i64),
                )
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("explain fact-version generation change: {error}"))
                })?;
            let mut details = Vec::new();
            while let Some(row) = rows.next().await.map_err(|error| {
                TursoStoreError::Backend(format!("read fact-version change query plan: {error}"))
            })? {
                match row.get_value(3).map_err(|error| {
                    TursoStoreError::Backend(format!("decode fact-version change query plan: {error}"))
                })? {
                    turso::Value::Text(detail) => details.push(detail),
                    other @ (turso::Value::Null
                    | turso::Value::Integer(_)
                    | turso::Value::Real(_)
                    | turso::Value::Blob(_)) => {
                        return Err(TursoStoreError::Snapshot(format!(
                            "fact-version change plan detail is not text: {other:?}"
                        )));
                    }
                }
            }
            Ok::<_, TursoStoreError>(details.join("; "))
        })?;
    let generation_plan = store.runtime.block_on(async {
        let mut rows = store
            .connection
            .query(
                &format!("EXPLAIN QUERY PLAN {FACT_VERSION_CHANGE_PAGE_QUERY}"),
                (2_i64, -1_i64, Vec::<u8>::new(), 0_i64, 101_i64),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("explain fact-version-change page: {error}"))
            })?;
        let mut details = Vec::new();
        while let Some(row) = rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read fact-version-change page plan: {error}"))
        })? {
            match row.get_value(3).map_err(|error| {
                TursoStoreError::Backend(format!("decode fact-version-change page plan: {error}"))
            })? {
                turso::Value::Text(detail) => details.push(detail),
                other @ (turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Blob(_)) => {
                    return Err(TursoStoreError::Snapshot(format!(
                        "fact-version-change page plan detail is not text: {other:?}"
                    )));
                }
            }
        }
        Ok::<_, TursoStoreError>(details.join("; "))
    })?;
    drop(store);
    drop(std::fs::remove_file(&path));
    if (!plan.contains("syntaxmesh_fact_versions_identity_time_idx")
        && !plan.contains("sqlite_autoindex_syntaxmesh_fact_versions_1"))
        || !plan.contains("syntaxmesh_fact_versions_identity_end_idx")
        || !generation_plan.contains("syntaxmesh_fact_versions_start_generation_idx")
        || !generation_plan.contains("syntaxmesh_fact_versions_end_generation_idx")
    {
        return Err(TursoStoreError::Snapshot(format!(
            "fact-version generation change does not use the identity and generation indexes: {plan}; {generation_plan}"
        )));
    }
    Ok(())
}

#[test]
fn observation_timeline_plan_uses_ordered_time_index() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-observation-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store.runtime.block_on(async {
            for sequence in 1_u64..=4_096 {
                let fact_id = sequence.to_be_bytes().to_vec();
                let observed_at = sequence.to_be_bytes().to_vec();
                let sequence_number = i64::try_from(sequence).map_err(|error| {
                    TursoStoreError::Snapshot(format!(
                        "observation query plan sequence is invalid: {error}"
                    ))
                })?;
                store
                    .connection
                    .execute(
                        "INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, observed_at_unix_nanos, payload) VALUES (2, ?1, ?2, ?3, x'00')",
                        (fact_id, sequence_number, observed_at),
                    )
                    .await
                    .map_err(|error| {
                        TursoStoreError::Backend(format!(
                            "populate observation query plan fixture: {error}"
                        ))
                    })?;
            }
            Ok::<(), TursoStoreError>(())
        })?;
    let mut rows = store
            .runtime
            .block_on(store.connection.query(
                "EXPLAIN QUERY PLAN SELECT versions.fact_kind, versions.fact_id, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.observed_at_unix_nanos IS NOT NULL AND versions.observed_at_unix_nanos >= ?1 AND versions.observed_at_unix_nanos < ?2 ORDER BY versions.observed_at_unix_nanos, versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?3",
                (
                    ObservationTime(1024).0.to_be_bytes().to_vec(),
                    ObservationTime(1034).0.to_be_bytes().to_vec(),
                    11_i64,
                ),
            ))
            .map_err(|error| {
                TursoStoreError::Backend(format!("explain observation timeline query: {error}"))
            })?;
    let mut plan = Vec::new();
    while let Some(row) = store.runtime.block_on(rows.next()).map_err(|error| {
        TursoStoreError::Backend(format!("read observation timeline plan: {error}"))
    })? {
        let detail = match row.get_value(3).map_err(|error| {
            TursoStoreError::Backend(format!("read observation plan detail: {error}"))
        })? {
            turso::Value::Text(detail) => detail,
            other @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Blob(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "observation query plan detail is not text: {other:?}"
                )));
            }
        };
        plan.push(detail);
    }
    drop(rows);
    drop(store);
    drop(std::fs::remove_file(&path));

    if !plan
        .iter()
        .any(|detail| detail.contains("syntaxmesh_fact_versions_observed_time_idx"))
    {
        return Err(TursoStoreError::Snapshot(format!(
            "observation timeline query does not use the ordered time index: {plan:?}"
        )));
    }
    Ok(())
}

#[test]
fn migration_registry_is_contiguous_and_reaches_current_schema() -> Result<(), TursoStoreError> {
    for (offset, migration) in SCHEMA_MIGRATIONS.iter().enumerate() {
        let expected_from = i64::try_from(offset + 1)
            .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
        if migration.from != expected_from
            || migration.to != expected_from + 1
            || migration.name.is_empty()
            || (migration.checksum_sql.is_some()
                != matches!(
                    migration.kind,
                    MigrationKind::V14ToV15
                        | MigrationKind::V15ToV16
                        | MigrationKind::V16ToV17
                        | MigrationKind::V17ToV18
                        | MigrationKind::V18ToV19
                        | MigrationKind::V20ToV21
                        | MigrationKind::V21ToV22
                        | MigrationKind::V22ToV23
                        | MigrationKind::V23ToV24
                ))
        {
            return Err(TursoStoreError::Snapshot(format!(
                "invalid Turso migration registry entry: {migration:?}"
            )));
        }
    }
    if SCHEMA_MIGRATIONS.last().map(|migration| migration.to) != Some(SCHEMA_VERSION) {
        return Err(TursoStoreError::Snapshot(
            "Turso migration registry does not reach current schema".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn v17_upgrade_drops_temporal_endpoint_indexes_after_legacy_migrations()
-> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-temporal-endpoint-index-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store.runtime.block_on(async {
        store
            .connection
            .execute_batch(
                "DROP INDEX IF EXISTS syntaxmesh_fact_versions_source_idx; \
                     DROP INDEX IF EXISTS syntaxmesh_fact_versions_target_idx; \
                     DELETE FROM syntaxmesh_schema_migrations WHERE version >= 18; \
                     UPDATE syntaxmesh_schema SET version = 17 WHERE id = 1;",
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!(
                    "prepare v17 temporal-index migration fixture: {error}"
                ))
            })?;
        Ok::<(), TursoStoreError>(())
    })?;
    drop(store);

    TursoGraphStore::migrate(&path)?;
    let upgraded = TursoGraphStore::open(&path)?;
    let (version, temporal_indexes, current_indexes) = upgraded.runtime.block_on(async {
        Ok::<_, TursoStoreError>((
            read_schema_version(&upgraded.connection).await?,
            count_catalog_matches(
                &upgraded.connection,
                "index",
                &[
                    "syntaxmesh_fact_versions_source_idx",
                    "syntaxmesh_fact_versions_target_idx",
                ],
            )
            .await?,
            count_catalog_matches(
                &upgraded.connection,
                "index",
                &["syntaxmesh_edges_source_idx", "syntaxmesh_edges_target_idx"],
            )
            .await?,
        ))
    })?;
    drop(upgraded);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!(
            "remove temporal endpoint index migration fixture: {error}"
        ))
    })?;
    if version != SCHEMA_VERSION || temporal_indexes != 0 || current_indexes != 2 {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso v17 upgrade produced schema {version}, {temporal_indexes} temporal endpoint indexes, and {current_indexes} current endpoint indexes"
        )));
    }
    Ok(())
}

#[test]
fn v20_upgrade_drops_temporal_indexes_and_preserves_current_edge_indexes()
-> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-v20-temporal-endpoint-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store.runtime.block_on(async {
            store
                .connection
                .execute_batch(
                    "CREATE INDEX syntaxmesh_fact_versions_source_idx ON syntaxmesh_fact_versions(source_id, valid_from_sequence, valid_until_sequence); \
                     CREATE INDEX syntaxmesh_fact_versions_target_idx ON syntaxmesh_fact_versions(target_id, valid_from_sequence, valid_until_sequence); \
                     DELETE FROM syntaxmesh_schema_migrations WHERE version > 20; \
                     UPDATE syntaxmesh_schema SET version = 20 WHERE id = 1;",
                )
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!(
                        "prepare v20 temporal-index migration fixture: {error}"
                    ))
                })?;
            Ok::<(), TursoStoreError>(())
        })?;
    drop(store);

    TursoGraphStore::migrate(&path)?;
    let upgraded = TursoGraphStore::open(&path)?;
    let (version, temporal_indexes, current_indexes) = upgraded.runtime.block_on(async {
        Ok::<_, TursoStoreError>((
            read_schema_version(&upgraded.connection).await?,
            count_catalog_matches(
                &upgraded.connection,
                "index",
                &[
                    "syntaxmesh_fact_versions_source_idx",
                    "syntaxmesh_fact_versions_target_idx",
                ],
            )
            .await?,
            count_catalog_matches(
                &upgraded.connection,
                "index",
                &["syntaxmesh_edges_source_idx", "syntaxmesh_edges_target_idx"],
            )
            .await?,
        ))
    })?;
    drop(upgraded);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!(
            "remove v20 temporal endpoint index migration fixture: {error}"
        ))
    })?;
    if version != SCHEMA_VERSION || temporal_indexes != 0 || current_indexes != 2 {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso v20 upgrade produced schema {version}, {temporal_indexes} temporal endpoint indexes, and {current_indexes} current endpoint indexes"
        )));
    }
    Ok(())
}

#[test]
fn node_kind_migration_backfills_rows_and_resolution_uses_kind_index() -> Result<(), TursoStoreError>
{
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-node-kind-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let mut store = open_migrated(&path)?;
    let node = Node {
        id: NodeId::derive(&[b"node-kind-migration-module"]),
        kind: NodeKind::Module,
        name: "src/lib.ts".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"node-kind-migration-provenance"]),
        extension_payload: None,
    };
    let (version, kind_code, index_count, plan) = store.runtime.block_on(async {
            store
                .connection
                .execute(
                    "INSERT INTO syntaxmesh_nodes (id, payload, name, owner_file, terminal_name, provenance, node_kind) VALUES (?1, ?2, ?3, NULL, ?4, ?5, 0)",
                    (
                        node.id.0.0.to_vec(),
                        bincode::serialize(&node).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                        node.name.as_str(),
                        terminal_name(&node.name),
                        node.provenance.0.0.to_vec(),
                    ),
                )
                .await
                .map_err(|error| TursoStoreError::Backend(format!("insert node-kind migration fixture: {error}")))?;
            store
                .connection
                .execute_batch(
                    "DROP INDEX syntaxmesh_nodes_kind_idx; \
                     ALTER TABLE syntaxmesh_nodes DROP COLUMN node_kind; \
                     DELETE FROM syntaxmesh_schema_migrations WHERE version >= 19; \
                     UPDATE syntaxmesh_schema SET version = 18 WHERE id = 1;",
                )
                .await
                .map_err(|error| TursoStoreError::Backend(format!("prepare old node-kind schema: {error}")))?;
            migrate_v18_to_v19(&mut store.connection).await?;
            let mut rows = store
                .connection
                .query(
                    "SELECT node_kind FROM syntaxmesh_nodes WHERE id = ?1",
                    [node.id.0.0.to_vec()],
                )
                .await
                .map_err(|error| TursoStoreError::Backend(format!("read migrated node-kind: {error}")))?;
            let code = rows
                .next()
                .await
                .map_err(|error| TursoStoreError::Backend(format!("read migrated node-kind row: {error}")))?
                .ok_or_else(|| TursoStoreError::Snapshot("migrated node disappeared".to_owned()))?
                .get::<i64>(0)
                .map_err(|error| TursoStoreError::Backend(format!("decode migrated node-kind: {error}")))?;
            drop(rows);
            let plan = explain_node_resolution_query(&store.connection).await?;
            Ok::<_, TursoStoreError>((
                read_schema_version(&store.connection).await?,
                code,
                count_catalog_matches(&store.connection, "index", &["syntaxmesh_nodes_kind_idx"]).await?,
                plan,
            ))
        })?;
    drop(store);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove node-kind migration fixture: {error}"))
    })?;
    if version != 19
        || kind_code != syntaxmesh_store::NODE_KIND_CODE_MODULE
        || index_count != 1
        || !plan.contains("syntaxmesh_nodes_kind_idx")
    {
        return Err(TursoStoreError::Snapshot(format!(
            "node-kind migration produced version {version}, code {kind_code}, index count {index_count}, plan {plan}"
        )));
    }
    Ok(())
}

#[test]
fn migration_status_is_read_only_and_open_requires_explicit_migrate() -> Result<(), TursoStoreError>
{
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-explicit-migrations.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let missing = TursoGraphStore::migration_status(&path)?;
    let open_missing = TursoGraphStore::open(&path);
    if missing.current_version.is_some() || path.exists() || open_missing.is_ok() {
        return Err(TursoStoreError::Snapshot(
            "Turso status/open created an uninitialized database".to_owned(),
        ));
    }
    TursoGraphStore::migrate(&path)?;
    let current = TursoGraphStore::migration_status(&path)?;
    let opened = TursoGraphStore::open(&path)?;
    drop(opened);
    drop(std::fs::remove_file(&path));
    if current.current_version != Some(SCHEMA_VERSION)
        || current.target_version != SCHEMA_VERSION
        || !current.migration_ledger_validated
        || current
            .migrations
            .iter()
            .any(|migration| !migration.applied)
    {
        return Err(TursoStoreError::Snapshot(
            "explicit Turso migration lifecycle did not reach the current schema".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn open_checks_latest_history_while_integrity_scans_retained_payloads()
-> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-history-integrity.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"history-integrity-repository"]);
    let worktree = WorktreeId::derive(&[b"history-integrity-worktree"]);
    let first = GenerationId::derive(&[b"history-integrity-first"]);
    let second = GenerationId::derive(&[b"history-integrity-second"]);
    let mut store = open_migrated(&path)?;
    store.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"history-integrity-first-run"]),
        None,
        first,
    ))?;
    store.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"history-integrity-second-run"]),
        Some(first),
        second,
    ))?;
    // Warm both node-read paths before changing the authoritative history bytes.
    let page = store.historical_nodes_page(first, None, 1)?;
    let search = store.historical_search_nodes(first, "", 1)?;
    if !page.items.is_empty() || !search.is_empty() {
        return Err(TursoStoreError::Snapshot(
            "empty history cache fixture returned nodes".to_owned(),
        ));
    }
    store
        .runtime
        .block_on(store.connection.execute(
            "UPDATE syntaxmesh_generation_history SET payload = x'00' WHERE sequence = 1",
            (),
        ))
        .map_err(|error| {
            TursoStoreError::Backend(format!("corrupt old history fixture: {error}"))
        })?;
    let corrupted_page = store.historical_nodes_page(first, None, 1);
    let corrupted_search = store.historical_search_nodes(first, "", 1);
    if corrupted_page.is_ok() || corrupted_search.is_ok() {
        return Err(TursoStoreError::Snapshot(
            "warmed schema cache hid changed corrupt history".to_owned(),
        ));
    }
    drop(store);

    let opened = TursoGraphStore::open(&path)?;
    let integrity = opened.backend_integrity_check()?;
    drop(opened);
    drop(std::fs::remove_file(path));
    if integrity.passed
        || !integrity.findings.iter().any(|finding| {
            finding.contains("generation history sequence 1")
                && finding.contains("invalid history payload")
        })
    {
        return Err(TursoStoreError::Snapshot(format!(
            "full history integrity check missed a corrupt old entry: {integrity:?}"
        )));
    }
    Ok(())
}

async fn count_catalog_matches(
    connection: &turso::Connection,
    object_type: &str,
    names: &[&str],
) -> Result<usize, TursoStoreError> {
    let mut count = 0_usize;
    for name in names {
        let mut rows = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = ?1 AND name = ?2",
                (object_type, *name),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("inspect Turso schema catalog: {error}"))
            })?;
        if rows
            .next()
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read Turso schema catalog: {error}"))
            })?
            .is_some()
        {
            count = count.checked_add(1).ok_or_else(|| {
                TursoStoreError::Snapshot("schema object count overflow".to_owned())
            })?;
        }
    }
    Ok(count)
}

#[test]
fn fresh_schema_creates_change_event_fact_index() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-fresh-event-index.db",
        std::process::id()
    ));
    let store = open_migrated(&path)?;
    let exists = store.runtime.block_on(async {
        let mut rows = store
            .connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?1",
                ["syntaxmesh_change_event_fact_lookup_idx"],
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("query event index fixture: {error}"))
            })?;
        Ok::<bool, TursoStoreError>(
            rows.next()
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("read event index fixture: {error}"))
                })?
                .is_some(),
        )
    })?;
    drop(store);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove fresh-index fixture: {error}"))
    })?;
    if !exists {
        return Err(TursoStoreError::Snapshot(
            "fresh Turso schema omitted its change-event fact index".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn v22_upgrade_adds_fact_history_end_lookup_index() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-fact-history-end-index-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store.runtime.block_on(async {
        store
            .connection
            .execute_batch(
                "DROP INDEX syntaxmesh_fact_versions_identity_end_idx; \
                     DELETE FROM syntaxmesh_schema_migrations WHERE version > 22; \
                     UPDATE syntaxmesh_schema SET version = 22 WHERE id = 1;",
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!(
                    "prepare v22 fact-history index migration fixture: {error}"
                ))
            })
    })?;
    drop(store);

    TursoGraphStore::migrate(&path)?;
    let migrated = open_migrated(&path)?;
    let (index_exists, version) = migrated.runtime.block_on(async {
        let mut rows = migrated
            .connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?1",
                ["syntaxmesh_fact_versions_identity_end_idx"],
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!(
                    "inspect v22 fact-history index migration: {error}"
                ))
            })?;
        let index_exists = rows
            .next()
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read v22 fact-history index migration: {error}"))
            })?
            .is_some();
        let version = read_schema_version(&migrated.connection).await?;
        Ok::<_, TursoStoreError>((index_exists, version))
    })?;
    drop(migrated);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove v22 migration fixture: {error}"))
    })?;
    if version != SCHEMA_VERSION || !index_exists {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso v22 upgrade stopped at schema {version} or omitted the fact-history end index"
        )));
    }
    Ok(())
}

#[test]
fn v23_upgrade_adds_generation_leading_fact_history_indexes() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-fact-history-generation-index-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store.runtime.block_on(async {
        store
            .connection
            .execute_batch(
                "DROP INDEX syntaxmesh_fact_versions_start_generation_idx; \
                     DROP INDEX syntaxmesh_fact_versions_end_generation_idx; \
                     DELETE FROM syntaxmesh_schema_migrations WHERE version > 23; \
                     UPDATE syntaxmesh_schema SET version = 23 WHERE id = 1;",
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!(
                    "prepare v23 generation-index migration fixture: {error}"
                ))
            })
    })?;
    drop(store);

    TursoGraphStore::migrate(&path)?;
    let migrated = open_migrated(&path)?;
    let (index_count, version) = migrated.runtime.block_on(async {
            let mut rows = migrated
                .connection
                .query(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name IN ('syntaxmesh_fact_versions_start_generation_idx', 'syntaxmesh_fact_versions_end_generation_idx')",
                    (),
                )
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!(
                        "inspect v23 generation-index migration: {error}"
                    ))
                })?;
            let index_count = rows
                .next()
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!(
                        "read v23 generation-index migration: {error}"
                    ))
                })?
                .ok_or_else(|| {
                    TursoStoreError::Snapshot("index count result is missing".to_owned())
                })?
                .get::<i64>(0)
                .map_err(|error| {
                    TursoStoreError::Backend(format!(
                        "decode v23 generation-index migration: {error}"
                    ))
                })?;
            Ok::<_, TursoStoreError>((index_count, read_schema_version(&migrated.connection).await?))
        })?;
    drop(migrated);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove v23 migration fixture: {error}"))
    })?;
    if version != SCHEMA_VERSION || index_count != 2 {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso v23 upgrade stopped at schema {version} or installed {index_count} temporal indexes"
        )));
    }
    Ok(())
}

#[test]
fn changed_fact_reference_validation_is_transactional() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-changed-reference-validation.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"changed-reference-repository"]);
    let worktree = WorktreeId::derive(&[b"changed-reference-worktree"]);
    let first = GenerationId::derive(&[b"changed-reference-first"]);
    let second = GenerationId::derive(&[b"changed-reference-second"]);
    let mut store = open_migrated(&path)?;
    store.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"changed-reference-first-run"]),
        None,
        first,
    ))?;

    let missing = ProvenanceId::derive(&[b"changed-reference-missing-provenance"]);
    let mut invalid = empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"changed-reference-second-run"]),
        Some(first),
        second,
    );
    invalid.upsert_nodes.push(Node {
        id: NodeId::derive(&[b"changed-reference-node"]),
        kind: NodeKind::Function,
        name: "invalid-reference".to_owned(),
        owner_file: None,
        source: None,
        provenance: missing,
        extension_payload: None,
    });
    let result = store.apply_delta(invalid);
    let current = store
        .latest_generation()
        .map(|manifest| manifest.generation);
    drop(store);
    drop(std::fs::remove_file(&path));
    if !matches!(result, Err(StoreError::Integrity(_))) || current != Some(first) {
        return Err(TursoStoreError::Snapshot(
            "missing changed-fact provenance was accepted or partially published".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn turso_publishes_consequence_edges_and_evidence_atomically_across_restart()
-> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-consequence-publication.db",
        std::process::id()
    ));
    let mut store = open_migrated(&path)?;
    let repository = RepositoryId::derive(&[b"turso-consequence-repository"]);
    let worktree = WorktreeId::derive(&[b"turso-consequence-worktree"]);
    let generation = GenerationId::derive(&[b"turso-consequence-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"turso-consequence-provenance"]),
        producer_namespace: "test.turso-consequences".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let mut graph = empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"turso-consequence-run"]),
        None,
        generation,
    );
    graph.upsert_provenance.push(provenance.clone());
    let event = syntaxmesh_store::InMemoryGraphStore::new()
        .change_event_for_delta(&graph)
        .map_err(TursoStoreError::Store)?;
    let fact_version = FactVersionRef {
        fact: FactRef::Provenance(provenance.id),
        valid_from: generation,
    };
    let edge = ConsequenceEdge {
        id: ConsequenceEdgeId::derive(&[b"turso-consequence-edge"]),
        source: LineageEndpoint::ChangeEvent(event.id),
        target: LineageEndpoint::FactVersion(fact_version),
        kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
        evidence: vec![fact_version],
        derivation: ConsequenceDerivation::Explicit,
        provenance: provenance.id,
    };
    let mut second_edge = edge.clone();
    second_edge.id = ConsequenceEdgeId::derive(&[b"turso-consequence-second"]);
    let second_edge_id = second_edge.id;
    store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph,
                lineage: ChangeSetDelta::default(),
            },
            consequences: ConsequenceDelta {
                add: vec![edge.clone(), second_edge],
                retract: Vec::new(),
            },
        },
        Some(AcceptanceTime(91)),
    )?;
    let first_page = store.consequence_edges_for_endpoint(
        LineageEndpoint::ChangeEvent(event.id),
        generation,
        None,
        1,
    )?;
    let cursor = first_page.next_cursor.ok_or_else(|| {
        TursoStoreError::Snapshot("consequence query omitted continuation cursor".to_owned())
    })?;
    let second_page = store.consequence_edges_for_endpoint(
        LineageEndpoint::ChangeEvent(event.id),
        generation,
        Some(cursor),
        1,
    )?;
    if first_page.items.len() != 1
        || second_page.items.len() != 1
        || second_page.next_cursor.is_some()
    {
        return Err(TursoStoreError::Snapshot(
            "Turso indexed consequence pagination was inconsistent".to_owned(),
        ));
    }
    let range_page = store.consequence_edges_for_endpoint_range(
        LineageEndpoint::ChangeEvent(event.id),
        generation,
        generation,
        None,
        1,
    )?;
    let range_cursor = range_page.next_cursor.ok_or_else(|| {
        TursoStoreError::Snapshot(
            "consequence range page omitted its continuation cursor".to_owned(),
        )
    })?;
    let range_tail = store.consequence_edges_for_endpoint_range(
        LineageEndpoint::ChangeEvent(event.id),
        generation,
        generation,
        Some(range_cursor),
        1,
    )?;
    if range_page.items.len() != 1
        || range_page
            .items
            .first()
            .is_none_or(|item| item.valid_from != generation || item.valid_until.is_some())
        || range_tail.items.len() != 1
        || range_tail.next_cursor.is_some()
    {
        return Err(TursoStoreError::Snapshot(
            "Turso consequence validity-range page or cursor was inconsistent".to_owned(),
        ));
    }
    let fact_page = store.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        generation,
        None,
        10,
    )?;
    if fact_page.items.len() != 2 {
        return Err(TursoStoreError::Snapshot(
            "Turso exact fact-version endpoint index missed consequence edges".to_owned(),
        ));
    }
    let failed_retraction_generation =
        GenerationId::derive(&[b"turso-consequence-failed-retraction"]);
    store
        .runtime
        .block_on(store.connection.execute_batch(
            "CREATE TRIGGER fail_consequence_retraction
                 BEFORE INSERT ON syntaxmesh_generation_consequences
                 BEGIN SELECT RAISE(ABORT, 'injected consequence retraction failure'); END;",
        ))
        .map_err(|error| {
            TursoStoreError::Backend(format!(
                "install consequence retraction failure trigger: {error}"
            ))
        })?;
    let failed_retraction = store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: empty_delta(
                    repository,
                    worktree,
                    syntaxmesh_core::IndexRunId::derive(&[
                        b"turso-failed-consequence-retraction-run",
                    ]),
                    Some(generation),
                    failed_retraction_generation,
                ),
                lineage: ChangeSetDelta::default(),
            },
            consequences: ConsequenceDelta {
                add: Vec::new(),
                retract: vec![syntaxmesh_core::ConsequenceRetraction {
                    edge: second_edge_id,
                    provenance: provenance.id,
                }],
            },
        },
        Some(AcceptanceTime(92)),
    );
    store
        .runtime
        .block_on(
            store
                .connection
                .execute_batch("DROP TRIGGER fail_consequence_retraction"),
        )
        .map_err(|error| {
            TursoStoreError::Backend(format!(
                "remove consequence retraction failure trigger: {error}"
            ))
        })?;
    let after_failed_retraction = store.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        generation,
        None,
        10,
    )?;
    let failed_generation_exists = store.runtime.block_on(row_exists(
        &store.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_consequences WHERE generation = ?1)",
        failed_retraction_generation.0.0.to_vec(),
    ))?;
    let failed_acceptance_exists = store.runtime.block_on(row_exists(
        &store.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_acceptance WHERE generation = ?1)",
        failed_retraction_generation.0.0.to_vec(),
    ))?;
    if !matches!(failed_retraction, Err(StoreError::Backend(_)))
        || store
            .latest_generation()
            .map(|manifest| manifest.generation)
            != Some(generation)
        || store.generation_consequence_history()?.len() != 1
        || failed_acceptance_exists
        || after_failed_retraction.items.len() != 2
        || failed_generation_exists
    {
        return Err(TursoStoreError::Snapshot(
            "failed Turso consequence retraction leaked partial publication state".to_owned(),
        ));
    }
    let retracted_generation = GenerationId::derive(&[b"turso-consequence-retraction"]);
    store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: empty_delta(
                    repository,
                    worktree,
                    syntaxmesh_core::IndexRunId::derive(&[b"turso-consequence-retraction-run"]),
                    Some(generation),
                    retracted_generation,
                ),
                lineage: ChangeSetDelta::default(),
            },
            consequences: ConsequenceDelta {
                add: Vec::new(),
                retract: vec![syntaxmesh_core::ConsequenceRetraction {
                    edge: second_edge_id,
                    provenance: provenance.id,
                }],
            },
        },
        Some(AcceptanceTime(92)),
    )?;
    let historical_page = store.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        generation,
        None,
        10,
    )?;
    let current_page = store.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        retracted_generation,
        None,
        10,
    )?;
    let retracted_range = store.consequence_edges_for_endpoint_range(
        LineageEndpoint::FactVersion(fact_version),
        generation,
        retracted_generation,
        None,
        10,
    )?;
    let second_version = retracted_range
        .items
        .iter()
        .find(|item| item.edge.id == second_edge_id);
    if historical_page.items.len() != 2 || current_page.items.len() != 1 {
        return Err(TursoStoreError::Snapshot(
            "Turso consequence retraction changed historical endpoint results".to_owned(),
        ));
    }
    if second_version.is_none_or(|item| {
        item.valid_from != generation || item.valid_until != Some(retracted_generation)
    }) || store
        .consequence_edges_for_endpoint_range(
            LineageEndpoint::FactVersion(fact_version),
            retracted_generation,
            retracted_generation,
            None,
            10,
        )?
        .items
        .iter()
        .any(|item| item.edge.id == second_edge_id)
    {
        return Err(TursoStoreError::Snapshot(
            "Turso consequence range did not preserve the exclusive retraction boundary".to_owned(),
        ));
    }
    let counts = store.runtime.block_on(async {
        let mut result = Vec::new();
        for sql in [
            "SELECT COUNT(*) FROM syntaxmesh_generation_consequences WHERE generation = ?1",
            "SELECT COUNT(*) FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1",
            "SELECT COUNT(*) FROM syntaxmesh_consequence_evidence WHERE edge_id = ?1",
        ] {
            let bind = if sql.contains("generation =") {
                generation.0.0.to_vec()
            } else {
                edge.id.0.0.to_vec()
            };
            let mut rows = store
                .connection
                .query(sql, [bind])
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            let row = rows
                .next()
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?
                .ok_or_else(|| TursoStoreError::Snapshot("count row is missing".to_owned()))?;
            result.push(row.get::<i64>(0).map_err(|error| {
                TursoStoreError::Backend(format!("read consequence count: {error}"))
            })?);
        }
        Ok::<_, TursoStoreError>(result)
    })?;
    drop(store);
    let reopened = open_migrated(&path)?;
    let history = reopened.generation_consequence_history()?;
    let reopened_page = reopened.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        generation,
        None,
        10,
    )?;
    let reopened_current_page = reopened.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        retracted_generation,
        None,
        10,
    )?;
    drop(reopened);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove consequence fixture: {error}"))
    })?;
    if counts != [1, 1, 1]
        || reopened_page.items.len() != 2
        || reopened_current_page.items.len() != 1
        || history.len() != 2
        || history.first().and_then(|entry| entry.delta.add.first()) != Some(&edge)
    {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso consequence publication was not atomic/restartable: counts={counts:?}, journal={history:?}"
        )));
    }
    Ok(())
}

#[test]
fn v14_migration_applies_indexed_consequence_schema() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-consequence-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store.runtime.block_on(async {
            store
                .connection
                .execute_batch(include_str!(
                    "../../migrations/0015_evidence_consequence_edges.down.sql"
                ))
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("prepare v14 Turso fixture: {error}"))
                })?;
            store
                .connection
                .execute_batch("DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 14 WHERE id = 1;")
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("set v14 Turso fixture: {error}"))
                })?;
            Ok::<(), TursoStoreError>(())
        })?;
    drop(store);

    let upgraded = open_migrated(&path)?;
    let (version, tables, indexes) = upgraded.runtime.block_on(async {
        let version = read_schema_version(&upgraded.connection).await?;
        let tables = count_catalog_matches(
            &upgraded.connection,
            "table",
            &[
                "syntaxmesh_generation_consequences",
                "syntaxmesh_consequence_edge_versions",
                "syntaxmesh_consequence_evidence",
                "syntaxmesh_consequence_parents",
            ],
        )
        .await?;
        let indexes = count_catalog_matches(
            &upgraded.connection,
            "index",
            &[
                "syntaxmesh_consequence_source_idx",
                "syntaxmesh_consequence_target_idx",
                "syntaxmesh_consequence_evidence_idx",
                "syntaxmesh_consequence_parent_idx",
            ],
        )
        .await?;
        Ok::<_, TursoStoreError>((version, tables, indexes))
    })?;
    drop(upgraded);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove consequence migration fixture: {error}"))
    })?;
    if version != SCHEMA_VERSION || tables != 4 || indexes != 4 {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso v14 migration produced schema {version}, {tables} consequence tables and {indexes} indexes"
        )));
    }
    Ok(())
}

#[test]
fn v12_migration_backfills_fact_linked_change_events() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-change-event-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"event-migration-repository"]);
    let worktree = WorktreeId::derive(&[b"event-migration-worktree"]);
    let generation = GenerationId::derive(&[b"event-migration-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"event-migration-provenance"]),
        producer_namespace: "test.event-migration".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"event-migration-node"]),
        kind: NodeKind::Function,
        name: "event-migration".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut store = open_migrated(&path)?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"event-migration-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![node.clone()],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let expected = store.change_events_for_fact(FactRef::Node(node.id), None, 10)?;
    drop(store);

    let path_text = path
        .to_str()
        .ok_or_else(|| TursoStoreError::Backend("test path is not UTF-8".to_owned()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let database = runtime
        .block_on(turso::Builder::new_local(path_text).build())
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let connection = database
        .connect()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    runtime.block_on(
            connection.execute_batch(
                "DROP TABLE syntaxmesh_change_event_facts; DROP TABLE syntaxmesh_change_events; DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 12 WHERE id = 1;",
            ),
        ).map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    drop(connection);
    drop(database);
    drop(runtime);

    let migrated = open_migrated(&path)?;
    let actual = migrated.change_events_for_fact(FactRef::Node(node.id), None, 10)?;
    let version = migrated
        .runtime
        .block_on(read_schema_version(&migrated.connection))?;
    drop(migrated);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove migration fixture: {error}")))?;
    if expected != actual || version != SCHEMA_VERSION {
        return Err(TursoStoreError::Snapshot(
            "Turso change-event migration did not preserve/backfill lineage".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn acceptance_time_is_atomic_and_survives_restart() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-acceptance.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"acceptance-repository"]);
    let worktree = WorktreeId::derive(&[b"acceptance-worktree"]);
    let generation = GenerationId::derive(&[b"acceptance-generation"]);
    let delta = GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"acceptance-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let mut store = open_migrated(&path)?;
    store.apply_delta_at(delta, AcceptanceTime(u64::MAX))?;
    if store.acceptance_time(generation)? != Some(AcceptanceTime(u64::MAX)) {
        return Err(TursoStoreError::Snapshot(
            "Turso acceptance time was not readable after publication".to_owned(),
        ));
    }
    if store.accepted_through(generation)? != Some(AcceptanceTime(u64::MAX)) {
        return Err(TursoStoreError::Snapshot(
            "Turso acceptance prefix was not readable after publication".to_owned(),
        ));
    }
    drop(store);
    let reopened = open_migrated(&path)?;
    let accepted = reopened.acceptance_time(generation)?;
    let accepted_through = reopened.accepted_through(generation)?;
    drop(reopened);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove Turso acceptance fixture: {error}"))
    })?;
    if accepted != Some(AcceptanceTime(u64::MAX))
        || accepted_through != Some(AcceptanceTime(u64::MAX))
    {
        return Err(TursoStoreError::Snapshot(
            "Turso acceptance time did not survive restart".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn failed_generation_consequence_write_rolls_back_graph_and_acceptance()
-> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-{}-lineage-publication-rollback.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?
            .as_nanos()
    ));
    let repository = RepositoryId::derive(&[b"publication-rollback-repository"]);
    let worktree = WorktreeId::derive(&[b"publication-rollback-worktree"]);
    let first = GenerationId::derive(&[b"publication-rollback-first"]);
    let second = GenerationId::derive(&[b"publication-rollback-second"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"publication-rollback-provenance"]),
        producer_namespace: "test.publication-rollback".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"publication-rollback-node"]),
        kind: NodeKind::Function,
        name: "must-not-publish".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };

    let mut store = open_migrated(&path)?;
    store.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"publication-rollback-first-run"]),
        None,
        first,
    ))?;
    store
        .runtime
        .block_on(store.connection.execute_batch(
            "CREATE TRIGGER fail_consequence_publication
                 BEFORE INSERT ON syntaxmesh_generation_consequences
                 BEGIN SELECT RAISE(ABORT, 'injected consequence publication failure'); END;",
        ))
        .map_err(|error| {
            TursoStoreError::Backend(format!("install consequence failure trigger: {error}"))
        })?;

    let mut second_delta = empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"publication-rollback-second-run"]),
        Some(first),
        second,
    );
    second_delta.upsert_provenance.push(provenance.clone());
    second_delta.upsert_nodes.push(node.clone());
    let event =
        syntaxmesh_store::InMemoryGraphStore::new().change_event_for_delta(&second_delta)?;
    let fact_version = FactVersionRef {
        fact: FactRef::Provenance(provenance.id),
        valid_from: second,
    };
    let consequence = ConsequenceEdge {
        id: syntaxmesh_core::ConsequenceEdgeId::derive(&[b"publication-rollback-consequence"]),
        source: LineageEndpoint::ChangeEvent(event.id),
        target: LineageEndpoint::FactVersion(fact_version),
        kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
        evidence: vec![fact_version],
        derivation: ConsequenceDerivation::Explicit,
        provenance: provenance.id,
    };
    let consequence_id = consequence.id;
    let result = store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: second_delta,
                lineage: ChangeSetDelta::default(),
            },
            consequences: ConsequenceDelta {
                add: vec![consequence],
                retract: Vec::new(),
            },
        },
        Some(AcceptanceTime(20)),
    );
    let current_is_first = store
        .latest_generation()
        .map(|manifest| manifest.generation)
        == Some(first);
    let event_exists = store.runtime.block_on(row_exists(
        &store.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_change_events WHERE generation = ?1)",
        second.0.0.to_vec(),
    ))?;
    let acceptance_exists = store.runtime.block_on(row_exists(
        &store.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_acceptance WHERE generation = ?1)",
        second.0.0.to_vec(),
    ))?;
    let consequence_exists = store.runtime.block_on(row_exists(
        &store.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1)",
        consequence_id.0.0.to_vec(),
    ))?;
    if !matches!(result, Err(StoreError::Backend(_)))
        || !current_is_first
        || store.node(first, node.id)?.is_some()
        || store.generation_lineage_history()?.len() != 1
        || store.generation_consequence_history()?.len() != 1
        || consequence_exists
        || event_exists
        || acceptance_exists
    {
        return Err(TursoStoreError::Snapshot(
            "failed lineage write leaked part of the candidate generation".to_owned(),
        ));
    }
    drop(store);

    let reopened = open_migrated(&path)?;
    let durable_event = reopened.runtime.block_on(row_exists(
        &reopened.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_change_events WHERE generation = ?1)",
        second.0.0.to_vec(),
    ))?;
    let durable_acceptance = reopened.runtime.block_on(row_exists(
        &reopened.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_acceptance WHERE generation = ?1)",
        second.0.0.to_vec(),
    ))?;
    let durable_consequence = reopened.runtime.block_on(row_exists(
        &reopened.connection,
        "SELECT EXISTS(SELECT 1 FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1)",
        consequence_id.0.0.to_vec(),
    ))?;
    let durable_ok = reopened
        .latest_generation()
        .map(|manifest| manifest.generation)
        == Some(first)
        && reopened.generation_lineage_history()?.len() == 1
        && reopened.generation_consequence_history()?.len() == 1
        && reopened.node(first, node.id)?.is_none()
        && !durable_consequence
        && !durable_event
        && !durable_acceptance;
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove Turso fixture: {error}")))?;
    if !durable_ok {
        return Err(TursoStoreError::Snapshot(
            "reopened Turso database contains a partial failed generation".to_owned(),
        ));
    }
    Ok(())
}

async fn row_exists(
    connection: &turso::Connection,
    statement: &str,
    parameter: Vec<u8>,
) -> Result<bool, TursoStoreError> {
    let mut rows = connection
        .query(statement, [parameter])
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query fixture row: {error}")))?;
    let row = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read fixture row: {error}")))?
        .ok_or_else(|| TursoStoreError::Snapshot("fixture query returned no row".to_owned()))?;
    match row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode fixture row: {error}")))?
    {
        turso::Value::Integer(0) => Ok(false),
        turso::Value::Integer(1) => Ok(true),
        value @ (turso::Value::Null
        | turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_)
        | turso::Value::Blob(_)) => Err(TursoStoreError::Snapshot(format!(
            "fixture EXISTS query returned non-boolean value {value:?}"
        ))),
    }
}

#[test]
fn v9_migration_converts_observation_time_to_index_order() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-observed-time-v9.db",
        std::process::id()
    ));
    let path_text = path
        .to_str()
        .ok_or_else(|| TursoStoreError::Backend("test path is not UTF-8".to_owned()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let database = runtime
        .block_on(turso::Builder::new_local(path_text).build())
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let connection = database
        .connect()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    runtime.block_on(async {
            connection
                .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 9); CREATE TABLE syntaxmesh_fact_versions (fact_kind INTEGER NOT NULL, fact_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, observed_at_unix_nanos BLOB, source_id BLOB, target_id BLOB, payload BLOB NOT NULL, PRIMARY KEY (fact_kind, fact_id, valid_from_sequence));")
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            for (id, time) in [(1_u8, 1_u64), (2_u8, 256_u64)] {
                connection
                    .execute(
                        "INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, observed_at_unix_nanos, payload) VALUES (2, ?1, 1, ?2, x'00')",
                        (vec![id], bincode::serialize(&time).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
                    )
                    .await
                    .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            }
            Ok::<(), TursoStoreError>(())
        })?;
    drop(connection);
    drop(database);
    drop(runtime);

    let store =
        open_migrated(&path).map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let (times, version) = store.runtime.block_on(async {
            let mut rows = store
                .connection
                .query(
                    "SELECT observed_at_unix_nanos FROM syntaxmesh_fact_versions ORDER BY observed_at_unix_nanos",
                    (),
                )
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            let mut times = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?
            {
                let bytes = match row
                    .get_value(0)
                    .map_err(|error| TursoStoreError::Backend(error.to_string()))?
                {
                    turso::Value::Blob(bytes) => bytes,
                    turso::Value::Null
                    | turso::Value::Integer(_)
                    | turso::Value::Real(_)
                    | turso::Value::Text(_) => {
                        return Err(TursoStoreError::Snapshot(
                            "migrated observed time is not a blob".to_owned(),
                        ));
                    }
                };
                let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    TursoStoreError::Snapshot(format!("migrated observed time has {} bytes", bytes.len()))
                })?;
                times.push(u64::from_be_bytes(bytes));
            }
            Ok::<_, TursoStoreError>((times, read_schema_version(&store.connection).await?))
        })?;
    drop(store);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove Turso fixture: {error}")))?;
    if times != [1, 256] || version != SCHEMA_VERSION {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso observation-time migration produced {times:?} at schema {version}"
        )));
    }
    Ok(())
}

#[test]
fn v11_migration_replaces_observation_index_with_cursor_order() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-observed-index-v11.db",
        std::process::id()
    ));
    let path_text = path
        .to_str()
        .ok_or_else(|| TursoStoreError::Backend("test path is not UTF-8".to_owned()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let database = runtime
        .block_on(turso::Builder::new_local(path_text).build())
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let connection = database
        .connect()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    runtime.block_on(async {
            connection
                .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 11); CREATE TABLE syntaxmesh_fact_versions (fact_kind INTEGER NOT NULL, fact_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, observed_at_unix_nanos BLOB, source_id BLOB, target_id BLOB, payload BLOB NOT NULL, PRIMARY KEY (fact_kind, fact_id, valid_from_sequence)); CREATE INDEX syntaxmesh_fact_versions_observed_time_idx ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id) WHERE observed_at_unix_nanos IS NOT NULL; INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, observed_at_unix_nanos, payload) VALUES (2, x'01', 1, x'000000000000007b', x'00');")
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))
        })?;
    drop(connection);
    drop(database);
    drop(runtime);

    let store =
        open_migrated(&path).map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let (definition, version) = store.runtime.block_on(async {
            let mut rows = store
                .connection
                .query(
                    "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'syntaxmesh_fact_versions_observed_time_idx'",
                    (),
                )
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            let row = rows
                .next()
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?
                .ok_or_else(|| TursoStoreError::Snapshot("observation index is missing".to_owned()))?;
            let definition = match row
                .get_value(0)
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?
            {
                turso::Value::Text(value) => value.to_ascii_lowercase(),
                turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Blob(_) => {
                    return Err(TursoStoreError::Snapshot(
                        "observation index definition is not text".to_owned(),
                    ));
                }
            };
            Ok::<_, TursoStoreError>((definition, read_schema_version(&store.connection).await?))
        })?;
    drop(store);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove Turso fixture: {error}")))?;
    if version != SCHEMA_VERSION
        || !definition.contains("fact_kind, fact_id, valid_from_sequence")
        || !definition.contains("where observed_at_unix_nanos is not null")
    {
        return Err(TursoStoreError::Snapshot(format!(
            "Turso v11 migration left schema {version} or an unordered observation index: {definition}"
        )));
    }
    Ok(())
}

#[test]
fn v2_migration_backfills_and_validates_query_indexes() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-v2-migration.db",
        std::process::id()
    ));
    let path_text = path
        .to_str()
        .ok_or_else(|| TursoStoreError::Backend("test path is not UTF-8".to_owned()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let database = runtime
        .block_on(turso::Builder::new_local(path_text).build())
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let mut connection = database
        .connect()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let file_id = FileId::derive(&[b"legacy-file"]);
    let provenance_id = ProvenanceId::derive(&[b"legacy-provenance"]);
    let source_id = NodeId::derive(&[b"legacy-source"]);
    let target_id = NodeId::derive(&[b"legacy-target"]);
    let source = Node {
        id: source_id,
        kind: NodeKind::Function,
        name: "legacy_source".to_owned(),
        owner_file: Some(file_id),
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    let target = Node {
        id: target_id,
        kind: NodeKind::Function,
        name: "legacy_target".to_owned(),
        owner_file: Some(file_id),
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    let edge = Edge {
        id: EdgeId::derive(&[b"legacy-edge"]),
        source: source_id,
        target: target_id,
        relation: RelationKind::Calls,
        provenance: provenance_id,
        extension_payload: None,
    };
    runtime.block_on(async {
            connection
                .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 2); CREATE TABLE syntaxmesh_nodes (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL); CREATE TABLE syntaxmesh_edges (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL);")
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            connection
                .execute(
                    "INSERT INTO syntaxmesh_nodes (id, payload) VALUES (?1, ?2)",
                    (source.id.0.0.to_vec(), bincode::serialize(&source).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
                )
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            connection
                .execute(
                    "INSERT INTO syntaxmesh_nodes (id, payload) VALUES (?1, ?2)",
                    (target.id.0.0.to_vec(), bincode::serialize(&target).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
                )
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            connection
                .execute(
                    "INSERT INTO syntaxmesh_edges (id, payload) VALUES (?1, ?2)",
                    (edge.id.0.0.to_vec(), bincode::serialize(&edge).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
                )
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            migrate_v2_to_v3(&mut connection).await?;
            migrate_v3_to_v4(&mut connection).await?;
            migrate_v4_to_v5(&mut connection).await?;
            integrity::validate_index_columns(&connection).await
        })?;
    drop(connection);
    drop(database);
    drop(runtime);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove migration fixture: {error}")))?;
    Ok(())
}

#[test]
fn failed_v2_migration_rolls_back_schema_changes() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-v2-migration-failure.db",
        std::process::id()
    ));
    let path_text = path
        .to_str()
        .ok_or_else(|| TursoStoreError::Backend("test path is not UTF-8".to_owned()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let database = runtime
        .block_on(turso::Builder::new_local(path_text).build())
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let mut connection = database
        .connect()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    runtime.block_on(async {
            connection
                .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 2); CREATE TABLE syntaxmesh_nodes (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL); CREATE TABLE syntaxmesh_edges (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL); INSERT INTO syntaxmesh_nodes (id, payload) VALUES (x'01', x'00');")
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            let migration = migrate_v2_to_v3(&mut connection).await;
            let version = read_schema_version(&connection).await?;
            let new_columns_exist = integrity::validate_index_columns(&connection).await.is_ok();
            if migration.is_ok() || version != 2 || new_columns_exist {
                return Err(TursoStoreError::Snapshot(
                    "failed schema migration was not rolled back atomically".to_owned(),
                ));
            }
            Ok(())
        })?;
    drop(connection);
    drop(database);
    drop(runtime);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove migration fixture: {error}")))?;
    Ok(())
}

#[test]
fn failed_v3_migration_rolls_back_terminal_name_index() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-v3-migration-failure.db",
        std::process::id()
    ));
    let path_text = path
        .to_str()
        .ok_or_else(|| TursoStoreError::Backend("test path is not UTF-8".to_owned()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let database = runtime
        .block_on(turso::Builder::new_local(path_text).build())
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let mut connection = database
        .connect()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    runtime.block_on(async {
            connection
                .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 3); CREATE TABLE syntaxmesh_nodes (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL, name TEXT NOT NULL, owner_file BLOB); CREATE TABLE syntaxmesh_edges (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL, source BLOB, target BLOB); INSERT INTO syntaxmesh_nodes (id, payload, name) VALUES (x'01', x'00', 'broken');")
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            let migration = migrate_v3_to_v4(&mut connection).await;
            let version = read_schema_version(&connection).await?;
            let terminal_column_exists = connection
                .query("SELECT terminal_name FROM syntaxmesh_nodes LIMIT 0", ())
                .await
                .is_ok();
            if migration.is_ok() || version != 3 || terminal_column_exists {
                return Err(TursoStoreError::Snapshot(
                    "failed v3 migration did not roll back atomically".to_owned(),
                ));
            }
            Ok(())
        })?;
    drop(connection);
    drop(database);
    drop(runtime);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove migration fixture: {error}")))?;
    Ok(())
}

#[test]
fn failed_v4_migration_rolls_back_provenance_indexes() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-v4-migration-failure.db",
        std::process::id()
    ));
    let path_text = path
        .to_str()
        .ok_or_else(|| TursoStoreError::Backend("test path is not UTF-8".to_owned()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let database = runtime
        .block_on(turso::Builder::new_local(path_text).build())
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let mut connection = database
        .connect()
        .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    runtime.block_on(async {
            connection
                .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 4); CREATE TABLE syntaxmesh_nodes (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL, name TEXT NOT NULL, owner_file BLOB, terminal_name TEXT NOT NULL); CREATE TABLE syntaxmesh_edges (id BLOB PRIMARY KEY NOT NULL, payload BLOB NOT NULL, source BLOB NOT NULL, target BLOB NOT NULL); INSERT INTO syntaxmesh_nodes (id, payload, name, terminal_name) VALUES (x'01', x'00', 'broken', 'broken');")
                .await
                .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
            let migration = migrate_v4_to_v5(&mut connection).await;
            let version = read_schema_version(&connection).await?;
            let provenance_column_exists = connection
                .query("SELECT provenance FROM syntaxmesh_nodes LIMIT 0", ())
                .await
                .is_ok();
            if migration.is_ok() || version != 4 || provenance_column_exists {
                return Err(TursoStoreError::Snapshot(
                    "failed v4 migration did not roll back atomically".to_owned(),
                ));
            }
            Ok(())
        })?;
    drop(connection);
    drop(database);
    drop(runtime);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove migration fixture: {error}")))?;
    Ok(())
}

#[test]
fn persists_graph_generation_and_status() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-{}.db",
        std::process::id(),
        "adapter"
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"repo"]);
    let worktree = WorktreeId::derive(&[b"worktree"]);
    let generation = GenerationId::derive(&[b"generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"provenance"]),
        producer_namespace: "test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"node"]),
        kind: NodeKind::Function,
        name: "persisted".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut store = open_migrated(&path)?;
    let manifest = store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![node.clone()],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let verified = store.set_generation_status(generation, GenerationStatus::Verified)?;
    if verified.status != GenerationStatus::Verified {
        return Err(TursoStoreError::Snapshot(
            "Turso status update was not applied".to_owned(),
        ));
    }
    store.runtime.block_on(async {
            store
                .connection
                .execute("DELETE FROM syntaxmesh_generation_history", ())
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!(
                        "clear history for legacy migration fixture: {error}"
                    ))
                })?;
            store
                .connection
                .execute_batch("DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 5 WHERE id = 1;")
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("prepare legacy schema fixture: {error}"))
                })?;
            Ok::<(), TursoStoreError>(())
        })?;
    drop(store);
    let mut reopened = open_migrated(&path)?;
    let restored = reopened.manifest(generation)?;
    let restored_node = reopened.node(generation, node.id)?;
    let history = reopened.generation_history()?;
    let historical = reopened.historical_snapshot(generation)?;
    let next_generation = GenerationId::derive(&[b"after-history-migration"]);
    let next_manifest = reopened.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"post-migration-run"]),
        Some(generation),
        next_generation,
    ))?;
    let mixed_history = reopened.generation_history()?;
    let mixed_old_snapshot = reopened.historical_snapshot(generation)?;
    let mixed_new_snapshot = reopened.historical_snapshot(next_generation)?;
    reopened.runtime.block_on(async {
            reopened
                .connection
                .execute("DELETE FROM syntaxmesh_fact_versions", ())
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!(
                        "clear temporal index for v6 migration fixture: {error}"
                    ))
                })?;
            reopened
                .connection
                .execute_batch("DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 6 WHERE id = 1;")
                .await
                .map_err(|error| {
                    TursoStoreError::Backend(format!("prepare v6 migration fixture: {error}"))
                })?;
            Ok::<(), TursoStoreError>(())
        })?;
    drop(reopened);
    let migrated_restart = open_migrated(&path)?;
    let retained_historical = migrated_restart.historical_snapshot(generation)?;
    migrated_restart
        .runtime
        .block_on(migrated_restart.connection.execute(
            "UPDATE syntaxmesh_graph_checkpoints SET payload = x'00' WHERE sequence = 1",
            (),
        ))
        .map_err(|error| {
            TursoStoreError::Backend(format!("corrupt checkpoint test fixture: {error}"))
        })?;
    let checkpoint_independent_historical = migrated_restart.historical_snapshot(generation)?;
    let integrity = migrated_restart.backend_integrity_check()?;
    std::mem::drop(std::fs::remove_file(path));
    if restored.repository != manifest.repository
        || restored.worktree != manifest.worktree
        || restored.status != GenerationStatus::Verified
        || restored_node != Some(node)
        || history.len() != 1
        || next_manifest.schema_version != 2
        || mixed_history.len() != 2
        || !mixed_history
            .first()
            .is_some_and(|entry| entry.manifest.schema_version == restored.schema_version)
        || !mixed_history
            .get(1)
            .is_some_and(|entry| entry.manifest.schema_version == 2)
        || mixed_old_snapshot != historical
        || mixed_new_snapshot != historical
        || !history.first().is_some_and(|entry| {
            entry.manifest.generation == generation
                && entry.delta.is_none()
                && entry.anchor.is_some()
        })
        || historical.nodes.len() != 1
        || retained_historical != historical
        || checkpoint_independent_historical != historical
        || integrity.passed
        || !integrity
            .findings
            .iter()
            .any(|finding| finding.contains("checkpoint sequence 1"))
    {
        return Err(TursoStoreError::Snapshot(
            "Turso snapshot did not survive restart".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn explicit_change_set_membership_is_indexed_and_survives_restart() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-changeset.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"changeset-repository"]);
    let worktree = WorktreeId::derive(&[b"changeset-worktree"]);
    let first = GenerationId::derive(&[b"changeset-first"]);
    let second = GenerationId::derive(&[b"changeset-second"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"changeset-provenance"]),
        producer_namespace: "test.changeset".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let change_set = ChangeSetId::derive(&[b"changeset-id"]);
    let mut store = open_migrated(&path)?;
    let mut initial = empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"changeset-run-1"]),
        None,
        first,
    );
    initial.upsert_provenance.push(provenance.clone());
    store.apply_delta(initial)?;
    let next_delta = empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"changeset-run-2"]),
        Some(first),
        second,
    );
    let event = store
        .runtime
        .block_on(change_event_for_delta(&store.connection, &next_delta))?;
    let lineage = ChangeSetDelta {
        upsert_sets: vec![ChangeSet {
            id: change_set,
            kind: ChangeSetKind::ManualGroup,
            title: Some("test group".to_owned()),
            originating_intent: None,
            parent_changes: Vec::new(),
            git_commits: Vec::new(),
            pull_requests: Vec::new(),
            issues: Vec::new(),
            adrs: Vec::new(),
            repositories: vec![repository],
            first_generation: first,
            last_generation: Some(second),
            provenance: provenance.id,
        }],
        assign_events: vec![ChangeSetMembership {
            change_set,
            event: event.id,
            provenance: provenance.id,
        }],
        unassign_events: Vec::new(),
    };
    store.apply_delta_with_lineage(
        GraphDeltaWithLineage {
            graph: next_delta,
            lineage,
        },
        None,
    )?;
    if !store
        .events_for_change_set(change_set, first, None, 5)?
        .items
        .is_empty()
        || store
            .events_for_change_set(change_set, second, None, 5)?
            .items
            .first()
            .map(|item| item.event.id)
            != Some(event.id)
        || store.generation_lineage_history()?.len() != 2
    {
        return Err(TursoStoreError::Snapshot(
            "ChangeSet query did not respect validity or lineage history".to_owned(),
        ));
    }
    drop(store);
    let reopened = open_migrated(&path)?;
    let replayed = reopened.events_for_change_set(change_set, second, None, 5)?;
    if replayed.items.first().map(|item| item.event.id) != Some(event.id) {
        return Err(TursoStoreError::Snapshot(
            "ChangeSet projection did not survive restart".to_owned(),
        ));
    }
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| TursoStoreError::Backend(format!("remove ChangeSet fixture: {error}")))?;
    Ok(())
}

#[test]
fn v13_migration_backfills_empty_explicit_lineage_for_retained_generations()
-> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-lineage-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"lineage-migration-repository"]);
    let worktree = WorktreeId::derive(&[b"lineage-migration-worktree"]);
    let generation = GenerationId::derive(&[b"lineage-migration-generation"]);
    let mut store = open_migrated(&path)?;
    store.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"lineage-migration-run"]),
        None,
        generation,
    ))?;
    store.runtime.block_on(async {
        store
            .connection
            .execute_batch(
                "DROP TABLE syntaxmesh_schema_migrations;\
                     DROP TABLE syntaxmesh_change_set_membership_versions;\
                     DROP TABLE syntaxmesh_change_set_versions;\
                     DROP TABLE syntaxmesh_generation_lineage;\
                     UPDATE syntaxmesh_schema SET version = 13 WHERE id = 1;",
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("prepare v13 migration fixture: {error}"))
            })
    })?;
    drop(store);
    let reopened = open_migrated(&path)?;
    let history = reopened.generation_lineage_history()?;
    let backfilled = history.iter().find(|entry| entry.generation == generation);
    if history.len() != 1 || backfilled.is_none_or(|entry| !entry.delta.is_empty()) {
        return Err(TursoStoreError::Snapshot(
            "v13 migration did not backfill an empty lineage slot".to_owned(),
        ));
    }
    drop(reopened);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove lineage migration fixture: {error}"))
    })?;
    Ok(())
}

#[test]
fn rejects_a_stale_writer_across_open_handles() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-stale-writer.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"cross-handle-repo"]);
    let worktree = WorktreeId::derive(&[b"cross-handle-worktree"]);
    let first_generation = GenerationId::derive(&[b"cross-handle-first"]);
    let stale_generation = GenerationId::derive(&[b"cross-handle-stale"]);
    let mut first_writer = open_migrated(&path)?;
    let mut stale_writer = open_migrated(&path)?;
    first_writer.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"cross-handle-first-run"]),
        None,
        first_generation,
    ))?;
    let stale_result = stale_writer.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"cross-handle-stale-run"]),
        None,
        stale_generation,
    ));
    if stale_result
        != Err(StoreError::StaleBase {
            expected: None,
            actual: Some(first_generation),
        })
    {
        return Err(TursoStoreError::Snapshot(
            "Turso accepted a cross-handle stale writer".to_owned(),
        ));
    }
    drop(stale_writer);
    let mut stale_status_writer = open_migrated(&path)?;
    first_writer.set_generation_status(first_generation, GenerationStatus::Verified)?;
    let stale_status_result = stale_status_writer
        .set_generation_status(first_generation, GenerationStatus::VerificationFailed);
    if stale_status_result
        != Err(StoreError::StaleBase {
            expected: Some(first_generation),
            actual: Some(first_generation),
        })
    {
        return Err(TursoStoreError::Snapshot(
            "Turso accepted a stale cross-handle status update".to_owned(),
        ));
    }
    drop(stale_status_writer);
    drop(first_writer);
    let reopened = open_migrated(&path)?;
    let persisted = reopened.latest_generation();
    drop(reopened);
    std::mem::drop(std::fs::remove_file(path));
    if persisted.is_none_or(|manifest| {
        manifest.generation != first_generation || manifest.status != GenerationStatus::Verified
    }) {
        return Err(TursoStoreError::Snapshot(
            "stale Turso writer replaced the accepted generation".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn rejects_reads_from_a_superseded_open_snapshot() -> Result<(), TursoStoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-stale-reader.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"stale-reader-repo"]);
    let worktree = WorktreeId::derive(&[b"stale-reader-worktree"]);
    let first_generation = GenerationId::derive(&[b"stale-reader-first"]);
    let next_generation = GenerationId::derive(&[b"stale-reader-next"]);
    let mut writer = open_migrated(&path)?;
    writer.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"stale-reader-first-run"]),
        None,
        first_generation,
    ))?;
    let stale_reader = open_migrated(&path)?;
    writer.apply_delta(empty_delta(
        repository,
        worktree,
        syntaxmesh_core::IndexRunId::derive(&[b"stale-reader-next-run"]),
        Some(first_generation),
        next_generation,
    ))?;
    let stale_read = stale_reader.nodes(first_generation);
    drop(stale_reader);
    drop(writer);
    std::fs::remove_file(path).map_err(|error| {
        TursoStoreError::Backend(format!("remove stale reader fixture: {error}"))
    })?;
    if !matches!(
        stale_read,
        Err(StoreError::StaleBase {
            expected: Some(expected),
            actual: Some(actual),
        }) if expected == first_generation && actual == next_generation
    ) {
        return Err(TursoStoreError::Snapshot(
            "Turso served a superseded in-memory snapshot".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn durable_records_survive_restart_and_reject_stale_handles() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-{}-records.db",
        std::process::id()
    ));
    let mut writer = open_migrated(&path).map_err(TursoGraphStore::map_backend)?;
    let mut stale_writer = open_migrated(&path).map_err(TursoGraphStore::map_backend)?;
    let key = "syntaxmesh.test.operation/first";
    writer.compare_exchange_record(key, None, b"prepared")?;
    writer.compare_exchange_record("syntaxmesh.test.operation-extra", None, b"outside")?;
    writer.compare_exchange_record("unrelated", None, b"outside")?;
    if stale_writer
        .compare_exchange_record(key, None, b"wrong")
        .is_ok()
    {
        return Err(StoreError::Integrity(
            "Turso accepted stale durable-record creation".to_owned(),
        ));
    }
    drop(stale_writer);
    drop(writer);
    let reopened = open_migrated(&path).map_err(TursoGraphStore::map_backend)?;
    let record = reopened.read_record(key)?;
    let prefixed = reopened.records_with_prefix("syntaxmesh.test.operation/")?;
    let latest = reopened.last_record_with_prefix("syntaxmesh.test.operation/")?;
    let exact = reopened.last_record_with_prefix(key)?;
    drop(reopened);
    std::mem::drop(std::fs::remove_file(path));
    if record.as_deref() != Some(b"prepared")
        || prefixed != vec![(key.to_owned(), b"prepared".to_vec())]
        || latest != Some((key.to_owned(), b"prepared".to_vec()))
        || exact != Some((key.to_owned(), b"prepared".to_vec()))
    {
        return Err(StoreError::Integrity(
            "Turso durable records did not survive restart".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn record_prefix_upper_bound_handles_unicode_boundaries() {
    assert_eq!(prefix_upper_bound("scope/").as_deref(), Some("scope0"));
    assert_eq!(
        prefix_upper_bound("a\u{D7FF}").as_deref(),
        Some("a\u{E000}")
    );
    assert_eq!(prefix_upper_bound("\u{10FFFF}"), None);
}

fn empty_delta(
    repository: RepositoryId,
    worktree: WorktreeId,
    run_id: syntaxmesh_core::IndexRunId,
    expected_base: Option<GenerationId>,
    next_generation: GenerationId,
) -> GraphDelta {
    GraphDelta {
        repository,
        worktree,
        run_id,
        expected_base,
        next_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    }
}
