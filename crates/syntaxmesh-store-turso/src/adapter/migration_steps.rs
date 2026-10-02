//! Registered Turso schema transitions and transactional data migrations.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::migrations::{MigrationKind, SCHEMA_MIGRATIONS, SchemaMigration};
use syntaxmesh_store::IncidenceMutation;

pub(super) async fn apply_schema_migration(
    connection: &mut turso::Connection,
    migration: &SchemaMigration,
) -> Result<(), TursoStoreError> {
    match migration.kind {
        MigrationKind::VersionOnly => advance_schema_version(connection, migration.to).await,
        MigrationKind::V2ToV3 => migrate_v2_to_v3(connection).await,
        MigrationKind::V3ToV4 => migrate_v3_to_v4(connection).await,
        MigrationKind::V4ToV5 => migrate_v4_to_v5(connection).await,
        MigrationKind::V6ToV7 => migrate_v6_to_v7(connection).await,
        MigrationKind::V7ToV8 => migrate_v7_to_v8(connection).await,
        MigrationKind::V8ToV9 => migrate_v8_to_v9(connection).await,
        MigrationKind::V9ToV10 => migrate_v9_to_v10(connection).await,
        MigrationKind::V10ToV11 => migrate_v10_to_v11(connection).await,
        MigrationKind::V11ToV12 => migrate_v11_to_v12(connection).await,
        MigrationKind::V12ToV13 => migrate_v12_to_v13(connection).await,
        MigrationKind::V13ToV14 => migrate_v13_to_v14(connection).await,
        MigrationKind::V14ToV15 => migrate_v14_to_v15(connection).await,
        MigrationKind::V15ToV16 => migrate_v15_to_v16(connection).await,
        MigrationKind::V16ToV17 => migrate_v16_to_v17(connection).await,
        MigrationKind::V17ToV18 => migrate_v17_to_v18(connection).await,
        MigrationKind::V18ToV19 => migrate_v18_to_v19(connection).await,
        MigrationKind::V19ToV20 => migrate_v19_to_v20(connection).await,
        MigrationKind::V20ToV21 => migrate_v20_to_v21(connection).await,
        MigrationKind::V21ToV22 => migrate_v21_to_v22(connection).await,
        MigrationKind::V22ToV23 => migrate_v22_to_v23(connection).await,
        MigrationKind::V23ToV24 => migrate_v23_to_v24(connection).await,
    }
}

async fn migrate_v22_to_v23(connection: &mut turso::Connection) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin fact-history end-lookup migration: {error}"))
        })?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0023_fact_history_end_lookup.up.sql"
        ))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create fact-history end-lookup index: {error}"))
        })?;
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == 23)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("v23 migration is missing from registry".to_owned())
        })?;
    let mut timestamp_rows =
        transaction
            .query("SELECT unixepoch()", ())
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read fact-history migration time: {error}"))
            })?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read fact-history migration time row: {error}"))
        })?
        .ok_or_else(|| TursoStoreError::Snapshot("migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| {
            TursoStoreError::Backend(format!("decode fact-history migration time: {error}"))
        })?;
    drop(timestamp_rows);
    transaction
        .execute(
            "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, 0, ?4)",
            (
                migration.to,
                migration.name,
                timestamp,
                migration_checksum(migration),
            ),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record fact-history migration: {error}"))
        })?;
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 23 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("advance schema to version 23: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit fact-history migration: {error}"))
    })
}

async fn migrate_v23_to_v24(connection: &mut turso::Connection) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!(
                "begin fact-history generation-index migration: {error}"
            ))
        })?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0024_fact_history_generation_lookup.up.sql"
        ))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create fact-history generation indexes: {error}"))
        })?;
    let mut timestamp_rows = transaction
        .query("SELECT unixepoch()", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time: {error}")))?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time row: {error}")))?
        .ok_or_else(|| TursoStoreError::Snapshot("migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode migration time: {error}")))?;
    drop(timestamp_rows);
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == 24)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("v24 migration is missing from registry".to_owned())
        })?;
    transaction
        .execute(
            "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, 0, ?4)",
            (migration.to, migration.name, timestamp, migration_checksum(migration)),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("record v24 migration: {error}")))?;
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 24 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("advance schema to version 24: {error}"))
        })?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit v24 migration: {error}")))
}

