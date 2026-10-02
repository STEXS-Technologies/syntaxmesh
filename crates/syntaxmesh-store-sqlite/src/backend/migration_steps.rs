//! Version-to-version SQLite migration implementations.
//!
//! The ordered registry lives in `migrations.rs`; this module owns the
//! transactional data/schema work for each transition.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use syntaxmesh_core::{
    ChangeSetDelta, ConsequenceDelta, Edge, EdgeId, GenerationConsequenceEntry,
    GenerationHistoryEntry, GenerationLineageEntry, Node, NodeId,
};
use syntaxmesh_store::{GraphStore, IncidenceMutation, StoreError, node_kind_storage_code};

use super::tree_store::{publish_incidence_root, snapshot_incidence_changes};
use super::{
    CHECKPOINT_INTERVAL, SCHEMA_MIGRATIONS, apply_temporal_delta,
    backfill_change_events_in_transaction, commit_schema_migration, decode_optional_time,
    delta_tree_mutations, encode, initialize_history_anchor_in_transaction, migration_checksum,
    publish_persistent_root, read_canonical_snapshot, read_generation_history_rows, read_manifest,
    read_parent_sequence, read_persistent_snapshot, read_temporal_snapshot,
    record_schema_migration, restore_store, seed_temporal_snapshot, snapshot_root_for_schema,
    snapshot_tree_mutations, sql_error,
};

pub(super) fn migrate_v1_to_v2(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 2 WHERE id = 1 AND version = 1",
            [],
        )
        .map_err(sql_error)?;
    if transaction.changes() != 1 {
        return Err(StoreError::Integrity(
            "schema v1 migration did not advance exactly one version row".to_owned(),
        ));
    }
    record_schema_migration(&transaction, 2)?;
    commit_schema_migration(transaction, path, 2)
}

pub(super) fn migrate_v2_to_v3(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM syntaxmesh_fact_versions", [])
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM syntaxmesh_graph_checkpoints", [])
        .map_err(sql_error)?;
    let history = read_generation_history_rows(&transaction)?;
    if history.is_empty() {
        if let Some(manifest) = read_manifest(&transaction)? {
            let snapshot = read_canonical_snapshot(&transaction)?;
            let anchor = GenerationHistoryEntry {
                manifest: manifest.clone(),
                delta: None,
                anchor: Some(snapshot.clone()),
            };
            transaction
                .execute(
                    "INSERT INTO syntaxmesh_generation_history (sequence, generation, payload) VALUES (1, ?1, ?2)",
                    params![manifest.generation.0.0.as_slice(), encode(&anchor)?],
                )
                .map_err(sql_error)?;
            seed_temporal_snapshot(&transaction, &snapshot, 1)?;
            transaction.execute(
                "INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (1, ?1, ?2, ?3)",
                params![manifest.generation.0.0.as_slice(), encode(&manifest)?, encode(&snapshot)?],
            ).map_err(sql_error)?;
        }
    } else {
        for (sequence, entry) in &history {
            if let Some(anchor) = &entry.anchor {
                seed_temporal_snapshot(&transaction, anchor, *sequence)?;
            }
            if let Some(delta) = &entry.delta {
                apply_temporal_delta(&transaction, delta, *sequence, None)?;
            }
            if entry.anchor.is_none() && entry.delta.is_none() {
                return Err(StoreError::Integrity(
                    "history entry has neither delta nor checkpoint".to_owned(),
                ));
            }
        }
        let latest = history.last().map(|(_, entry)| entry.manifest.generation);
        let current = read_manifest(&transaction)?.map(|manifest| manifest.generation);
        if latest != current {
            return Err(StoreError::Integrity(
                "temporal migration history does not reach current manifest".to_owned(),
            ));
        }
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [3_i64],
        )
        .map_err(sql_error)?;
    transaction
        .execute_batch("CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_identity_time_idx ON syntaxmesh_fact_versions(fact_kind, fact_id, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_time_idx ON syntaxmesh_fact_versions(fact_kind, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_source_idx ON syntaxmesh_fact_versions(source_id, valid_from_sequence, valid_until_sequence); CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_target_idx ON syntaxmesh_fact_versions(target_id, valid_from_sequence, valid_until_sequence);")
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 3)?;
    commit_schema_migration(transaction, path, 3)
}

pub(super) fn migrate_v3_to_v4(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM syntaxmesh_fact_versions", [])
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM syntaxmesh_graph_checkpoints", [])
        .map_err(sql_error)?;
    let history = read_generation_history_rows(&transaction)?;
    if history.is_empty() {
        if let Some(manifest) = read_manifest(&transaction)? {
            let snapshot = read_canonical_snapshot(&transaction)?;
            let anchor = GenerationHistoryEntry {
                manifest: manifest.clone(),
                delta: None,
                anchor: Some(snapshot.clone()),
            };
            transaction
                .execute(
                    "INSERT INTO syntaxmesh_generation_history (sequence, generation, payload) VALUES (1, ?1, ?2)",
                    params![manifest.generation.0.0.as_slice(), encode(&anchor)?],
                )
                .map_err(sql_error)?;
            seed_temporal_snapshot(&transaction, &snapshot, 1)?;
            transaction
                .execute(
                    "INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (1, ?1, ?2, ?3)",
                    params![manifest.generation.0.0.as_slice(), encode(&manifest)?, encode(&snapshot)?],
                )
                .map_err(sql_error)?;
        }
    } else {
        for (sequence, entry) in &history {
            if let Some(snapshot) = &entry.anchor {
                seed_temporal_snapshot(&transaction, snapshot, *sequence)?;
            }
            if let Some(delta) = &entry.delta {
                apply_temporal_delta(&transaction, delta, *sequence, None)?;
            }
            if entry.anchor.is_none() && entry.delta.is_none() {
                return Err(StoreError::Integrity(
                    "history entry has neither delta nor checkpoint".to_owned(),
                ));
            }
        }
        let latest = history.last().map(|(_, entry)| entry.manifest.generation);
        let current = read_manifest(&transaction)?.map(|manifest| manifest.generation);
        if latest != current {
            return Err(StoreError::Integrity(
                "temporal migration history does not reach current manifest".to_owned(),
            ));
        }
    }
    for (sequence, entry) in history {
        if sequence == 1 || sequence % CHECKPOINT_INTERVAL == 0 {
            let snapshot = read_temporal_snapshot(&transaction, entry.manifest.generation)?;
            transaction.execute(
                "INSERT OR REPLACE INTO syntaxmesh_graph_checkpoints (sequence, generation, manifest, payload) VALUES (?1, ?2, ?3, ?4)",
                params![sequence, entry.manifest.generation.0.0.as_slice(), encode(&entry.manifest)?, encode(&snapshot)?],
            ).map_err(sql_error)?;
        }
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [4_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 4)?;
    commit_schema_migration(transaction, path, 4)
}

pub(super) fn migrate_v4_to_v5(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(
            "DELETE FROM syntaxmesh_temporal_roots; DELETE FROM syntaxmesh_temporal_tree_pages;",
        )
        .map_err(sql_error)?;
    let history = read_generation_history_rows(&transaction)?;
    for (sequence, entry) in history {
        let root_parent = if entry.anchor.is_some() {
            None
        } else {
            entry.manifest.parent
        };
        let mutations = if let Some(anchor) = &entry.anchor {
            snapshot_tree_mutations(anchor, entry.manifest.schema_version)?
        } else if let Some(delta) = &entry.delta {
            let parent_sequence = entry
                .manifest
                .parent
                .map(|parent| read_parent_sequence(&transaction, parent))
                .transpose()?;
            delta_tree_mutations(
                &transaction,
                delta,
                parent_sequence,
                entry.manifest.schema_version,
                None,
            )?
        } else {
            return Err(StoreError::Integrity(
                "history entry has neither delta nor checkpoint".to_owned(),
            ));
        };
        publish_persistent_root(
            &transaction,
            sequence,
            entry.manifest.generation,
            root_parent,
            &mutations,
        )?;
        let snapshot = read_persistent_snapshot(&transaction, entry.manifest.generation)?;
        if snapshot_root_for_schema(
            entry.manifest.generation,
            &snapshot,
            entry.manifest.schema_version,
        )? != entry.manifest.graph_root
        {
            return Err(StoreError::Integrity(
                "rebuilt persistent root differs from its generation manifest".to_owned(),
            ));
        }
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [5_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 5)?;
    commit_schema_migration(transaction, path, 5)
}

pub(super) fn migrate_v5_to_v6(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    let observed_values = {
        let mut statement = transaction
            .prepare("SELECT rowid, observed_at_unix_nanos FROM syntaxmesh_fact_versions WHERE observed_at_unix_nanos IS NOT NULL")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(sql_error)?;
        rows.map(|row| row.map_err(sql_error))
            .collect::<Result<Vec<_>, _>>()?
    };
    for (row_id, encoded) in observed_values {
        let timestamp: u64 = bincode::deserialize(&encoded).map_err(|error| {
            StoreError::Integrity(format!(
                "cannot migrate malformed observation time: {error}"
            ))
        })?;
        transaction
            .execute(
                "UPDATE syntaxmesh_fact_versions SET observed_at_unix_nanos = ?1 WHERE rowid = ?2",
                params![timestamp.to_be_bytes().as_slice(), row_id],
            )
            .map_err(sql_error)?;
    }
    transaction
        .execute_batch("CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_observed_time_idx ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id) WHERE observed_at_unix_nanos IS NOT NULL;")
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [6_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 6)?;
    commit_schema_migration(transaction, path, 6)
}

pub(super) fn migrate_v6_to_v7(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch("CREATE TABLE IF NOT EXISTS syntaxmesh_generation_acceptance (generation BLOB PRIMARY KEY NOT NULL, accepted_at_unix_nanos BLOB NOT NULL); CREATE INDEX IF NOT EXISTS syntaxmesh_generation_acceptance_time_idx ON syntaxmesh_generation_acceptance(accepted_at_unix_nanos, generation);")
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [7_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 7)?;
    commit_schema_migration(transaction, path, 7)
}

pub(super) fn migrate_v7_to_v8(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch("DROP INDEX IF EXISTS syntaxmesh_fact_versions_observed_time_idx; CREATE INDEX syntaxmesh_fact_versions_observed_time_idx ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id, valid_from_sequence) WHERE observed_at_unix_nanos IS NOT NULL;")
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [8_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 8)?;
    commit_schema_migration(transaction, path, 8)
}

pub(super) fn migrate_v8_to_v9(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0009_indexed_change_events.up.sql"
        ))
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [9_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 9)?;
    commit_schema_migration(transaction, path, 9)
}