pub(super) async fn migrate_v21_to_v22(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin incidence-root migration: {error}"))
        })?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0022_temporal_incidence_roots.up.sql"
        ))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create incidence-root table: {error}"))
        })?;
    transaction
        .execute("DELETE FROM syntaxmesh_temporal_incidence_roots", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("clear incidence roots for rebuild: {error}"))
        })?;

    let history = read_history_with_sequences(&transaction).await?;
    let mut active_edges = BTreeMap::<EdgeId, Edge>::new();
    let mut endpoint_edges = BTreeMap::<NodeId, BTreeSet<EdgeId>>::new();
    let mut parent = None;
    for (sequence, entry) in history {
        let changes = if let Some(anchor) = &entry.anchor {
            active_edges.clear();
            endpoint_edges.clear();
            for edge in &anchor.edges {
                index_edge(edge.clone(), &mut active_edges, &mut endpoint_edges);
            }
            parent = None;
            snapshot_incidence_changes(anchor)
        } else {
            let delta = entry.delta.as_ref().ok_or_else(|| {
                TursoStoreError::Snapshot(
                    "history entry lacks an anchor and delta during incidence backfill".to_owned(),
                )
            })?;
            let mut removed = delta.remove_edges.iter().copied().collect::<BTreeSet<_>>();
            for node in &delta.remove_nodes {
                if let Some(incident) = endpoint_edges.get(node) {
                    removed.extend(incident.iter().copied());
                }
            }
            removed.extend(delta.upsert_edges.iter().map(|edge| edge.id));
            let mut changes = Vec::new();
            for edge_id in removed {
                remove_edge_index(
                    edge_id,
                    &mut active_edges,
                    &mut endpoint_edges,
                    &mut changes,
                );
            }
            for edge in &delta.upsert_edges {
                index_edge(edge.clone(), &mut active_edges, &mut endpoint_edges);
                changes.push(IncidenceMutation {
                    edge: edge.id,
                    source: edge.source,
                    target: edge.target,
                    present: true,
                });
            }
            changes
        };
        publish_incidence_root(
            &transaction,
            sequence,
            entry.manifest.generation,
            parent,
            &changes,
        )
        .await?;
        parent = Some(entry.manifest.generation);
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 22 WHERE id = 1 AND version = 21",
            (),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record incidence-root schema version: {error}"))
        })?;
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == 22)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("v22 migration is missing from registry".to_owned())
        })?;
    let mut timestamp_rows =
        transaction
            .query("SELECT unixepoch()", ())
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read incidence migration time: {error}"))
            })?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read incidence migration time row: {error}"))
        })?
        .ok_or_else(|| TursoStoreError::Snapshot("incidence migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| {
            TursoStoreError::Backend(format!("decode incidence migration time: {error}"))
        })?;
    drop(timestamp_rows);
    transaction
        .execute(
            "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, 0, ?4)",
            (migration.to, migration.name, timestamp, migration_checksum(migration)),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record incidence-root migration: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit incidence-root migration: {error}"))
    })
}

fn index_edge(
    edge: Edge,
    active_edges: &mut BTreeMap<EdgeId, Edge>,
    endpoint_edges: &mut BTreeMap<NodeId, BTreeSet<EdgeId>>,
) {
    endpoint_edges
        .entry(edge.source)
        .or_default()
        .insert(edge.id);
    endpoint_edges
        .entry(edge.target)
        .or_default()
        .insert(edge.id);
    active_edges.insert(edge.id, edge);
}

fn remove_edge_index(
    edge_id: EdgeId,
    active_edges: &mut BTreeMap<EdgeId, Edge>,
    endpoint_edges: &mut BTreeMap<NodeId, BTreeSet<EdgeId>>,
    changes: &mut Vec<IncidenceMutation>,
) {
    let Some(edge) = active_edges.remove(&edge_id) else {
        return;
    };
    for endpoint in [edge.source, edge.target] {
        if let Some(edges) = endpoint_edges.get_mut(&endpoint) {
            edges.remove(&edge_id);
            if edges.is_empty() {
                endpoint_edges.remove(&endpoint);
            }
        }
    }
    changes.push(IncidenceMutation {
        edge: edge.id,
        source: edge.source,
        target: edge.target,
        present: false,
    });
}

pub(super) async fn migrate_v18_to_v19(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("begin node-kind migration: {error}")))?;
    let mut columns = transaction
        .query("PRAGMA table_info('syntaxmesh_nodes')", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("inspect node schema: {error}")))?;
    let mut has_node_kind = false;
    while let Some(row) = columns
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read node schema: {error}")))?
    {
        if row
            .get::<String>(1)
            .map_err(|error| TursoStoreError::Backend(format!("decode node column: {error}")))?
            == "node_kind"
        {
            has_node_kind = true;
        }
    }
    drop(columns);
    if has_node_kind {
        transaction
            .execute_batch(
                "CREATE INDEX IF NOT EXISTS syntaxmesh_nodes_kind_idx ON syntaxmesh_nodes(node_kind, id)",
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("create node-kind index: {error}"))
            })?;
    } else {
        transaction
            .execute_batch(include_str!(
                "../../migrations/0019_indexed_node_kinds.up.sql"
            ))
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("create node-kind projection: {error}"))
            })?;
    }
    let mut rows = transaction
        .query("SELECT id, payload FROM syntaxmesh_nodes ORDER BY id", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read nodes for kind backfill: {error}"))
        })?;
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read node-kind backfill row: {error}"))
    })? {
        let id = row
            .get_value(0)
            .map_err(|error| TursoStoreError::Backend(format!("read node ID: {error}")))?;
        let payload = row
            .get_value(1)
            .map_err(|error| TursoStoreError::Backend(format!("read node payload: {error}")))?;
        let node: Node = super::decode_blob(payload, "node payload")?;
        if id != turso::Value::Blob(node.id.0.0.to_vec()) {
            return Err(TursoStoreError::Snapshot(
                "node row ID disagrees with canonical payload during kind-index migration"
                    .to_owned(),
            ));
        }
        transaction
            .execute(
                "UPDATE syntaxmesh_nodes SET node_kind = ?1 WHERE id = ?2",
                (
                    syntaxmesh_store::node_kind_storage_code(&node.kind),
                    node.id.0.0.to_vec(),
                ),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("backfill node-kind projection: {error}"))
            })?;
    }
    drop(rows);

    let mut timestamp_rows = transaction
        .query("SELECT unixepoch()", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time: {error}")))?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time row: {error}")))?
        .ok_or_else(|| TursoStoreError::Snapshot("migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode migration time: {error}")))?;
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == 19)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("v19 migration is missing from registry".to_owned())
        })?;
    let mut ledger_rows = transaction
        .query(
            "SELECT 1 FROM syntaxmesh_schema_migrations WHERE version = 19 LIMIT 1",
            (),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("inspect migration ledger: {error}")))?;
    let ledger_entry_exists = ledger_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration ledger: {error}")))?
        .is_some();
    drop(ledger_rows);
    if !ledger_entry_exists {
        transaction
            .execute(
                "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, 0, ?4)",
                (
                    migration.to,
                    migration.name,
                    timestamp,
                    migration_checksum(migration),
                ),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("record node-kind migration: {error}"))
            })?;
    }
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 19 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("advance schema to version 19: {error}"))
        })?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit node-kind migration: {error}")))
}

async fn advance_schema_version(
    connection: &mut turso::Connection,
    version: i64,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("begin schema migration: {error}")))?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [version],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("record schema migration: {error}")))?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit schema migration: {error}")))
}

async fn migrate_v19_to_v20(connection: &mut turso::Connection) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin diagnostic schema migration: {error}"))
        })?;
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == 20)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("v20 migration is missing from registry".to_owned())
        })?;
    let mut timestamp_rows =
        transaction
            .query("SELECT unixepoch()", ())
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read diagnostic migration time: {error}"))
            })?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read diagnostic migration time row: {error}"))
        })?
        .ok_or_else(|| TursoStoreError::Snapshot("migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| {
            TursoStoreError::Backend(format!("decode diagnostic migration time: {error}"))
        })?;
    drop(timestamp_rows);
    transaction
        .execute(
            "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, 0, ?4)",
            (
                migration.to,
                migration.name,
                timestamp,
                migration_checksum(migration),
            ),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record diagnostic schema migration: {error}"))
        })?;
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 20 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("advance schema to version 20: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit diagnostic schema migration: {error}"))
    })
}

async fn migrate_v20_to_v21(connection: &mut turso::Connection) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin temporal endpoint-index migration: {error}"))
        })?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0021_remove_temporal_endpoint_indexes.up.sql"
        ))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("drop temporal endpoint indexes: {error}"))
        })?;
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == 21)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("v21 migration is missing from registry".to_owned())
        })?;
    let mut timestamp_rows = transaction
        .query("SELECT unixepoch()", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time: {error}")))?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time row: {error}")))?
        .ok_or_else(|| TursoStoreError::Snapshot("migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode migration time: {error}")))?;
    drop(timestamp_rows);
    transaction
        .execute(
            "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, 0, ?4)",
            (
                migration.to,
                migration.name,
                timestamp,
                migration_checksum(migration),
            ),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record temporal endpoint-index migration: {error}"))
        })?;
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 21 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("advance schema to version 21: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit endpoint-index migration: {error}"))
    })
}