pub(super) fn migrate_v9_to_v10(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0010_schema_migration_ledger.up.sql"
        ))
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [10_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 10)?;
    commit_schema_migration(transaction, path, 10)
}

pub(super) fn migrate_v10_to_v11(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0011_sql_migration_checksums.up.sql"
        ))
        .map_err(sql_error)?;
    for version in [9_i64, 10_i64] {
        let migration = SCHEMA_MIGRATIONS
            .iter()
            .find(|migration| migration.to == version)
            .ok_or_else(|| {
                StoreError::Integrity(format!(
                    "SQLite migration registry has no SQL migration {version}"
                ))
            })?;
        let checksum = migration_checksum(migration).ok_or_else(|| {
            StoreError::Integrity(format!(
                "SQLite migration {version} has no registered SQL checksum"
            ))
        })?;
        let updated = transaction
            .execute(
                "UPDATE syntaxmesh_schema_migrations SET checksum = ?1 WHERE version = ?2 AND name = ?3",
                params![checksum, version, migration.name],
            )
            .map_err(sql_error)?;
        if updated != 1 {
            return Err(StoreError::Integrity(format!(
                "SQLite migration ledger has no version {version} to checksum"
            )));
        }
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [11_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 11)?;
    commit_schema_migration(transaction, path, 11)
}

pub(super) fn migrate_v11_to_v12(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0012_explicit_startup_compatibility.up.sql"
        ))
        .map_err(sql_error)?;
    let memory = restore_store(&transaction)?;
    initialize_history_anchor_in_transaction(&transaction, &memory)?;
    backfill_change_events_in_transaction(&transaction, &memory)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [12_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 12)?;
    commit_schema_migration(transaction, path, 12)
}

pub(super) fn migrate_v12_to_v13(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0013_explicit_change_set_lineage.up.sql"
        ))
        .map_err(sql_error)?;
    let memory = restore_store(&transaction)?;
    for (index, history_entry) in memory.generation_history()?.iter().enumerate() {
        let sequence = i64::try_from(index.saturating_add(1)).map_err(|error| {
            StoreError::Integrity(format!("history sequence overflow: {error}"))
        })?;
        let lineage = GenerationLineageEntry {
            generation: history_entry.manifest.generation,
            delta: ChangeSetDelta::default(),
        };
        transaction
            .execute(
                "INSERT OR IGNORE INTO syntaxmesh_generation_lineage (sequence, generation, payload) VALUES (?1, ?2, ?3)",
                params![sequence, lineage.generation.0.0.as_slice(), encode(&lineage)?],
            )
            .map_err(sql_error)?;
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1",
            [13_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 13)?;
    commit_schema_migration(transaction, path, 13)
}