async fn migrate_v16_to_v17(connection: &mut turso::Connection) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("begin ledger migration: {error}")))?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0017_schema_migration_ledger.up.sql"
        ))
        .await
        .map_err(|error| TursoStoreError::Backend(format!("create migration ledger: {error}")))?;
    let mut timestamp_rows = transaction
        .query("SELECT unixepoch()", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time: {error}")))?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time row: {error}")))?
        .ok_or_else(|| TursoStoreError::Snapshot("migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode migration time: {error}")))?;
    for migration in SCHEMA_MIGRATIONS
        .iter()
        .filter(|migration| migration.to <= 17)
    {
        let checksum = migration
            .checksum_sql
            .map(|sql| blake3::hash(sql.as_bytes()).to_hex().to_string());
        let (applied_at, adopted) = if migration.to == 17 {
            (Some(timestamp), 0_i64)
        } else {
            (None, 1_i64)
        };
        transaction
            .execute(
                "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, ?4, ?5)",
                (migration.to, migration.name, applied_at, adopted, checksum),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("record migration ledger entry: {error}")))?;
    }
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 17 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("advance schema to version 17: {error}"))
        })?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit ledger migration: {error}")))
}

async fn migrate_v17_to_v18(connection: &mut turso::Connection) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin temporal endpoint index migration: {error}"))
        })?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0018_temporal_endpoint_indexes.up.sql"
        ))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create temporal endpoint indexes: {error}"))
        })?;
    let mut timestamp_rows = transaction
        .query("SELECT unixepoch()", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time: {error}")))?;
    let timestamp = timestamp_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read migration time row: {error}")))?
        .ok_or_else(|| TursoStoreError::Snapshot("migration time is missing".to_owned()))?
        .get::<i64>(0)
        .map_err(|error| TursoStoreError::Backend(format!("decode migration time: {error}")))?;
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.to == 18)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("v18 migration is missing from registry".to_owned())
        })?;
    transaction
        .execute(
            "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted, checksum) VALUES (?1, ?2, ?3, 0, ?4)",
            (
                migration.to,
                migration.name,
                timestamp,
                migration_checksum(migration),
            ),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record temporal endpoint index migration: {error}"))
        })?;
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 18 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("advance schema to version 18: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit temporal endpoint index migration: {error}"))
    })
}

pub(super) async fn migrate_v2_to_v3(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("begin schema migration: {error}")))?;
    transaction
        .execute_batch("ALTER TABLE syntaxmesh_nodes ADD COLUMN name TEXT NOT NULL DEFAULT ''; ALTER TABLE syntaxmesh_nodes ADD COLUMN owner_file BLOB; ALTER TABLE syntaxmesh_edges ADD COLUMN source BLOB; ALTER TABLE syntaxmesh_edges ADD COLUMN target BLOB;")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("add indexed graph columns: {error}")))?;
    let mut nodes = transaction
        .query("SELECT payload FROM syntaxmesh_nodes ORDER BY id", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read nodes for migration: {error}")))?;
    while let Some(row) = nodes
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read node migration row: {error}")))?
    {
        let node: Node = decode_blob(
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(format!("read node payload: {error}")))?,
            "node payload",
        )?;
        transaction
            .execute(
                "UPDATE syntaxmesh_nodes SET name = ?1, owner_file = ?2 WHERE id = ?3",
                (
                    node.name,
                    node.owner_file.map(|file| file.0.0.to_vec()),
                    node.id.0.0.to_vec(),
                ),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("backfill node index: {error}")))?;
    }
    let mut edges = transaction
        .query("SELECT payload FROM syntaxmesh_edges ORDER BY id", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read edges for migration: {error}")))?;
    while let Some(row) = edges
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read edge migration row: {error}")))?
    {
        let edge: Edge = decode_blob(
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(format!("read edge payload: {error}")))?,
            "edge payload",
        )?;
        transaction
            .execute(
                "UPDATE syntaxmesh_edges SET source = ?1, target = ?2 WHERE id = ?3",
                (
                    edge.source.0.0.to_vec(),
                    edge.target.0.0.to_vec(),
                    edge.id.0.0.to_vec(),
                ),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("backfill edge index: {error}")))?;
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [3_i64],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("record schema migration: {error}")))?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit schema migration: {error}")))
}

pub(super) async fn migrate_v3_to_v4(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("begin schema migration: {error}")))?;
    transaction
        .execute_batch("ALTER TABLE syntaxmesh_nodes ADD COLUMN terminal_name TEXT NOT NULL DEFAULT ''; CREATE INDEX syntaxmesh_nodes_terminal_name_idx ON syntaxmesh_nodes(terminal_name, id);")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("add terminal-name index: {error}")))?;
    let mut nodes = transaction
        .query("SELECT payload FROM syntaxmesh_nodes ORDER BY id", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read nodes for migration: {error}")))?;
    while let Some(row) = nodes
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read node migration row: {error}")))?
    {
        let node: Node = decode_blob(
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(format!("read node payload: {error}")))?,
            "node payload",
        )?;
        transaction
            .execute(
                "UPDATE syntaxmesh_nodes SET terminal_name = ?1 WHERE id = ?2",
                (terminal_name(&node.name), node.id.0.0.to_vec()),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("backfill terminal-name index: {error}"))
            })?;
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [4_i64],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("record schema migration: {error}")))?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit schema migration: {error}")))
}

pub(super) async fn migrate_v4_to_v5(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| TursoStoreError::Backend(format!("begin schema migration: {error}")))?;
    transaction
        .execute_batch("ALTER TABLE syntaxmesh_nodes ADD COLUMN provenance BLOB; ALTER TABLE syntaxmesh_edges ADD COLUMN provenance BLOB;")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("add provenance indexes: {error}")))?;
    let mut nodes = transaction
        .query("SELECT id, payload FROM syntaxmesh_nodes ORDER BY id", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read nodes for migration: {error}")))?;
    while let Some(row) = nodes
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read node migration row: {error}")))?
    {
        let node: Node = decode_blob(
            row.get_value(1)
                .map_err(|error| TursoStoreError::Backend(format!("read node payload: {error}")))?,
            "node payload",
        )?;
        transaction
            .execute(
                "UPDATE syntaxmesh_nodes SET provenance = ?1 WHERE id = ?2",
                (node.provenance.0.0.to_vec(), node.id.0.0.to_vec()),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("backfill node provenance: {error}"))
            })?;
    }
    let mut edges = transaction
        .query("SELECT id, payload FROM syntaxmesh_edges ORDER BY id", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read edges for migration: {error}")))?;
    while let Some(row) = edges
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read edge migration row: {error}")))?
    {
        let edge: Edge = decode_blob(
            row.get_value(1)
                .map_err(|error| TursoStoreError::Backend(format!("read edge payload: {error}")))?,
            "edge payload",
        )?;
        transaction
            .execute(
                "UPDATE syntaxmesh_edges SET provenance = ?1 WHERE id = ?2",
                (edge.provenance.0.0.to_vec(), edge.id.0.0.to_vec()),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("backfill edge provenance: {error}"))
            })?;
    }
    transaction
        .execute_batch("CREATE INDEX syntaxmesh_nodes_provenance_idx ON syntaxmesh_nodes(provenance, id); CREATE INDEX syntaxmesh_edges_provenance_idx ON syntaxmesh_edges(provenance, id);")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("create provenance indexes: {error}")))?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [5_i64],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("record schema migration: {error}")))?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit schema migration: {error}")))
}

pub(super) async fn migrate_v6_to_v7(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin temporal-index migration: {error}"))
        })?;
    transaction
        .execute("DELETE FROM syntaxmesh_fact_versions", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("clear temporal index for rebuild: {error}"))
        })?;
    let history = read_history_with_sequences(&transaction).await?;
    if history.is_empty() {
        if let Some(manifest) = read_single::<GenerationManifest>(
            &transaction,
            "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
        )
        .await?
        {
            let snapshot = read_canonical_snapshot(&transaction).await?;
            let anchor = GenerationHistoryEntry {
                manifest: manifest.clone(),
                delta: None,
                anchor: Some(snapshot.clone()),
            };
            transaction.execute("INSERT INTO syntaxmesh_generation_history (sequence, generation, payload) VALUES (1, ?1, ?2)", (manifest.generation.0.0.to_vec(), bincode::serialize(&anchor).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?)).await
                .map_err(|error| TursoStoreError::Backend(format!("write temporal migration anchor: {error}")))?;
            seed_temporal_snapshot(&transaction, &snapshot, 1).await?;
        }
    } else {
        for (sequence, entry) in &history {
            if let Some(snapshot) = &entry.anchor {
                seed_temporal_snapshot(&transaction, snapshot, *sequence).await?;
            }
            if let Some(delta) = &entry.delta {
                apply_temporal_delta(&transaction, delta, *sequence, None).await?;
            }
            if entry.anchor.is_none() && entry.delta.is_none() {
                return Err(TursoStoreError::Snapshot(
                    "history entry has neither delta nor checkpoint".to_owned(),
                ));
            }
        }
        let last_history = history.last().map(|(_, entry)| entry.manifest.generation);
        let current = read_single::<GenerationManifest>(
            &transaction,
            "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
        )
        .await?;
        if last_history != current.map(|manifest| manifest.generation) {
            return Err(TursoStoreError::Snapshot(
                "temporal migration history does not reach the current graph".to_owned(),
            ));
        }
    }
    transaction.execute_batch("CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_identity_time_idx ON syntaxmesh_fact_versions(fact_kind, fact_id, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_time_idx ON syntaxmesh_fact_versions(fact_kind, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_source_idx ON syntaxmesh_fact_versions(source_id, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_target_idx ON syntaxmesh_fact_versions(target_id, valid_from_sequence, valid_until_sequence);").await
        .map_err(|error| TursoStoreError::Backend(format!("create temporal indexes: {error}")))?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [7_i64],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record temporal schema version: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit temporal-index migration: {error}"))
    })
}