pub(super) fn migrate_v13_to_v14(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0014_evidence_consequence_edges.up.sql"
        ))
        .map_err(sql_error)?;
    let memory = restore_store(&transaction)?;
    for (index, history_entry) in memory.generation_history()?.iter().enumerate() {
        let sequence = i64::try_from(index.saturating_add(1)).map_err(|error| {
            StoreError::Integrity(format!("history sequence overflow: {error}"))
        })?;
        let consequence = GenerationConsequenceEntry {
            generation: history_entry.manifest.generation,
            delta: ConsequenceDelta::default(),
        };
        transaction
            .execute(
                "INSERT OR IGNORE INTO syntaxmesh_generation_consequences (sequence, generation, payload) VALUES (?1, ?2, ?3)",
                params![sequence, consequence.generation.0.0.as_slice(), encode(&consequence)?],
            )
            .map_err(sql_error)?;
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1 AND version = ?2",
            params![14_i64, 13_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 14)?;
    commit_schema_migration(transaction, path, 14)
}

pub(super) fn migrate_v14_to_v15(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    let has_prefix = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('syntaxmesh_generation_acceptance') WHERE name = 'accepted_through_unix_nanos')",
        [],
        |row| row.get::<_, bool>(0),
    ).map_err(sql_error)?;
    if !has_prefix {
        transaction
            .execute_batch(include_str!(
                "../../migrations/0015_generation_acceptance_prefix.up.sql"
            ))
            .map_err(sql_error)?;
    }
    let history = read_generation_history_rows(&transaction)?;
    let mut maximum = 0_u64;
    let mut known = true;
    for (_, entry) in history {
        let accepted = transaction
            .query_row(
                "SELECT accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
                [entry.manifest.generation.0.0.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(sql_error)?;
        match accepted {
            Some(bytes) if known => {
                let value = decode_optional_time(Some(bytes), "accepted_at_unix_nanos")?
                    .ok_or_else(|| {
                        StoreError::Integrity("acceptance timestamp unexpectedly absent".to_owned())
                    })?;
                maximum = maximum.max(value);
                transaction.execute(
                    "UPDATE syntaxmesh_generation_acceptance SET accepted_through_unix_nanos = ?1 WHERE generation = ?2",
                    params![maximum.to_be_bytes().as_slice(), entry.manifest.generation.0.0.as_slice()],
                ).map_err(sql_error)?;
            }
            _ => known = false,
        }
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = ?1 WHERE id = 1 AND version = ?2",
            params![15_i64, 14_i64],
        )
        .map_err(sql_error)?;
    record_schema_migration(&transaction, 15)?;
    commit_schema_migration(transaction, path, 15)
}

pub(super) fn migrate_v15_to_v16(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    let has_node_kind = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('syntaxmesh_nodes') WHERE name = 'node_kind')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
    if has_node_kind {
        transaction
            .execute_batch(
                "CREATE INDEX IF NOT EXISTS syntaxmesh_nodes_kind_idx ON syntaxmesh_nodes(node_kind, id)",
            )
            .map_err(sql_error)?;
    } else {
        transaction
            .execute_batch(include_str!(
                "../../migrations/0016_indexed_node_kinds.up.sql"
            ))
            .map_err(sql_error)?;
    }
    let rows = {
        let mut statement = transaction
            .prepare("SELECT id, payload FROM syntaxmesh_nodes ORDER BY id")
            .map_err(sql_error)?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?
    };
    for (id, payload) in rows {
        let node: Node = super::decode(&payload)?;
        if id != node.id.0.0 {
            return Err(StoreError::Integrity(
                "node row ID disagrees with canonical payload during kind-index migration"
                    .to_owned(),
            ));
        }
        transaction
            .execute(
                "UPDATE syntaxmesh_nodes SET node_kind = ?1 WHERE id = ?2",
                params![node_kind_storage_code(&node.kind), id],
            )
            .map_err(sql_error)?;
    }
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 16 WHERE id = 1 AND version = 15",
            [],
        )
        .map_err(sql_error)?;
    if transaction.changes() != 1 {
        return Err(StoreError::Integrity(
            "SQLite node-kind migration did not advance exactly one version row".to_owned(),
        ));
    }
    let ledger_entry_exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM syntaxmesh_schema_migrations WHERE version = 16)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
    if !ledger_entry_exists {
        record_schema_migration(&transaction, 16)?;
    }
    commit_schema_migration(transaction, path, 16)
}