pub(super) async fn migrate_v7_to_v8(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin checkpoint migration: {error}"))
        })?;
    let history = read_history_with_sequences(&transaction).await?;
    for (sequence, entry) in history {
        if sequence == 1 || sequence % CHECKPOINT_INTERVAL == 0 {
            let snapshot = read_temporal_snapshot(&transaction, entry.manifest.generation).await?;
            transaction.execute(
                "INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (?1, ?2, ?3, ?4)",
                (sequence, entry.manifest.generation.0.0.to_vec(), bincode::serialize(&entry.manifest).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?, bincode::serialize(&snapshot).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
            ).await.map_err(|error| TursoStoreError::Backend(format!("write temporal checkpoint: {error}")))?;
        }
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [8_i64],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record checkpoint schema version: {error}"))
        })?;
    transaction
        .commit()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("commit checkpoint migration: {error}")))
}

pub(super) async fn migrate_v8_to_v9(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin persistent-root migration: {error}"))
        })?;
    transaction
        .execute_batch(
            "DELETE FROM syntaxmesh_temporal_roots; DELETE FROM syntaxmesh_temporal_tree_pages;",
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("clear persistent roots for rebuild: {error}"))
        })?;
    let history = read_history_with_sequences(&transaction).await?;
    for (sequence, entry) in history {
        let root_parent = if entry.anchor.is_some() {
            None
        } else {
            entry.manifest.parent
        };
        let mutations = if let Some(anchor) = &entry.anchor {
            snapshot_tree_mutations(anchor, entry.manifest.schema_version)?
        } else if let Some(delta) = &entry.delta {
            let parent_sequence = match entry.manifest.parent {
                Some(parent) => Some(read_parent_sequence(&transaction, parent).await?),
                None => None,
            };
            delta_tree_mutations(
                &transaction,
                delta,
                parent_sequence,
                entry.manifest.schema_version,
                None,
            )
            .await?
        } else {
            return Err(TursoStoreError::Snapshot(
                "history entry has neither delta nor checkpoint".to_owned(),
            ));
        };
        publish_persistent_root(
            &transaction,
            sequence,
            entry.manifest.generation,
            root_parent,
            &mutations,
        )
        .await?;
        let snapshot = read_persistent_snapshot(&transaction, entry.manifest.generation).await?;
        if snapshot_root_for_schema(
            entry.manifest.generation,
            &snapshot,
            entry.manifest.schema_version,
        )? != entry.manifest.graph_root
        {
            return Err(TursoStoreError::Snapshot(
                "rebuilt persistent root differs from its generation manifest".to_owned(),
            ));
        }
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [9_i64],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record persistent-root schema version: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit persistent-root migration: {error}"))
    })
}

pub(super) async fn migrate_v9_to_v10(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin observed-time migration: {error}"))
        })?;
    let observed_values = {
        let mut rows = transaction
            .query(
                "SELECT rowid, observed_at_unix_nanos FROM syntaxmesh_fact_versions WHERE observed_at_unix_nanos IS NOT NULL",
                (),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("read observed times: {error}")))?;
        let mut values = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|error| TursoStoreError::Backend(format!("read observed-time row: {error}")))?
        {
            let row_id = row.get::<i64>(0).map_err(|error| {
                TursoStoreError::Backend(format!("decode observed-time row ID: {error}"))
            })?;
            let encoded = match row.get_value(1).map_err(|error| {
                TursoStoreError::Backend(format!("decode observed-time value: {error}"))
            })? {
                turso::Value::Blob(value) => value,
                turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Text(_) => {
                    return Err(TursoStoreError::Snapshot(
                        "observed-time index value is not a blob".to_owned(),
                    ));
                }
            };
            let timestamp: u64 = bincode::deserialize(&encoded).map_err(|error| {
                TursoStoreError::Snapshot(format!("migrate malformed observation time: {error}"))
            })?;
            values.push((row_id, timestamp.to_be_bytes().to_vec()));
        }
        values
    };
    for (row_id, encoded) in observed_values {
        transaction
            .execute(
                "UPDATE syntaxmesh_fact_versions SET observed_at_unix_nanos = ?1 WHERE rowid = ?2",
                (encoded, row_id),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("rewrite observed-time index value: {error}"))
            })?;
    }
    transaction
        .execute_batch("CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_observed_time_idx ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id) WHERE observed_at_unix_nanos IS NOT NULL; UPDATE syntaxmesh_schema SET version = 10 WHERE id = 1;")
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create observed-time index: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit observed-time migration: {error}"))
    })
}

pub(super) async fn migrate_v10_to_v11(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin acceptance-time migration: {error}"))
        })?;
    transaction
        .execute_batch("CREATE TABLE IF NOT EXISTS syntaxmesh_generation_acceptance (generation BLOB PRIMARY KEY NOT NULL, accepted_at_unix_nanos BLOB NOT NULL); CREATE INDEX IF NOT EXISTS syntaxmesh_generation_acceptance_time_idx ON syntaxmesh_generation_acceptance(accepted_at_unix_nanos, generation); UPDATE syntaxmesh_schema SET version = 11 WHERE id = 1;")
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create generation acceptance table: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit acceptance-time migration: {error}"))
    })
}

pub(super) async fn migrate_v11_to_v12(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin observation-index migration: {error}"))
        })?;
    transaction
        .execute_batch("DROP INDEX IF EXISTS syntaxmesh_fact_versions_observed_time_idx; CREATE INDEX syntaxmesh_fact_versions_observed_time_idx ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id, valid_from_sequence) WHERE observed_at_unix_nanos IS NOT NULL; UPDATE syntaxmesh_schema SET version = 12 WHERE id = 1;")
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create ordered observation index: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit observation-index migration: {error}"))
    })
}

pub(super) async fn migrate_v12_to_v13(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin change-event migration: {error}"))
        })?;
    transaction
        .execute_batch("CREATE TABLE IF NOT EXISTS syntaxmesh_change_events (generation BLOB PRIMARY KEY NOT NULL, event_id BLOB NOT NULL UNIQUE, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_change_event_facts (generation BLOB NOT NULL, generation_sequence INTEGER NOT NULL, fact_kind INTEGER NOT NULL, fact_id BLOB NOT NULL, change_kind INTEGER NOT NULL, PRIMARY KEY (generation, fact_kind, fact_id)); CREATE INDEX IF NOT EXISTS syntaxmesh_change_event_fact_lookup_idx ON syntaxmesh_change_event_facts(fact_kind, fact_id, generation_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_change_events_generation_idx ON syntaxmesh_change_events(generation); UPDATE syntaxmesh_schema SET version = 13 WHERE id = 1;")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("create change-event indexes: {error}")))?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit change-event migration: {error}"))
    })
}

pub(super) async fn migrate_v13_to_v14(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin explicit ChangeSet migration: {error}"))
        })?;
    transaction
        .execute_batch("CREATE TABLE IF NOT EXISTS syntaxmesh_generation_lineage (sequence INTEGER PRIMARY KEY NOT NULL, generation BLOB NOT NULL UNIQUE, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS syntaxmesh_change_set_versions (change_set_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, payload BLOB NOT NULL, PRIMARY KEY (change_set_id, valid_from_sequence)); CREATE TABLE IF NOT EXISTS syntaxmesh_change_set_membership_versions (change_set_id BLOB NOT NULL, event_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, provenance_id BLOB NOT NULL, PRIMARY KEY (change_set_id, event_id, valid_from_sequence)); CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_validity_idx ON syntaxmesh_change_set_versions(change_set_id, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_membership_set_idx ON syntaxmesh_change_set_membership_versions(change_set_id, valid_from_sequence, valid_until_sequence, event_id); CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_membership_event_idx ON syntaxmesh_change_set_membership_versions(event_id, valid_from_sequence, valid_until_sequence, change_set_id);")
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create explicit ChangeSet tables: {error}"))
        })?;
    let history = read_many::<GenerationHistoryEntry>(
        &transaction,
        "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence",
    )
    .await?;
    for (index, entry) in history.iter().enumerate() {
        let lineage = GenerationLineageEntry {
            generation: entry.manifest.generation,
            delta: ChangeSetDelta::default(),
        };
        transaction
            .execute(
                "INSERT OR IGNORE INTO syntaxmesh_generation_lineage (sequence, generation, payload) VALUES (?1, ?2, ?3)",
                (
                    i64::try_from(index.saturating_add(1)).unwrap_or(i64::MAX),
                    lineage.generation.0.0.to_vec(),
                    bincode::serialize(&lineage)
                        .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
                ),
            )
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("backfill empty ChangeSet lineage: {error}"))
            })?;
    }
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 14 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record Turso schema version 14: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit explicit ChangeSet migration: {error}"))
    })
}