pub(super) fn migrate_v16_to_v17(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 17 WHERE id = 1 AND version = 16",
            [],
        )
        .map_err(sql_error)?;
    if transaction.changes() != 1 {
        return Err(StoreError::Integrity(
            "SQLite diagnostic migration did not advance exactly one version row".to_owned(),
        ));
    }
    record_schema_migration(&transaction, 17)?;
    commit_schema_migration(transaction, path, 17)
}

pub(super) fn migrate_v17_to_v18(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0018_remove_temporal_endpoint_indexes.up.sql"
        ))
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 18 WHERE id = 1 AND version = 17",
            [],
        )
        .map_err(sql_error)?;
    if transaction.changes() != 1 {
        return Err(StoreError::Integrity(
            "SQLite temporal endpoint-index migration did not advance exactly one version row"
                .to_owned(),
        ));
    }
    record_schema_migration(&transaction, 18)?;
    commit_schema_migration(transaction, path, 18)
}

pub(super) fn migrate_v18_to_v19(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0019_temporal_incidence_roots.up.sql"
        ))
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM syntaxmesh_temporal_incidence_roots", [])
        .map_err(sql_error)?;

    let history = {
        let mut statement = transaction
            .prepare(
                "SELECT sequence, payload FROM syntaxmesh_generation_history ORDER BY sequence",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(sql_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)?
    };
    let mut active_edges = BTreeMap::<EdgeId, Edge>::new();
    let mut endpoint_edges = BTreeMap::<NodeId, BTreeSet<EdgeId>>::new();
    let mut parent = None;
    for (sequence, payload) in history {
        let entry: GenerationHistoryEntry = super::decode(&payload)?;
        let changes = if let Some(anchor) = &entry.anchor {
            active_edges.clear();
            endpoint_edges.clear();
            let changes = snapshot_incidence_changes(anchor);
            for edge in &anchor.edges {
                index_edge(edge.clone(), &mut active_edges, &mut endpoint_edges);
            }
            parent = None;
            changes
        } else {
            let delta = entry.delta.as_ref().ok_or_else(|| {
                StoreError::Integrity(
                    "generation history entry lacks both an anchor and a delta during incidence backfill"
                        .to_owned(),
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
        )?;
        parent = Some(entry.manifest.generation);
    }

    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 19 WHERE id = 1 AND version = 18",
            [],
        )
        .map_err(sql_error)?;
    if transaction.changes() != 1 {
        return Err(StoreError::Integrity(
            "SQLite incidence-root migration did not advance exactly one version row".to_owned(),
        ));
    }
    record_schema_migration(&transaction, 19)?;
    commit_schema_migration(transaction, path, 19)
}

pub(super) fn migrate_v19_to_v20(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0020_fact_history_end_lookup.up.sql"
        ))
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 20 WHERE id = 1 AND version = 19",
            [],
        )
        .map_err(sql_error)?;
    if transaction.changes() != 1 {
        return Err(StoreError::Integrity(
            "SQLite fact-history end-lookup migration did not advance exactly one version row"
                .to_owned(),
        ));
    }
    record_schema_migration(&transaction, 20)?;
    commit_schema_migration(transaction, path, 20)
}

pub(super) fn migrate_v20_to_v21(
    connection: &mut Connection,
    path: &Path,
) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(include_str!(
            "../../migrations/0021_fact_history_generation_lookup.up.sql"
        ))
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE syntaxmesh_schema SET version = 21 WHERE id = 1 AND version = 20",
            [],
        )
        .map_err(sql_error)?;
    if transaction.changes() != 1 {
        return Err(StoreError::Integrity(
            "SQLite fact-history generation-index migration did not advance exactly one version row"
                .to_owned(),
        ));
    }
    record_schema_migration(&transaction, 21)?;
    commit_schema_migration(transaction, path, 21)
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