pub(super) async fn migrate_v14_to_v15(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin consequence-edge migration: {error}"))
        })?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0015_evidence_consequence_edges.up.sql"
        ))
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("create consequence-edge schema: {error}"))
        })?;
    let history = read_many::<GenerationHistoryEntry>(
        &transaction,
        "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence",
    )
    .await?;
    for (index, history_entry) in history.iter().enumerate() {
        let consequence = GenerationConsequenceEntry {
            generation: history_entry.manifest.generation,
            delta: ConsequenceDelta::default(),
        };
        let sequence = i64::try_from(index.saturating_add(1)).map_err(|error| {
            TursoStoreError::Snapshot(format!("generation sequence overflow: {error}"))
        })?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO syntaxmesh_generation_consequences (sequence, generation, payload) VALUES (?1, ?2, ?3)",
                (sequence, consequence.generation.0.0.to_vec(), bincode::serialize(&consequence).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(format!("backfill empty consequence journal: {error}")))?;
    }
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 15 WHERE id = 1", ())
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("record Turso schema version 15: {error}"))
        })?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit consequence-edge migration: {error}"))
    })
}

pub(super) async fn migrate_v15_to_v16(
    connection: &mut turso::Connection,
) -> Result<(), TursoStoreError> {
    let transaction = connection
        .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("begin acceptance-prefix migration: {error}"))
        })?;
    let mut columns = transaction
        .query("PRAGMA table_info('syntaxmesh_generation_acceptance')", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("inspect acceptance schema: {error}")))?;
    let mut has_prefix = false;
    while let Some(row) = columns
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read acceptance schema: {error}")))?
    {
        if row.get_value(1).ok()
            == Some(turso::Value::Text("accepted_through_unix_nanos".to_owned()))
        {
            has_prefix = true;
        }
    }
    if !has_prefix {
        transaction
            .execute_batch(include_str!(
                "../../migrations/0016_generation_acceptance_prefix.up.sql"
            ))
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("add acceptance-prefix column: {error}"))
            })?;
    }
    let history = read_many::<GenerationHistoryEntry>(
        &transaction,
        "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence",
    )
    .await?;
    let mut maximum = 0_u64;
    let mut known = true;
    for entry in history {
        let mut rows = transaction.query(
            "SELECT accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
            [entry.manifest.generation.0.0.to_vec()],
        ).await.map_err(|error| TursoStoreError::Backend(format!("read legacy acceptance during backfill: {error}")))?;
        let accepted = if let Some(row) = rows.next().await.map_err(|error| {
            TursoStoreError::Backend(format!("read legacy acceptance row: {error}"))
        })? {
            match row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("decode legacy acceptance: {error}"))
            })? {
                turso::Value::Blob(bytes) => {
                    let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                        TursoStoreError::Snapshot(format!(
                            "acceptance timestamp must be eight bytes, got {}",
                            bytes.len()
                        ))
                    })?;
                    Some(AcceptanceTime(u64::from_be_bytes(bytes)))
                }
                other @ (turso::Value::Null
                | turso::Value::Integer(_)
                | turso::Value::Real(_)
                | turso::Value::Text(_)) => {
                    return Err(TursoStoreError::Snapshot(format!(
                        "invalid acceptance-time storage type: {other:?}"
                    )));
                }
            }
        } else {
            None
        };
        match accepted {
            Some(time) if known => {
                maximum = maximum.max(time.0);
                transaction.execute(
                    "UPDATE syntaxmesh_generation_acceptance SET accepted_through_unix_nanos = ?1 WHERE generation = ?2",
                    (maximum.to_be_bytes().to_vec(), entry.manifest.generation.0.0.to_vec()),
                ).await.map_err(|error| TursoStoreError::Backend(format!("backfill acceptance prefix: {error}")))?;
            }
            _ => known = false,
        }
    }
    transaction
        .execute("UPDATE syntaxmesh_schema SET version = 16 WHERE id = 1", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("record schema version 16: {error}")))?;
    transaction.commit().await.map_err(|error| {
        TursoStoreError::Backend(format!("commit acceptance-prefix migration: {error}"))
    })
}
