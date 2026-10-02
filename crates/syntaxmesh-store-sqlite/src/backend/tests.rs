use super::*;
use syntaxmesh_core::{
    ChangeSet, ChangeSetKind, ChangeSetMembership, ChangeSetMembershipKey,
    ChangeSetMembershipRemoval, ConsequenceEdge, ConsequenceEdgeId, FactVersionRef,
};

#[test]
fn schema_two_tree_decoder_reads_legacy_json_and_compact_bincode() -> Result<(), StoreError> {
    let fact = vec![1_u64, 256, u64::MAX];
    let legacy_json = serde_json::to_vec(&fact)
        .map_err(|error| StoreError::Backend(format!("encode legacy tree fixture: {error}")))?;
    let compact = encode_tree_fact(&fact, 2)?;
    if decode_tree_fact::<Vec<u64>>(&legacy_json, 2)? != fact
        || decode_tree_fact::<Vec<u64>>(&compact, 2)? != fact
    {
        return Err(StoreError::Integrity(
            "schema-v2 tree decoder did not accept both stored payload formats".to_owned(),
        ));
    }
    Ok(())
}

fn open_migrated(path: impl AsRef<Path>) -> Result<SqliteGraphStore, StoreError> {
    SqliteGraphStore::migrate(path.as_ref())?;
    SqliteGraphStore::open(path)
}

fn create_schema_v10_fixture(path: &Path) -> Result<(), StoreError> {
    let store = open_migrated(path)?;
    store
        .connection
        .execute_batch(
            "DROP TABLE syntaxmesh_schema_migrations;
                 CREATE TABLE syntaxmesh_schema_migrations (
                    version INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    applied_at_unix_seconds INTEGER,
                    adopted INTEGER NOT NULL CHECK (adopted IN (0, 1))
                 );
                 UPDATE syntaxmesh_schema SET version = 10 WHERE id = 1;",
        )
        .map_err(sql_error)?;
    for migration in SCHEMA_MIGRATIONS
        .iter()
        .filter(|migration| migration.to <= 10)
    {
        store
                .connection
                .execute(
                    "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted) VALUES (?1, ?2, NULL, 1)",
                    params![migration.to, migration.name],
                )
                .map_err(sql_error)?;
    }
    drop(store);
    Ok(())
}

fn concurrently_open(path: &Path, point: migration_test_sync::Point) -> Result<(), String> {
    use std::sync::{Arc, Barrier};

    let barrier = Arc::new(Barrier::new(2));
    let _gate = migration_test_sync::arm(path, point, Arc::clone(&barrier));
    let handles = (0..2)
        .map(|_| {
            let path = path.to_path_buf();
            std::thread::spawn(move || open_migrated(path).map(|_| ()))
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle
            .join()
            .map_err(|panic| format!("concurrent SQLite open panicked: {panic:?}"))?
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[test]
fn simultaneous_sqlite_opens_serialize_bootstrap_and_migration() -> Result<(), StoreError> {
    let suffix = std::process::id();
    let fresh_path =
        std::env::temp_dir().join(format!("syntaxmesh-sqlite-{suffix}-concurrent-fresh.db"));
    concurrently_open(&fresh_path, migration_test_sync::Point::AfterSchemaProbe)
        .map_err(StoreError::Backend)?;
    std::fs::remove_file(&fresh_path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;

    let upgrade_path =
        std::env::temp_dir().join(format!("syntaxmesh-sqlite-{suffix}-concurrent-upgrade.db"));
    let store = open_migrated(&upgrade_path)?;
    store
        .connection
        .execute_batch(
            "DROP TABLE syntaxmesh_schema_migrations;
                 CREATE TABLE syntaxmesh_schema_migrations (
                    version INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    applied_at_unix_seconds INTEGER,
                    adopted INTEGER NOT NULL CHECK (adopted IN (0, 1))
                 );
                 UPDATE syntaxmesh_schema SET version = 10 WHERE id = 1;",
        )
        .map_err(sql_error)?;
    for migration in SCHEMA_MIGRATIONS
        .iter()
        .filter(|migration| migration.to <= 10)
    {
        store
                .connection
                .execute(
                    "INSERT INTO syntaxmesh_schema_migrations (version, name, applied_at_unix_seconds, adopted) VALUES (?1, ?2, NULL, 1)",
                    params![migration.to, migration.name],
                )
                .map_err(sql_error)?;
    }
    drop(store);
    concurrently_open(&upgrade_path, migration_test_sync::Point::AfterVersionRead)
        .map_err(StoreError::Backend)?;
    let migrated = open_migrated(&upgrade_path)?;
    let version = read_schema_version(&migrated.connection)?.ok_or_else(|| {
        StoreError::Integrity("concurrent SQLite migration removed schema version".to_owned())
    })?;
    drop(migrated);
    std::fs::remove_file(&upgrade_path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if version != SCHEMA_VERSION {
        return Err(StoreError::Integrity(format!(
            "concurrent SQLite open stopped at schema {version}"
        )));
    }
    Ok(())
}

#[test]
fn schema_v8_migration_backfills_fact_linked_change_events() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-change-event-migration.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"event-migration-repository"]);
    let worktree = WorktreeId::derive(&[b"event-migration-worktree"]);
    let generation = GenerationId::derive(&[b"event-migration-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"event-migration-provenance"]),
        producer_namespace: "test.event-migration".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"event-migration-node"]),
        kind: syntaxmesh_core::NodeKind::Function,
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

    let setup_connection = Connection::open(&path).map_err(sql_error)?;
    setup_connection
            .execute_batch("DROP TABLE syntaxmesh_change_event_facts; DROP TABLE syntaxmesh_change_events; DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 8 WHERE id = 1;")
            .map_err(sql_error)?;
    drop(setup_connection);

    let migrated = open_migrated(&path)?;
    let actual = migrated.change_events_for_fact(FactRef::Node(node.id), None, 10)?;
    let schema_version = migrated
        .connection
        .query_row(
            "SELECT version FROM syntaxmesh_schema WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    let (adopted_count, executed_count) = migrated
            .connection
            .query_row(
                "SELECT SUM(adopted), SUM(applied_at_unix_seconds IS NOT NULL) FROM syntaxmesh_schema_migrations",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .map_err(sql_error)?;
    drop(migrated);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove migration fixture: {error}")))?;
    if expected != actual
        || schema_version != SCHEMA_VERSION
        || adopted_count != 7
        || executed_count != 13
    {
        return Err(StoreError::Integrity(
            "SQLite change-event migration did not preserve lineage and migration ledger history"
                .to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn explicit_change_set_membership_is_atomic_indexed_and_restartable() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-explicit-change-set.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"changeset-repository"]);
    let worktree = WorktreeId::derive(&[b"changeset-worktree"]);
    let first = GenerationId::derive(&[b"changeset-first"]);
    let second = GenerationId::derive(&[b"changeset-second"]);
    let third = GenerationId::derive(&[b"changeset-third"]);
    let fourth = GenerationId::derive(&[b"changeset-fourth"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"changeset-provenance"]),
        producer_namespace: "test.changeset".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let mut store = open_migrated(&path)?;
    let mut first_delta = empty_delta(repository, worktree, None, first, b"changeset-first-run");
    first_delta.upsert_provenance.push(provenance.clone());
    store.apply_delta(first_delta)?;
    let first_event = store
        .change_event(first)?
        .ok_or_else(|| StoreError::Integrity("first event is absent".to_owned()))?;

    let second_delta = empty_delta(
        repository,
        worktree,
        Some(first),
        second,
        b"changeset-second-run",
    );
    let second_event = store.memory.change_event_for_delta(&second_delta)?;
    let change_set = ChangeSetId::derive(&[b"explicit-change-set"]);
    let request = GraphDeltaWithLineage {
        graph: second_delta,
        lineage: ChangeSetDelta {
            upsert_sets: vec![ChangeSet {
                id: change_set,
                kind: ChangeSetKind::ManualGroup,
                title: Some("explicit group".to_owned()),
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
            assign_events: vec![
                ChangeSetMembership {
                    change_set,
                    event: first_event.id,
                    provenance: provenance.id,
                },
                ChangeSetMembership {
                    change_set,
                    event: second_event.id,
                    provenance: provenance.id,
                },
            ],
            unassign_events: Vec::new(),
        },
    };
    store.apply_delta_with_lineage(request, Some(AcceptanceTime(20)))?;
    let before_assignment = store.events_for_change_set(change_set, first, None, 10)?;
    let first_page = store.events_for_change_set(change_set, second, None, 1)?;
    if !before_assignment.items.is_empty()
        || first_page.items.len() != 1
        || first_page.next_cursor.is_none()
        || first_page.items.first().map(|item| item.event.id) != Some(first_event.id)
    {
        return Err(StoreError::Integrity(
            "ChangeSet query did not respect membership-valid generation or pagination".to_owned(),
        ));
    }
    let cursor = first_page.next_cursor.ok_or_else(|| {
        StoreError::Integrity("ChangeSet page omitted its continuation cursor".to_owned())
    })?;
    let continued = store.events_for_change_set(change_set, second, Some(cursor), 1)?;
    if continued.items.first().map(|item| item.event.id) != Some(second_event.id)
        || continued.next_cursor.is_some()
    {
        return Err(StoreError::Integrity(
            "ChangeSet cursor did not continue in stable event order".to_owned(),
        ));
    }
    drop(store);

    let mut reopened = SqliteGraphStore::open(&path).map_err(|error| {
        StoreError::Integrity(format!("reopen schema-v1 SQLite fixture: {error:?}"))
    })?;
    let replayed = reopened.events_for_change_set(change_set, second, None, 10)?;
    if replayed
        .items
        .iter()
        .map(|item| item.event.id)
        .collect::<Vec<_>>()
        != vec![first_event.id, second_event.id]
        || reopened.generation_lineage_history()?.len() != 2
    {
        return Err(StoreError::Integrity(
            "ChangeSet evidence or membership projection did not survive restart".to_owned(),
        ));
    }

    let removal = GraphDeltaWithLineage {
        graph: empty_delta(
            repository,
            worktree,
            Some(second),
            third,
            b"changeset-third-run",
        ),
        lineage: ChangeSetDelta {
            upsert_sets: vec![ChangeSet {
                id: change_set,
                kind: ChangeSetKind::ManualGroup,
                title: Some("updated group".to_owned()),
                originating_intent: None,
                parent_changes: Vec::new(),
                git_commits: Vec::new(),
                pull_requests: Vec::new(),
                issues: Vec::new(),
                adrs: Vec::new(),
                repositories: vec![repository],
                first_generation: first,
                last_generation: Some(third),
                provenance: provenance.id,
            }],
            unassign_events: vec![ChangeSetMembershipRemoval {
                key: ChangeSetMembershipKey {
                    change_set,
                    event: first_event.id,
                },
                provenance: provenance.id,
            }],
            ..ChangeSetDelta::default()
        },
    };
    reopened.apply_delta_with_lineage(removal, None)?;
    let current = reopened.events_for_change_set(change_set, third, None, 10)?;
    let historical = reopened.events_for_change_set(change_set, second, None, 10)?;
    let current_declaration = reopened.change_set_at(change_set, third)?;
    let historical_declaration = reopened.change_set_at(change_set, second)?;
    if current
        .items
        .iter()
        .map(|item| item.event.id)
        .collect::<Vec<_>>()
        != vec![second_event.id]
        || historical.items.len() != 2
        || current_declaration.as_ref().is_none_or(|version| {
            version.change_set.title.as_deref() != Some("updated group")
                || version.valid_from != third
                || version.valid_until.is_some()
        })
        || historical_declaration.as_ref().is_none_or(|version| {
            version.change_set.title.as_deref() != Some("explicit group")
                || version.valid_from != second
                || version.valid_until != Some(third)
        })
    {
        return Err(StoreError::Integrity(
            "membership removal changed history before its accepted generation".to_owned(),
        ));
    }

    let rejected = reopened.apply_delta_with_lineage(
        GraphDeltaWithLineage {
            graph: empty_delta(
                repository,
                worktree,
                Some(third),
                fourth,
                b"changeset-invalid-run",
            ),
            lineage: ChangeSetDelta {
                assign_events: vec![ChangeSetMembership {
                    change_set,
                    event: syntaxmesh_core::ChangeEventId::derive(&[b"unknown-event"]),
                    provenance: provenance.id,
                }],
                ..ChangeSetDelta::default()
            },
        },
        None,
    );
    if !matches!(rejected, Err(StoreError::InvalidDelta(_)))
        || reopened
            .latest_generation()
            .map(|manifest| manifest.generation)
            != Some(third)
    {
        return Err(StoreError::Integrity(
            "rejected membership publication partially changed the accepted graph".to_owned(),
        ));
    }
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    Ok(())
}

#[test]
fn failed_generation_consequence_write_rolls_back_graph_and_acceptance() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-{}-lineage-publication-rollback.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| StoreError::Backend(error.to_string()))?
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
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"publication-rollback-node"]),
        kind: syntaxmesh_core::NodeKind::Function,
        name: "must-not-publish".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };

    let mut store = open_migrated(&path)?;
    store
        .apply_delta(empty_delta(
            repository,
            worktree,
            None,
            first,
            b"publication-rollback-first-run",
        ))
        .map_err(|error| {
            StoreError::Backend(format!("publish rollback fixture anchor: {error}"))
        })?;
    store
        .connection
        .execute_batch(
            "CREATE TRIGGER fail_consequence_publication
                 BEFORE INSERT ON syntaxmesh_generation_consequences
                 BEGIN SELECT RAISE(ABORT, 'injected consequence publication failure'); END;",
        )
        .map_err(sql_error)?;

    let mut second_delta = empty_delta(
        repository,
        worktree,
        Some(first),
        second,
        b"publication-rollback-second-run",
    );
    second_delta.upsert_provenance.push(provenance.clone());
    second_delta.upsert_nodes.push(node.clone());
    let event = InMemoryGraphStore::new().change_event_for_delta(&second_delta)?;
    let fact_version = FactVersionRef {
        fact: FactRef::Provenance(provenance.id),
        valid_from: second,
    };
    let consequence = ConsequenceEdge {
        id: ConsequenceEdgeId::derive(&[b"publication-rollback-consequence"]),
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
    if !matches!(result, Err(StoreError::Backend(_)))
            || store
                .latest_generation()
                .map(|manifest| manifest.generation)
                != Some(first)
            || store
                .node(first, node.id)
                .map_err(|error| {
                    StoreError::Backend(format!("check in-memory rollback state: {error}"))
                })?
                .is_some()
            || store
                .generation_lineage_history()
                .map_err(|error| {
                    StoreError::Backend(format!("check in-memory lineage rollback: {error}"))
                })?
                .len()
                != 1
            || store.generation_consequence_history()?.len() != 1
            || store
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1)",
                    [consequence_id.0.0.as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
            || store
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM syntaxmesh_change_events WHERE generation = ?1)",
                    [second.0.0.as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
            || store
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_acceptance WHERE generation = ?1)",
                    [second.0.0.as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
        {
            return Err(StoreError::Integrity(
                "failed lineage write leaked part of the candidate generation".to_owned(),
            ));
        }
    drop(store);

    let reopened = SqliteGraphStore::open(&path)
        .map_err(|error| StoreError::Backend(format!("reopen rollback fixture: {error}")))?;
    let durable_ok = reopened
            .latest_generation()
            .map(|manifest| manifest.generation)
            == Some(first)
            && reopened.generation_lineage_history()?.len() == 1
            && reopened.generation_consequence_history()?.len() == 1
            && !reopened
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM syntaxmesh_change_events WHERE generation = ?1)",
                    [second.0.0.as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
            && !reopened
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_acceptance WHERE generation = ?1)",
                    [second.0.0.as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
            && reopened.node(first, node.id)?.is_none();
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if !durable_ok {
        return Err(StoreError::Integrity(
            "reopened SQLite database contains a partial failed generation".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn acceptance_time_is_atomic_and_survives_restart() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-acceptance.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"acceptance-repository"]);
    let worktree = WorktreeId::derive(&[b"acceptance-worktree"]);
    let mut store = open_migrated(&path)?;
    let mut expected_base = None;
    let mut last_generation = None;
    for (suffix, accepted_at) in [("max", u64::MAX), ("one", 1), ("two-fifty-six", 256)] {
        let generation = GenerationId::derive(&[suffix.as_bytes()]);
        let delta = GraphDelta {
            repository,
            worktree,
            run_id: syntaxmesh_core::IndexRunId::derive(&[suffix.as_bytes()]),
            expected_base,
            next_generation: generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        };
        store.apply_delta_at(delta, AcceptanceTime(accepted_at))?;
        if store.acceptance_time(generation)? != Some(AcceptanceTime(accepted_at)) {
            return Err(StoreError::Integrity(
                "SQLite acceptance time was not readable after publication".to_owned(),
            ));
        }
        if store.accepted_through(generation)? != Some(AcceptanceTime(u64::MAX)) {
            return Err(StoreError::Integrity(
                "SQLite acceptance prefix did not preserve the ancestry maximum".to_owned(),
            ));
        }
        expected_base = Some(generation);
        last_generation = Some(generation);
    }
    store.connection.execute_batch(
            "UPDATE syntaxmesh_generation_acceptance SET accepted_through_unix_nanos = NULL; DELETE FROM syntaxmesh_schema_migrations WHERE version > 14; UPDATE syntaxmesh_schema SET version = 14 WHERE id = 1;",
        ).map_err(sql_error)?;
    drop(store);
    let reopened = open_migrated(&path)?;
    let last_generation = last_generation.ok_or_else(|| {
        StoreError::Integrity("acceptance fixture did not publish generations".to_owned())
    })?;
    let accepted = reopened.acceptance_time(last_generation)?;
    let accepted_through = reopened.accepted_through(last_generation)?;
    let mut statement = reopened
            .connection
            .prepare("SELECT accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance ORDER BY accepted_at_unix_nanos")
            .map_err(sql_error)?;
    let ordered = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(sql_error)?
        .map(|row| {
            let bytes = row.map_err(sql_error)?;
            let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                StoreError::Integrity(format!("invalid acceptance time length {}", bytes.len()))
            })?;
            Ok(u64::from_be_bytes(bytes))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    drop(statement);
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if accepted != Some(AcceptanceTime(256))
        || accepted_through != Some(AcceptanceTime(u64::MAX))
        || ordered != [1, 256, u64::MAX]
    {
        return Err(StoreError::Integrity(
            "SQLite acceptance index ordering or restart persistence was incorrect".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn schema_v5_migration_converts_observation_time_to_index_order() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-observed-time-v5.db",
        std::process::id()
    ));
    let mut current = open_migrated(&path)?;
    let connection = &mut current.connection;
    connection
            .execute_batch("DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 5 WHERE id = 1;")
            .map_err(sql_error)?;
    for (id, time) in [(1_u8, 1_u64), (2_u8, 256_u64)] {
        connection
                .execute(
                    "INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, observed_at_unix_nanos, payload) VALUES (2, ?1, 1, ?2, x'00')",
                    params![[id], bincode::serialize(&time).map_err(|error| StoreError::Backend(error.to_string()))?],
                )
                .map_err(sql_error)?;
    }
    drop(current);
    let store = open_migrated(&path)?;
    let mut statement = store
            .connection
            .prepare("SELECT observed_at_unix_nanos FROM syntaxmesh_fact_versions ORDER BY observed_at_unix_nanos")
            .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(sql_error)?;
    let times = rows
        .map(|row| {
            let bytes = row.map_err(sql_error)?;
            let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                StoreError::Integrity(format!(
                    "migrated observation time has {} bytes",
                    bytes.len()
                ))
            })?;
            Ok(u64::from_be_bytes(bytes))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let schema_version = store
        .connection
        .query_row(
            "SELECT version FROM syntaxmesh_schema WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    drop(statement);
    drop(store);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if times != [1, 256] || schema_version != SCHEMA_VERSION {
        return Err(StoreError::Integrity(format!(
            "SQLite observation-time migration produced {times:?} at schema {schema_version}"
        )));
    }
    Ok(())
}

#[test]
fn schema_v7_migration_replaces_observation_index_with_cursor_order() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-observed-index-v7.db",
        std::process::id()
    ));
    let current = open_migrated(&path)?;
    current
            .connection
            .execute_batch("DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 7 WHERE id = 1; DROP INDEX syntaxmesh_fact_versions_observed_time_idx; CREATE INDEX syntaxmesh_fact_versions_observed_time_idx ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id) WHERE observed_at_unix_nanos IS NOT NULL; INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, observed_at_unix_nanos, payload) VALUES (2, x'01', 1, x'000000000000007b', x'00');")
            .map_err(sql_error)?;
    drop(current);

    let store = open_migrated(&path)?;
    let definition = store
            .connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'syntaxmesh_fact_versions_observed_time_idx'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?
            .to_ascii_lowercase();
    let version = store
        .connection
        .query_row(
            "SELECT version FROM syntaxmesh_schema WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    drop(store);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if version != SCHEMA_VERSION
        || !definition.contains("fact_kind, fact_id, valid_from_sequence")
        || !definition.contains("where observed_at_unix_nanos is not null")
    {
        return Err(StoreError::Integrity(format!(
            "SQLite v7 migration left schema {version} or an unordered observation index: {definition}"
        )));
    }
    Ok(())
}

#[test]
fn sqlite_integrity_check_reports_ok_for_initialized_store() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-integrity.db",
        std::process::id()
    ));
    let store = open_migrated(&path)?;
    let report = store.backend_integrity_check()?;
    let event_index_exists = store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'syntaxmesh_change_event_fact_lookup_idx')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
    if report.kind != BackendIntegrityKind::SqliteDatabase
        || !report.passed
        || report.findings.as_slice() != ["ok"]
        || !event_index_exists
    {
        return Err(StoreError::Integrity(format!(
            "SQLite integrity check unexpectedly failed: {report:?}"
        )));
    }
    drop(store);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    Ok(())
}

#[test]
fn sqlite_open_rejects_missing_migration_ledger_entry() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-migration-ledger.db",
        std::process::id()
    ));
    let store = open_migrated(&path)?;
    store
        .connection
        .execute(
            "DELETE FROM syntaxmesh_schema_migrations WHERE version = 8",
            [],
        )
        .map_err(sql_error)?;
    drop(store);

    let open_result = open_migrated(&path);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if !matches!(open_result, Err(StoreError::Integrity(_))) {
        return Err(StoreError::Integrity(
            "SQLite accepted an incomplete migration ledger".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn v19_upgrade_adds_fact_history_end_lookup_index() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-fact-history-end-index-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store
        .connection
        .execute_batch(
            "DROP INDEX syntaxmesh_fact_versions_identity_end_idx;
                 DELETE FROM syntaxmesh_schema_migrations WHERE version > 19;
                 UPDATE syntaxmesh_schema SET version = 19 WHERE id = 1;",
        )
        .map_err(sql_error)?;
    drop(store);

    SqliteGraphStore::migrate(&path)?;
    let migrated = open_migrated(&path)?;
    let index_exists = migrated
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'syntaxmesh_fact_versions_identity_end_idx')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
    let version = read_schema_version(&migrated.connection)?.ok_or_else(|| {
        StoreError::Integrity("SQLite migration removed schema version".to_owned())
    })?;
    drop(migrated);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if version != SCHEMA_VERSION || !index_exists {
        return Err(StoreError::Integrity(format!(
            "SQLite v19 upgrade stopped at schema {version} or omitted the fact-history end index"
        )));
    }
    Ok(())
}

#[test]
fn v20_upgrade_adds_generation_leading_fact_history_indexes() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-fact-history-generation-index-migration.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    store
        .connection
        .execute_batch(
            "DROP INDEX syntaxmesh_fact_versions_start_generation_idx;
                 DROP INDEX syntaxmesh_fact_versions_end_generation_idx;
                 DELETE FROM syntaxmesh_schema_migrations WHERE version > 20;
                 UPDATE syntaxmesh_schema SET version = 20 WHERE id = 1;",
        )
        .map_err(sql_error)?;
    drop(store);

    SqliteGraphStore::migrate(&path)?;
    let migrated = open_migrated(&path)?;
    let index_count = migrated
            .connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name IN ('syntaxmesh_fact_versions_start_generation_idx', 'syntaxmesh_fact_versions_end_generation_idx')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?;
    let version = read_schema_version(&migrated.connection)?.ok_or_else(|| {
        StoreError::Integrity("SQLite migration removed schema version".to_owned())
    })?;
    drop(migrated);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if version != SCHEMA_VERSION || index_count != 2 {
        return Err(StoreError::Integrity(format!(
            "SQLite v20 upgrade stopped at schema {version} or installed {index_count} temporal indexes"
        )));
    }
    Ok(())
}

#[test]
fn schema_v10_migration_adds_checksums_to_sql_history() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-checksums-v10.db",
        std::process::id()
    ));
    create_schema_v10_fixture(&path)?;
    let migrated = open_migrated(&path)?;
    let checksum_count = migrated
        .connection
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_schema_migrations WHERE checksum IS NOT NULL",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    let version = migrated
        .connection
        .query_row(
            "SELECT version FROM syntaxmesh_schema WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    drop(migrated);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if checksum_count != 12 || version != SCHEMA_VERSION {
        return Err(StoreError::Integrity(format!(
            "SQLite v10 migration produced {checksum_count} SQL checksums at schema {version}"
        )));
    }
    Ok(())
}

#[test]
fn migration_status_is_read_only_and_reports_bootstrap_and_current_schema() -> Result<(), StoreError>
{
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-migration-status.db",
        std::process::id()
    ));
    let file = std::fs::File::create(&path)
        .map_err(|error| StoreError::Backend(format!("create status fixture: {error}")))?;
    drop(file);

    let empty = SqliteGraphStore::migration_status(&path)?;
    let empty_size = std::fs::metadata(&path)
        .map_err(|error| StoreError::Backend(format!("stat status fixture: {error}")))?
        .len();
    let empty_is_pending = empty.current_version.is_none()
        && !empty.migration_ledger_validated
        && !empty.migrations.is_empty()
        && empty.migrations.iter().all(|migration| !migration.applied)
        && empty_size == 0;

    SqliteGraphStore::migrate(&path)?;
    let current = SqliteGraphStore::migration_status(&path)?;
    let bootstrapped = Connection::open(&path).map_err(sql_error)?;
    let acceptance_prefix_column = bootstrapped
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('syntaxmesh_generation_acceptance') WHERE name = 'accepted_through_unix_nanos')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
    drop(bootstrapped);
    let current_is_applied = current.current_version == Some(SCHEMA_VERSION)
        && current.target_version == SCHEMA_VERSION
        && current.migration_ledger_validated
        && acceptance_prefix_column
        && !current.migrations.is_empty()
        && current.migrations.iter().all(|migration| migration.applied);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove status fixture: {error}")))?;
    if !empty_is_pending || !current_is_applied {
        return Err(StoreError::Integrity(format!(
            "SQLite migration status mismatch: empty={empty:?}, current={current:?}, empty_size={empty_size}"
        )));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn sqlite_lifecycle_rejects_symlink_database_paths() -> Result<(), StoreError> {
    use std::os::unix::fs::symlink;

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| StoreError::Backend(format!("read test clock: {error}")))?
        .as_nanos();
    let root = std::env::temp_dir();
    let target = root.join(format!("syntaxmesh-sqlite-{nonce}-target.db"));
    let alias = root.join(format!("syntaxmesh-sqlite-{nonce}-alias.db"));
    std::fs::File::create(&target)
        .map_err(|error| StoreError::Backend(format!("create symlink target: {error}")))?;
    symlink(&target, &alias)
        .map_err(|error| StoreError::Backend(format!("create database symlink: {error}")))?;

    let status = SqliteGraphStore::migration_status(&alias);
    let migrate = SqliteGraphStore::migrate(&alias);
    let open = SqliteGraphStore::open(&alias);
    std::fs::remove_file(&alias)
        .map_err(|error| StoreError::Backend(format!("remove database symlink: {error}")))?;
    std::fs::remove_file(&target)
        .map_err(|error| StoreError::Backend(format!("remove symlink target: {error}")))?;

    if status.is_ok() || migrate.is_ok() || open.is_ok() {
        return Err(StoreError::Integrity(
            "SQLite lifecycle accepted a symlink database path".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn sqlite_connections_enable_defensive_mode_and_safe_pragmas() -> Result<(), StoreError> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| StoreError::Backend(format!("read test clock: {error}")))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("syntaxmesh-sqlite-{nonce}-defensive.db"));
    let store = open_migrated(&path)?;
    let defensive = store
        .connection
        .db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
        .map_err(sql_error)?;
    let trusted_schema = store
        .connection
        .pragma_query_value(None, "trusted_schema", |row| row.get::<_, i64>(0))
        .map_err(sql_error)?;
    let cell_size_check = store
        .connection
        .pragma_query_value(None, "cell_size_check", |row| row.get::<_, i64>(0))
        .map_err(sql_error)?;
    let foreign_keys = store
        .connection
        .pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))
        .map_err(sql_error)?;
    let synchronous = store
        .connection
        .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))
        .map_err(sql_error)?;
    let journal_mode = store
        .connection
        .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    drop(store);
    std::fs::remove_file(&path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;

    if !defensive
        || trusted_schema != 0
        || cell_size_check != 1
        || foreign_keys != 1
        || synchronous != 2
        || !journal_mode.eq_ignore_ascii_case("wal")
    {
        return Err(StoreError::Integrity(format!(
            "SQLite connection safety settings mismatch: defensive={defensive}, trusted_schema={trusted_schema}, cell_size_check={cell_size_check}, foreign_keys={foreign_keys}, synchronous={synchronous}, journal_mode={journal_mode}"
        )));
    }
    Ok(())
}

#[test]
fn accepted_delta_updates_only_touched_current_projection_rows() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-incremental-projection.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"projection-repository"]);
    let worktree = WorktreeId::derive(&[b"projection-worktree"]);
    let first = GenerationId::derive(&[b"projection-first"]);
    let second = GenerationId::derive(&[b"projection-second"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"projection-provenance"]),
        producer_namespace: "test.projection".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let mut retained = Node {
        id: NodeId::derive(&[b"projection-retained-node"]),
        kind: syntaxmesh_core::NodeKind::Function,
        name: "retained".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let untouched = Node {
        id: NodeId::derive(&[b"projection-untouched-node"]),
        name: "untouched".to_owned(),
        ..retained.clone()
    };
    let mut store = open_migrated(&path)?;
    let mut initial = empty_delta(repository, worktree, None, first, b"projection-first-run");
    initial.upsert_provenance.push(provenance);
    initial
        .upsert_nodes
        .extend([retained.clone(), untouched.clone()]);
    store.apply_delta(initial)?;
    store
            .connection
            .execute_batch(
                "CREATE TABLE syntaxmesh_projection_node_audit (operation TEXT NOT NULL, node_id BLOB NOT NULL);
                 CREATE TRIGGER syntaxmesh_projection_node_delete AFTER DELETE ON syntaxmesh_nodes BEGIN
                     INSERT INTO syntaxmesh_projection_node_audit VALUES ('delete', OLD.id);
                 END;
                 CREATE TRIGGER syntaxmesh_projection_node_update AFTER UPDATE ON syntaxmesh_nodes BEGIN
                     INSERT INTO syntaxmesh_projection_node_audit VALUES ('update', NEW.id);
                 END;",
            )
            .map_err(sql_error)?;
    retained.name = "updated".to_owned();
    let mut update = empty_delta(
        repository,
        worktree,
        Some(first),
        second,
        b"projection-second-run",
    );
    update.upsert_nodes.push(retained.clone());
    store.apply_delta(update)?;
    let deletes: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_projection_node_audit WHERE operation = 'delete'",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let (updates, updated_id): (i64, Vec<u8>) = store
            .connection
            .query_row(
                "SELECT COUNT(*), MIN(node_id) FROM syntaxmesh_projection_node_audit WHERE operation = 'update'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
    if deletes != 0
        || updates != 1
        || updated_id != retained.id.0.0
        || store.node(second, untouched.id)? != Some(untouched)
        || store.node(second, retained.id)? != Some(retained)
    {
        return Err(StoreError::Integrity(format!(
            "SQLite rewrote unaffected projection rows: deletes={deletes}, updates={updates}, updated_id={updated_id:?}"
        )));
    }
    drop(store);
    std::fs::remove_file(&path)
        .map_err(|error| StoreError::Backend(format!("remove projection fixture: {error}")))?;
    Ok(())
}

#[test]
fn sqlite_v17_upgrade_drops_temporal_endpoint_indexes() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-drop-temporal-endpoint-indexes.db",
        std::process::id()
    ));
    let store = open_migrated(&path)?;
    let fresh_index_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('syntaxmesh_fact_versions_source_idx', 'syntaxmesh_fact_versions_target_idx')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
    store
            .connection
            .execute_batch(
                "CREATE INDEX syntaxmesh_fact_versions_source_idx ON syntaxmesh_fact_versions(source_id, valid_from_sequence, valid_until_sequence);
                 CREATE INDEX syntaxmesh_fact_versions_target_idx ON syntaxmesh_fact_versions(target_id, valid_from_sequence, valid_until_sequence);
                 DELETE FROM syntaxmesh_schema_migrations WHERE version >= 18;
                 UPDATE syntaxmesh_schema SET version = 17 WHERE id = 1;",
            )
            .map_err(sql_error)?;
    drop(store);

    let failure = migration_test_sync::arm_failure(
        &path,
        18,
        migration_test_sync::CommitBoundary::BeforeCommit,
    );
    let failed_upgrade = SqliteGraphStore::migrate(&path);
    drop(failure);
    let rolled_back = Connection::open(&path).map_err(sql_error)?;
    let rolled_back_version = read_schema_version(&rolled_back)?.ok_or_else(|| {
        StoreError::Integrity("failed SQLite migration removed the schema version".to_owned())
    })?;
    let rolled_back_indexes: i64 = rolled_back
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('syntaxmesh_fact_versions_source_idx', 'syntaxmesh_fact_versions_target_idx')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
    drop(rolled_back);
    if failed_upgrade.is_ok() || rolled_back_version != 17 || rolled_back_indexes != 2 {
        return Err(StoreError::Integrity(format!(
            "failed SQLite endpoint-index migration was not atomic: result={failed_upgrade:?}, version={rolled_back_version}, indexes={rolled_back_indexes}"
        )));
    }

    SqliteGraphStore::migrate(&path)?;
    let migrated = SqliteGraphStore::open(&path)?;
    let version = read_schema_version(&migrated.connection)?.ok_or_else(|| {
        StoreError::Integrity("SQLite migration removed the schema version".to_owned())
    })?;
    let migrated_index_count: i64 = migrated
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('syntaxmesh_fact_versions_source_idx', 'syntaxmesh_fact_versions_target_idx')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
    drop(migrated);
    std::fs::remove_file(&path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if fresh_index_count != 0 || version != SCHEMA_VERSION || migrated_index_count != 0 {
        return Err(StoreError::Integrity(format!(
            "SQLite current schema retained temporal endpoint indexes: fresh={fresh_index_count}, migrated version={version}, migrated indexes={migrated_index_count}"
        )));
    }
    Ok(())
}

#[test]
fn sqlite_publishes_v2_generation_after_retained_v1_root() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-mixed-root-version.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let repository = RepositoryId::derive(&[b"mixed-root-repository"]);
    let worktree = WorktreeId::derive(&[b"mixed-root-worktree"]);
    let first_generation = GenerationId::derive(&[b"mixed-root-generation-one"]);
    let second_generation = GenerationId::derive(&[b"mixed-root-generation-two"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"mixed-root-provenance"]),
        producer_namespace: "mixed-root-test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let first_node = Node {
        id: NodeId::derive(&[b"mixed-root-node-one"]),
        kind: syntaxmesh_core::NodeKind::Function,
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
        next_generation: first_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![first_node],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let first_snapshot = GraphSnapshot {
        files: store.files(first_generation)?,
        provenance: store.provenance(first_generation)?,
        nodes: store.nodes(first_generation)?,
        edges: store.edges(first_generation)?,
    };
    let legacy_manifest = GenerationManifest {
        schema_version: 1,
        graph_root: schema_v1_root(
            first_generation,
            &first_snapshot.nodes,
            &first_snapshot.edges,
        )?,
        ..first_manifest
    };
    let mut history = store.generation_history()?;
    let history_entry = history.first_mut().ok_or_else(|| {
        StoreError::Integrity("first generation is missing its history entry".to_owned())
    })?;
    history_entry.manifest = legacy_manifest.clone();
    store
        .connection
        .execute(
            "UPDATE syntaxmesh_manifest SET payload = ?1 WHERE id = 1",
            [encode(&legacy_manifest)?],
        )
        .map_err(sql_error)?;
    store
        .connection
        .execute(
            "UPDATE syntaxmesh_generation_history SET payload = ?1 WHERE sequence = 1",
            [encode(history_entry)?],
        )
        .map_err(sql_error)?;
    store
        .connection
        .execute(
            "UPDATE syntaxmesh_graph_checkpoints SET manifest = ?1 WHERE sequence = 1",
            [encode(&legacy_manifest)?],
        )
        .map_err(sql_error)?;
    store
        .connection
        .execute_batch(
            "DELETE FROM syntaxmesh_temporal_roots; DELETE FROM syntaxmesh_temporal_tree_pages;",
        )
        .map_err(sql_error)?;
    let legacy_tree = snapshot_tree_mutations(&first_snapshot, 1)?;
    let transaction = store
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    publish_persistent_root(&transaction, 1, first_generation, None, &legacy_tree)?;
    transaction.commit().map_err(sql_error)?;
    drop(store);

    let mut reopened = SqliteGraphStore::open(&path)?;
    if reopened.manifest(first_generation)? != legacy_manifest
        || reopened.historical_snapshot(first_generation)? != first_snapshot
    {
        return Err(StoreError::Integrity(
            "SQLite did not reopen the retained schema-v1 generation".to_owned(),
        ));
    }
    let second_manifest = reopened.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"mixed-root-run-two"]),
        expected_base: Some(first_generation),
        next_generation: second_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: vec![second_node],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let second_snapshot = GraphSnapshot {
        files: reopened.files(second_generation)?,
        provenance: reopened.provenance(second_generation)?,
        nodes: reopened.nodes(second_generation)?,
        edges: reopened.edges(second_generation)?,
    };
    if second_manifest.schema_version != 2
        || reopened.historical_snapshot(first_generation)? != first_snapshot
        || reopened.historical_snapshot(second_generation)? != second_snapshot
        || reopened
            .generation_history()?
            .iter()
            .map(|entry| entry.manifest.schema_version)
            .collect::<Vec<_>>()
            != [1, 2]
        || syntaxmesh_store::graph_snapshot_root_v2(second_generation, &second_snapshot)?
            != second_manifest.graph_root
    {
        return Err(StoreError::Integrity(
            "SQLite did not preserve mixed v1/v2 generation roots".to_owned(),
        ));
    }
    drop(reopened);

    let restarted = SqliteGraphStore::open(&path)?;
    let restarted_history = restarted.generation_history()?;
    let old_after_restart = restarted.historical_snapshot(first_generation)?;
    let new_after_restart = restarted.historical_snapshot(second_generation)?;
    drop(restarted);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove mixed-root fixture: {error}")))?;
    if restarted_history
        .iter()
        .map(|entry| entry.manifest.schema_version)
        .collect::<Vec<_>>()
        != [1, 2]
        || old_after_restart != first_snapshot
        || new_after_restart != second_snapshot
    {
        return Err(StoreError::Integrity(
            "SQLite mixed v1/v2 roots did not survive a second restart".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn migration_commit_failpoints_prove_rollback_and_reconcile_commit() -> Result<(), StoreError> {
    let suffix = std::process::id();
    let before_path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{suffix}-migration-before-commit.db"
    ));
    create_schema_v10_fixture(&before_path)?;
    let before_guard = migration_test_sync::arm_failure(
        &before_path,
        11,
        migration_test_sync::CommitBoundary::BeforeCommit,
    );
    let before_result = open_migrated(&before_path);
    drop(before_guard);
    let before_connection = Connection::open(&before_path).map_err(sql_error)?;
    let before_version = read_schema_version(&before_connection)?.ok_or_else(|| {
        StoreError::Integrity("pre-commit failure removed the SQLite schema version".to_owned())
    })?;
    let checksum_column_exists = before_connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('syntaxmesh_schema_migrations') WHERE name = 'checksum')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
    drop(before_connection);
    std::fs::remove_file(&before_path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;

    let after_path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{suffix}-migration-after-commit.db"
    ));
    create_schema_v10_fixture(&after_path)?;
    let after_guard = migration_test_sync::arm_failure(
        &after_path,
        11,
        migration_test_sync::CommitBoundary::AfterCommit,
    );
    let after_result = open_migrated(&after_path);
    drop(after_guard);
    let after_store = after_result?;
    let after_version = read_schema_version(&after_store.connection)?.ok_or_else(|| {
        StoreError::Integrity("post-commit recovery removed the SQLite schema version".to_owned())
    })?;
    let checksum_count = after_store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM syntaxmesh_schema_migrations WHERE checksum IS NOT NULL",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    drop(after_store);
    std::fs::remove_file(&after_path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;

    if before_result.is_ok()
        || before_version != 10
        || checksum_column_exists
        || after_version != SCHEMA_VERSION
        || checksum_count != 12
    {
        return Err(StoreError::Integrity(format!(
            "SQLite migration interruption boundaries left pre-version {before_version}/column={checksum_column_exists}, post-version {after_version}/checksums={checksum_count}"
        )));
    }
    Ok(())
}

#[test]
fn sqlite_open_rejects_applied_sql_migration_checksum_mismatch() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-migration-checksum.db",
        std::process::id()
    ));
    let store = open_migrated(&path)?;
    let rust_only_without_checksum = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM syntaxmesh_schema_migrations WHERE version < 9 AND checksum IS NULL",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?;
    store
        .connection
        .execute(
            "UPDATE syntaxmesh_schema_migrations SET checksum = ?1 WHERE version = 9",
            ["0".repeat(64)],
        )
        .map_err(sql_error)?;
    drop(store);

    let open_result = open_migrated(&path);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if rust_only_without_checksum != 7 || !matches!(open_result, Err(StoreError::Integrity(_))) {
        return Err(StoreError::Integrity(
                "SQLite accepted a mismatched SQL migration checksum or assigned a checksum to Rust-only migrations"
                    .to_owned(),
            ));
    }
    Ok(())
}

#[test]
fn unsupported_schema_version_fails_closed() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-unknown-schema.db",
        std::process::id()
    ));
    let connection = Connection::open(&path).map_err(sql_error)?;
    connection
            .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 999);")
            .map_err(sql_error)?;
    drop(connection);
    let open_result = open_migrated(&path);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if !matches!(open_result, Err(StoreError::Backend(_))) {
        return Err(StoreError::Integrity(
            "SQLite opened an unsupported schema version".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn migration_registry_is_contiguous_and_named() -> Result<(), StoreError> {
    for (offset, migration) in SCHEMA_MIGRATIONS.iter().enumerate() {
        let expected =
            i64::try_from(offset + 1).map_err(|error| StoreError::Integrity(error.to_string()))?;
        if migration.from != expected
            || expected.checked_add(1) != Some(migration.to)
            || migration.name.is_empty()
            || (migration.rollback_sql.is_some() && migration.checksum_sql.is_none())
            || migration
                .fresh_bootstrap_sql
                .is_some_and(|bootstrap| migration.checksum_sql != Some(bootstrap))
        {
            return Err(StoreError::Integrity(format!(
                "SQLite migration registry is not contiguous at {migration:?}"
            )));
        }
    }
    if SCHEMA_MIGRATIONS.last().map(|migration| migration.to) != Some(SCHEMA_VERSION) {
        return Err(StoreError::Integrity(
            "SQLite migration registry does not reach current schema".to_owned(),
        ));
    }
    if SCHEMA_MIGRATIONS
        .iter()
        .filter(|migration| migration.rollback_sql.is_some())
        .count()
        != 6
    {
        return Err(StoreError::Integrity(
            "expected reversible consequence and acceptance-prefix migrations".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn registered_sql_migrations_match_bundled_files() -> Result<(), StoreError> {
    let migration_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let bootstrap_path = migration_dir.join("0000_initial_schema.sql");
    let bootstrap = std::fs::read_to_string(&bootstrap_path).map_err(|error| {
        StoreError::Backend(format!(
            "read SQLite bootstrap schema {bootstrap_path:?}: {error}"
        ))
    })?;
    if bootstrap != SCHEMA {
        return Err(StoreError::Integrity(
            "embedded SQLite bootstrap schema differs from its bundled file".to_owned(),
        ));
    }

    let mut disk_up = BTreeSet::new();
    let mut disk_down = BTreeSet::new();
    for entry in std::fs::read_dir(&migration_dir)
        .map_err(|error| StoreError::Backend(format!("read SQLite migration directory: {error}")))?
    {
        let entry = entry.map_err(|error| {
            StoreError::Backend(format!("read SQLite migration directory entry: {error}"))
        })?;
        let name = entry.file_name().into_string().map_err(|error| {
            StoreError::Integrity(format!("SQLite migration filename is not UTF-8: {error:?}"))
        })?;
        if name.ends_with(".up.sql") && name != "0000_initial_schema.sql" {
            disk_up.insert(name);
        } else if name.ends_with(".down.sql") {
            disk_down.insert(name);
        }
    }

    let mut registered_up = BTreeSet::new();
    let mut registered_down = BTreeSet::new();
    for migration in SCHEMA_MIGRATIONS {
        let up_filename = format!("{:04}_{}.up.sql", migration.to, migration.name);
        if let Some(expected_sql) = migration.checksum_sql {
            let path = migration_dir.join(&up_filename);
            let actual_sql = std::fs::read_to_string(&path).map_err(|error| {
                StoreError::Backend(format!(
                    "read registered SQLite migration {path:?}: {error}"
                ))
            })?;
            if actual_sql != expected_sql {
                return Err(StoreError::Integrity(format!(
                    "registered SQLite migration {} differs from bundled SQL",
                    up_filename
                )));
            }
            registered_up.insert(up_filename.clone());
            if migration
                .fresh_bootstrap_sql
                .is_some_and(|sql| sql != expected_sql)
            {
                return Err(StoreError::Integrity(format!(
                    "fresh SQLite bootstrap SQL for {} differs from its registered migration",
                    migration.name
                )));
            }
        }
        if let Some(expected_sql) = migration.rollback_sql {
            let down_filename = up_filename.replace(".up.sql", ".down.sql");
            let path = migration_dir.join(&down_filename);
            let actual_sql = std::fs::read_to_string(&path).map_err(|error| {
                StoreError::Backend(format!("read registered SQLite rollback {path:?}: {error}"))
            })?;
            if actual_sql != expected_sql {
                return Err(StoreError::Integrity(format!(
                    "registered SQLite rollback {} differs from bundled SQL",
                    down_filename
                )));
            }
            registered_down.insert(down_filename);
        }
    }
    if registered_up != disk_up {
        return Err(StoreError::Integrity(format!(
            "SQLite up migrations differ from registry: unregistered files {:?}, missing files {:?}",
            disk_up.difference(&registered_up).collect::<Vec<_>>(),
            registered_up.difference(&disk_up).collect::<Vec<_>>()
        )));
    }
    if registered_down != disk_down {
        return Err(StoreError::Integrity(format!(
            "SQLite down migrations differ from registry: unregistered files {:?}, missing files {:?}",
            disk_down.difference(&registered_down).collect::<Vec<_>>(),
            registered_down.difference(&disk_down).collect::<Vec<_>>()
        )));
    }
    Ok(())
}

#[test]
fn node_kind_migration_adds_projection_and_backfills_payloads() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-node-kind-migration.db",
        std::process::id()
    ));
    let store = open_migrated(&path)?;
    let node = Node {
        id: NodeId::derive(&[b"node-kind-migration-module"]),
        kind: syntaxmesh_core::NodeKind::Module,
        name: "src/lib.rs".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"node-kind-migration-provenance"]),
        extension_payload: None,
    };
    store
        .connection
        .execute(
            "INSERT INTO syntaxmesh_nodes (id, payload, node_kind) VALUES (?1, ?2, ?3)",
            params![
                node.id.0.0.as_slice(),
                encode(&node)?,
                node_kind_storage_code(&node.kind),
            ],
        )
        .map_err(sql_error)?;
    store
        .connection
        .execute_batch(
            "DROP INDEX syntaxmesh_nodes_kind_idx;
                 ALTER TABLE syntaxmesh_nodes DROP COLUMN node_kind;
                 DELETE FROM syntaxmesh_schema_migrations WHERE version >= 16;
                 UPDATE syntaxmesh_schema SET version = 15 WHERE id = 1;",
        )
        .map_err(sql_error)?;
    drop(store);

    SqliteGraphStore::migrate(&path)?;
    let connection = Connection::open(&path).map_err(sql_error)?;
    let (version, code, index_exists) = connection
            .query_row(
                "SELECT s.version, n.node_kind, EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'syntaxmesh_nodes_kind_idx') FROM syntaxmesh_schema s JOIN syntaxmesh_nodes n ON n.id = ?1 WHERE s.id = 1",
                [node.id.0.0.as_slice()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, bool>(2)?)),
            )
            .map_err(sql_error)?;
    drop(connection);
    std::fs::remove_file(path).map_err(|error| {
        StoreError::Backend(format!("remove node-kind migration fixture: {error}"))
    })?;
    if version != SCHEMA_VERSION || code != node_kind_storage_code(&node.kind) || !index_exists {
        return Err(StoreError::Integrity(format!(
            "SQLite node-kind migration produced version {version}, code {code}, index={index_exists}"
        )));
    }
    Ok(())
}

#[test]
fn consequence_migration_round_trips_up_and_down() -> Result<(), StoreError> {
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.name == "evidence_consequence_edges")
        .ok_or_else(|| StoreError::Integrity("missing consequence migration".to_owned()))?;
    let up = migration.checksum_sql.ok_or_else(|| {
        StoreError::Integrity("consequence migration has no checksummed up SQL".to_owned())
    })?;
    let down = migration.rollback_sql.ok_or_else(|| {
        StoreError::Integrity("consequence migration has no rollback SQL".to_owned())
    })?;
    let connection = Connection::open_in_memory().map_err(sql_error)?;
    connection.execute_batch(up).map_err(sql_error)?;
    let created_tables = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 'syntaxmesh_consequence_%'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?;
    if created_tables != 3 {
        return Err(StoreError::Integrity(format!(
            "consequence migration created {created_tables} of 3 expected tables"
        )));
    }
    connection.execute_batch(down).map_err(sql_error)?;
    let remaining_tables = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND (name LIKE 'syntaxmesh_consequence_%' OR name = 'syntaxmesh_generation_consequences')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?;
    if remaining_tables != 0 {
        return Err(StoreError::Integrity(
            "consequence rollback left newly introduced tables behind".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn acceptance_prefix_migration_round_trips_up_and_down() -> Result<(), StoreError> {
    let migration = SCHEMA_MIGRATIONS
        .iter()
        .find(|migration| migration.name == "generation_acceptance_prefix")
        .ok_or_else(|| StoreError::Integrity("missing acceptance-prefix migration".to_owned()))?;
    let connection = Connection::open_in_memory().map_err(sql_error)?;
    connection.execute_batch(
            "CREATE TABLE syntaxmesh_generation_acceptance (generation BLOB PRIMARY KEY NOT NULL, accepted_at_unix_nanos BLOB NOT NULL);",
        ).map_err(sql_error)?;
    connection
        .execute_batch(migration.checksum_sql.ok_or_else(|| {
            StoreError::Integrity("acceptance-prefix migration has no checksummed SQL".to_owned())
        })?)
        .map_err(sql_error)?;
    let present = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('syntaxmesh_generation_acceptance') WHERE name = 'accepted_through_unix_nanos')",
            [], |row| row.get::<_, bool>(0),
        ).map_err(sql_error)?;
    connection
        .execute_batch(migration.rollback_sql.ok_or_else(|| {
            StoreError::Integrity("acceptance-prefix migration has no rollback SQL".to_owned())
        })?)
        .map_err(sql_error)?;
    let absent = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('syntaxmesh_generation_acceptance') WHERE name = 'accepted_through_unix_nanos')",
            [], |row| row.get::<_, bool>(0),
        ).map_err(sql_error)?;
    if !present || absent {
        return Err(StoreError::Integrity(
            "acceptance-prefix migration did not round-trip its added column".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn consequence_range_query_uses_both_endpoint_indexes() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-consequence-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let details = {
        let mut statement = store
            .connection
            .prepare(&format!("EXPLAIN QUERY PLAN {CONSEQUENCE_RANGE_QUERY}"))
            .map_err(sql_error)?;
        statement
            .query_map(
                params![
                    10_i64,
                    2_i64,
                    "change_event",
                    vec![1_u8; 32],
                    None::<i64>,
                    None::<Vec<u8>>,
                    "change_event",
                    vec![1_u8; 32],
                    None::<i64>,
                    None::<Vec<u8>>,
                    None::<Vec<u8>>,
                    11_i64,
                ],
                |row| row.get::<_, String>(3),
            )
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?
    };
    drop(store);
    drop(std::fs::remove_file(&path));

    let plan = details.join("; ");
    if !plan.contains("syntaxmesh_consequence_source_idx")
        || !plan.contains("syntaxmesh_consequence_target_idx")
    {
        return Err(StoreError::Integrity(format!(
            "consequence range query does not use both endpoint indexes: {plan}"
        )));
    }
    Ok(())
}

#[test]
fn fact_history_page_uses_composite_history_index() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-fact-history-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let details = {
        let mut statement = store
            .connection
            .prepare(&format!(
                "EXPLAIN QUERY PLAN {FACT_HISTORY_AFTER_PAGE_QUERY}"
            ))
            .map_err(sql_error)?;
        statement
            .query_map(
                params![vec![1_u8; 32], FILE_FACT, vec![1_u8; 32], 1_i64, 3_i64],
                |row| row.get::<_, String>(3),
            )
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?
    };
    drop(store);
    drop(std::fs::remove_file(&path));
    let plan = details.join("; ");
    if !plan.contains("syntaxmesh_fact_versions_identity_time_idx")
        && !plan.contains("sqlite_autoindex_syntaxmesh_fact_versions_1")
    {
        return Err(StoreError::Integrity(format!(
            "fact-history cursor query does not use the composite identity/sequence index: {plan}"
        )));
    }
    Ok(())
}

#[test]
fn generation_change_page_seeks_sequence_primary_key() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-generation-change-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let details = {
        let mut statement = store
                .connection
                .prepare("EXPLAIN QUERY PLAN SELECT sequence, payload FROM syntaxmesh_generation_history WHERE sequence > ?1 AND sequence <= ?2 ORDER BY sequence LIMIT ?3")
                .map_err(sql_error)?;
        statement
            .query_map(params![1_i64, 4_i64, 2_i64], |row| row.get::<_, String>(3))
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?
    };
    drop(store);
    drop(std::fs::remove_file(&path));
    let plan = details.join("; ");
    if !plan.contains("SEARCH syntaxmesh_generation_history USING INTEGER PRIMARY KEY") {
        return Err(StoreError::Integrity(format!(
            "generation change page does not seek its sequence primary key: {plan}"
        )));
    }
    Ok(())
}

#[test]
fn fact_version_generation_change_uses_start_and_end_indexes() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-fact-version-change-query-plan.db",
        std::process::id()
    ));
    drop(std::fs::remove_file(&path));
    let store = open_migrated(&path)?;
    let details = {
        let mut statement = store
                .connection
                .prepare("EXPLAIN QUERY PLAN SELECT valid_from_sequence FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence = ?3 UNION ALL SELECT valid_from_sequence FROM syntaxmesh_fact_versions INDEXED BY syntaxmesh_fact_versions_identity_end_idx WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_until_sequence = ?3 AND valid_from_sequence <> ?3 ORDER BY valid_from_sequence")
                .map_err(sql_error)?;
        statement
            .query_map(params![NODE_FACT, vec![1_u8; 32], 2_i64], |row| {
                row.get::<_, String>(3)
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?
    };
    let generation_plan = {
        let mut statement = store
            .connection
            .prepare(&format!(
                "EXPLAIN QUERY PLAN {FACT_VERSION_CHANGE_PAGE_QUERY}"
            ))
            .map_err(sql_error)?;
        statement
            .query_map(
                params![2_i64, -1_i64, Vec::<u8>::new(), 0_i64, 101_i64],
                |row| row.get::<_, String>(3),
            )
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?
    };
    drop(store);
    drop(std::fs::remove_file(&path));
    let plan = details.join("; ");
    if (!plan.contains("syntaxmesh_fact_versions_identity_time_idx")
        && !plan.contains("sqlite_autoindex_syntaxmesh_fact_versions_1"))
        || !plan.contains("syntaxmesh_fact_versions_identity_end_idx")
        || !generation_plan
            .join("; ")
            .contains("syntaxmesh_fact_versions_start_generation_idx")
        || !generation_plan
            .join("; ")
            .contains("syntaxmesh_fact_versions_end_generation_idx")
    {
        return Err(StoreError::Integrity(format!(
            "fact-version generation change does not use identity indexes and generation-page indexes: {plan}; {}",
            generation_plan.join("; ")
        )));
    }
    Ok(())
}

#[test]
fn sqlite_publishes_consequence_edges_and_evidence_atomically_across_restart()
-> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-consequence-publication.db",
        std::process::id()
    ));
    let mut store = open_migrated(&path)?;
    let repository = RepositoryId::derive(&[b"consequence-repository"]);
    let worktree = WorktreeId::derive(&[b"consequence-worktree"]);
    let generation = GenerationId::derive(&[b"consequence-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"consequence-provenance"]),
        producer_namespace: "test.consequences".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let mut graph = empty_delta(repository, worktree, None, generation, b"consequence-run");
    graph.upsert_provenance.push(provenance.clone());
    let event = InMemoryGraphStore::new().change_event_for_delta(&graph)?;
    let fact_version = FactVersionRef {
        fact: FactRef::Provenance(provenance.id),
        valid_from: generation,
    };
    let edge = ConsequenceEdge {
        id: ConsequenceEdgeId::derive(&[b"sqlite-consequence"]),
        source: LineageEndpoint::ChangeEvent(event.id),
        target: LineageEndpoint::FactVersion(fact_version),
        kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
        evidence: vec![fact_version],
        derivation: ConsequenceDerivation::Explicit,
        provenance: provenance.id,
    };
    let mut second_edge = edge.clone();
    second_edge.id = ConsequenceEdgeId::derive(&[b"sqlite-consequence-second"]);
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
        Some(AcceptanceTime(81)),
    )?;
    let first_page = store.consequence_edges_for_endpoint(
        LineageEndpoint::ChangeEvent(event.id),
        generation,
        None,
        1,
    )?;
    let cursor = first_page.next_cursor.ok_or_else(|| {
        StoreError::Integrity("consequence query omitted continuation cursor".to_owned())
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
        return Err(StoreError::Integrity(
            "SQLite indexed consequence pagination was inconsistent".to_owned(),
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
        StoreError::Integrity("consequence range page omitted its continuation cursor".to_owned())
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
        return Err(StoreError::Integrity(
            "SQLite consequence validity-range page or cursor was inconsistent".to_owned(),
        ));
    }
    let fact_page = store.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        generation,
        None,
        10,
    )?;
    if fact_page.items.len() != 2 {
        return Err(StoreError::Integrity(
            "SQLite exact fact-version endpoint index missed consequence edges".to_owned(),
        ));
    }
    let failed_retraction_generation =
        GenerationId::derive(&[b"sqlite-consequence-failed-retraction"]);
    store
        .connection
        .execute_batch(
            "CREATE TRIGGER fail_consequence_retraction
                 BEFORE INSERT ON syntaxmesh_generation_consequences
                 BEGIN SELECT RAISE(ABORT, 'injected consequence retraction failure'); END;",
        )
        .map_err(sql_error)?;
    let failed_retraction = store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: empty_delta(
                    repository,
                    worktree,
                    Some(generation),
                    failed_retraction_generation,
                    b"failed-consequence-retraction-run",
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
        Some(AcceptanceTime(82)),
    );
    store
        .connection
        .execute_batch("DROP TRIGGER fail_consequence_retraction")
        .map_err(sql_error)?;
    let after_failed_retraction = store.consequence_edges_for_endpoint(
        LineageEndpoint::FactVersion(fact_version),
        generation,
        None,
        10,
    )?;
    let failed_generation_exists = store
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_consequences WHERE generation = ?1)",
            [failed_retraction_generation.0.0.as_slice()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
    let failed_acceptance_exists = store
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM syntaxmesh_generation_acceptance WHERE generation = ?1)",
            [failed_retraction_generation.0.0.as_slice()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(sql_error)?;
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
        return Err(StoreError::Integrity(
            "failed SQLite consequence retraction leaked partial publication state".to_owned(),
        ));
    }
    let retracted_generation = GenerationId::derive(&[b"sqlite-consequence-retraction"]);
    store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: empty_delta(
                    repository,
                    worktree,
                    Some(generation),
                    retracted_generation,
                    b"consequence-retraction-run",
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
        Some(AcceptanceTime(82)),
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
        return Err(StoreError::Integrity(
            "SQLite consequence retraction changed historical endpoint results".to_owned(),
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
        return Err(StoreError::Integrity(
            "SQLite consequence range did not preserve the exclusive retraction boundary"
                .to_owned(),
        ));
    }
    let row_counts = (
        store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM syntaxmesh_generation_consequences WHERE generation = ?1",
                [generation.0.0.as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?,
        store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM syntaxmesh_consequence_edge_versions WHERE edge_id = ?1",
                [edge.id.0.0.as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?,
        store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM syntaxmesh_consequence_evidence WHERE edge_id = ?1",
                [edge.id.0.0.as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?,
    );
    drop(store);
    let reopened = SqliteGraphStore::open(&path)?;
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
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove consequence fixture: {error}")))?;
    if row_counts != (1, 1, 1)
        || reopened_page.items.len() != 2
        || reopened_current_page.items.len() != 1
        || history.len() != 2
        || history.first().and_then(|entry| entry.delta.add.first()) != Some(&edge)
    {
        return Err(StoreError::Integrity(format!(
            "SQLite consequence publication was not atomic/restartable: rows={row_counts:?}, journal={history:?}"
        )));
    }
    Ok(())
}

#[test]
fn schema_v13_upgrade_applies_consequence_schema_and_ledger_atomically() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-consequences-v13.db",
        std::process::id()
    ));
    let current = open_migrated(&path)?;
    current
        .connection
        .execute_batch(include_str!(
            "../../migrations/0014_evidence_consequence_edges.down.sql"
        ))
        .map_err(sql_error)?;
    current
            .connection
            .execute_batch(
                "DELETE FROM syntaxmesh_schema_migrations WHERE version >= 14; UPDATE syntaxmesh_schema SET version = 13 WHERE id = 1;",
            )
            .map_err(sql_error)?;
    drop(current);

    let upgraded = open_migrated(&path)?;
    let version = read_schema_version(&upgraded.connection)?.ok_or_else(|| {
        StoreError::Integrity("SQLite v13 upgrade removed schema version".to_owned())
    })?;
    let indexed_tables = upgraded
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('syntaxmesh_generation_consequences', 'syntaxmesh_consequence_edge_versions', 'syntaxmesh_consequence_evidence', 'syntaxmesh_consequence_parents')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?;
    let migration_rows = upgraded
            .connection
            .query_row(
                "SELECT COUNT(*) FROM syntaxmesh_schema_migrations WHERE version = 14 AND name = 'evidence_consequence_edges' AND checksum IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)?;
    drop(upgraded);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if version != SCHEMA_VERSION || indexed_tables != 4 || migration_rows != 1 {
        return Err(StoreError::Integrity(format!(
            "SQLite v13 upgrade produced version {version}, {indexed_tables} consequence tables and {migration_rows} ledger rows"
        )));
    }
    Ok(())
}

#[test]
fn open_fails_closed_without_migrating_a_stale_database() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-explicit-migration.db",
        std::process::id()
    ));
    let missing_open = SqliteGraphStore::open(&path);
    if missing_open.is_ok() || path.exists() {
        return Err(StoreError::Integrity(
            "SQLite open created a missing database instead of requiring migration".to_owned(),
        ));
    }
    SqliteGraphStore::migrate(&path)?;
    let connection = Connection::open(&path).map_err(sql_error)?;
    connection
        .execute_batch(
            "DELETE FROM syntaxmesh_schema_migrations WHERE version = 12;
                 UPDATE syntaxmesh_schema SET version = 11 WHERE id = 1;",
        )
        .map_err(sql_error)?;
    drop(connection);

    let open_result = SqliteGraphStore::open(&path);
    let verify_connection = Connection::open(&path).map_err(sql_error)?;
    let version = read_schema_version(&verify_connection)?
        .ok_or_else(|| StoreError::Integrity("open removed the stale SQLite version".to_owned()))?;
    drop(verify_connection);
    std::fs::remove_file(&path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;

    if open_result.is_ok() || version != 11 {
        return Err(StoreError::Integrity(format!(
            "open migrated stale schema instead of failing closed (version={version})"
        )));
    }
    Ok(())
}

#[test]
fn open_does_not_rebuild_current_schema_derived_projections() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-no-startup-repair.db",
        std::process::id()
    ));
    let mut store = open_migrated(&path)?;
    store.apply_delta(empty_delta(
        RepositoryId::derive(&[b"startup-repair-repository"]),
        WorktreeId::derive(&[b"startup-repair-worktree"]),
        None,
        GenerationId::derive(&[b"startup-repair-generation"]),
        b"startup-repair-run",
    ))?;
    store
        .connection
        .execute_batch(
            "DELETE FROM syntaxmesh_change_event_facts; DELETE FROM syntaxmesh_change_events;",
        )
        .map_err(sql_error)?;
    drop(store);

    let open_result = SqliteGraphStore::open(&path);
    let connection = Connection::open(&path).map_err(sql_error)?;
    let event_count = connection
        .query_row("SELECT COUNT(*) FROM syntaxmesh_change_events", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(sql_error)?;
    drop(connection);
    std::fs::remove_file(&path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;

    if open_result.is_ok() || event_count != 0 {
        return Err(StoreError::Integrity(
            "SQLite open accepted or repaired a missing derived projection".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn failed_migration_keeps_version_and_rows_unchanged() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-migration-rollback.db",
        std::process::id()
    ));
    let setup_connection = Connection::open(&path).map_err(sql_error)?;
    setup_connection
            .execute_batch("CREATE TABLE syntaxmesh_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO syntaxmesh_schema (id, version) VALUES (1, 5); CREATE TABLE syntaxmesh_fact_versions (fact_kind INTEGER NOT NULL, fact_id BLOB NOT NULL, valid_from_sequence INTEGER NOT NULL, valid_until_sequence INTEGER, observed_at_unix_nanos BLOB, source_id BLOB, target_id BLOB, payload BLOB NOT NULL, PRIMARY KEY (fact_kind, fact_id, valid_from_sequence)); INSERT INTO syntaxmesh_fact_versions (fact_kind, fact_id, valid_from_sequence, observed_at_unix_nanos, payload) VALUES (2, x'01', 1, x'ff', x'00');")
            .map_err(sql_error)?;
    drop(setup_connection);

    let result = open_migrated(&path);
    let verify_connection = Connection::open(&path).map_err(sql_error)?;
    let version = verify_connection
        .query_row(
            "SELECT version FROM syntaxmesh_schema WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    let encoded_time = verify_connection
        .query_row(
            "SELECT observed_at_unix_nanos FROM syntaxmesh_fact_versions WHERE fact_id = x'01'",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(sql_error)?;
    let index_exists = verify_connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'syntaxmesh_fact_versions_observed_time_idx')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
    drop(verify_connection);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if result.is_ok() || version != 5 || encoded_time != [0xff] || index_exists {
        return Err(StoreError::Integrity(format!(
            "failed migration changed schema state: result_ok={}, version={version}, payload={encoded_time:?}, index={index_exists}",
            result.is_ok()
        )));
    }
    Ok(())
}

#[test]
fn database_without_schema_version_is_not_initialized_over_existing_tables()
-> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-missing-schema-version.db",
        std::process::id()
    ));
    let setup_connection = Connection::open(&path).map_err(sql_error)?;
    setup_connection
            .execute_batch("CREATE TABLE unrelated_data (value TEXT NOT NULL); INSERT INTO unrelated_data VALUES ('preserve');")
            .map_err(sql_error)?;
    drop(setup_connection);
    let result = open_migrated(&path);
    let verify_connection = Connection::open(&path).map_err(sql_error)?;
    let app_table_count = verify_connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;
    let preserved = verify_connection
        .query_row("SELECT value FROM unrelated_data", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(sql_error)?;
    drop(verify_connection);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if result.is_ok() || app_table_count != 1 || preserved != "preserve" {
        return Err(StoreError::Integrity(
            "unversioned nonempty database was modified instead of rejected".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn durable_records_use_cas_and_survive_restart() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-records.db",
        std::process::id()
    ));
    let mut writer = open_migrated(&path)?;
    let mut stale_writer = open_migrated(&path)?;
    let key = "syntaxmesh.test.operation/first";
    writer.compare_exchange_record(key, None, b"prepared")?;
    writer.compare_exchange_record("syntaxmesh.test.operation-extra", None, b"outside")?;
    writer.compare_exchange_record("unrelated", None, b"outside")?;
    if stale_writer
        .compare_exchange_record(key, None, b"incorrect")
        .is_ok()
    {
        return Err(StoreError::Integrity(
            "SQLite accepted a stale durable-record write".to_owned(),
        ));
    }
    drop(stale_writer);
    drop(writer);
    let reopened = open_migrated(&path)?;
    let value = reopened.read_record(key)?;
    let records = reopened.records_with_prefix("syntaxmesh.test.operation/")?;
    let latest = reopened.last_record_with_prefix("syntaxmesh.test.operation/")?;
    let exact = reopened.last_record_with_prefix(key)?;
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if value.as_deref() != Some(b"prepared")
        || records != vec![(key.to_owned(), b"prepared".to_vec())]
        || latest != Some((key.to_owned(), b"prepared".to_vec()))
        || exact != Some((key.to_owned(), b"prepared".to_vec()))
    {
        return Err(StoreError::Integrity(
            "SQLite durable records did not survive restart".to_owned(),
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

#[test]
fn schema_v4_migration_rebuilds_shared_roots_for_retained_generations() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-persistent-roots.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"persistent-roots-repository"]);
    let worktree = WorktreeId::derive(&[b"persistent-roots-worktree"]);
    let first = GenerationId::derive(&[b"persistent-roots-first"]);
    let second = GenerationId::derive(&[b"persistent-roots-second"]);
    let provenance = Provenance {
        id: syntaxmesh_core::ProvenanceId::derive(&[b"persistent-roots-provenance"]),
        producer_namespace: "test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let node_id = NodeId::derive(&[b"persistent-roots-node"]);
    let first_node = Node {
        id: node_id,
        kind: syntaxmesh_core::NodeKind::Function,
        name: "first".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let second_node = Node {
        name: "second".to_owned(),
        ..first_node.clone()
    };
    let mut store = open_migrated(&path)?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"persistent-roots-first-run"]),
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
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"persistent-roots-second-run"]),
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
    let first_state = store.historical_snapshot(first)?;
    let second_state = store.historical_snapshot(second)?;
    store
            .connection
            .execute_batch(
                "DELETE FROM syntaxmesh_temporal_roots; DELETE FROM syntaxmesh_temporal_tree_pages; DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 4 WHERE id = 1;",
            )
            .map_err(sql_error)?;
    drop(store);

    let reopened = open_migrated(&path)?;
    let rebuilt_first = reopened.historical_snapshot(first)?;
    let rebuilt_second = reopened.historical_snapshot(second)?;
    reopened
        .connection
        .execute(
            "UPDATE syntaxmesh_graph_checkpoints SET payload = x'00' WHERE sequence = 1",
            [],
        )
        .map_err(sql_error)?;
    let checkpoint_independent_first = reopened.historical_snapshot(first)?;
    let integrity = reopened.backend_integrity_check()?;
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if first_state != rebuilt_first
        || second_state != rebuilt_second
        || checkpoint_independent_first != rebuilt_first
        || integrity.passed
        || !integrity
            .findings
            .iter()
            .any(|finding| finding.contains("checkpoint sequence 1"))
        || rebuilt_first
            .nodes
            .first()
            .is_none_or(|node| node.name != "first")
        || rebuilt_second
            .nodes
            .first()
            .is_none_or(|node| node.name != "second")
    {
        return Err(StoreError::Integrity(
            "schema migration failed to rebuild historical persistent roots".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn persistent_root_corruption_fails_historical_reads_and_integrity_check() -> Result<(), StoreError>
{
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-persistent-root-corruption.db",
        std::process::id()
    ));
    let generation = GenerationId::derive(&[b"root-corruption-generation"]);
    let mut store = open_migrated(&path)?;
    let mut delta = empty_delta(
        RepositoryId::derive(&[b"root-corruption-repository"]),
        WorktreeId::derive(&[b"root-corruption-worktree"]),
        None,
        generation,
        b"root-corruption-run",
    );
    let provenance_id = syntaxmesh_core::ProvenanceId::derive(&[b"root-corruption-provenance"]);
    let node_id = NodeId::derive(&[b"root-corruption-node"]);
    delta.upsert_provenance.push(Provenance {
        id: provenance_id,
        producer_namespace: "test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    });
    delta.upsert_nodes.push(Node {
        id: node_id,
        kind: syntaxmesh_core::NodeKind::Function,
        name: "root-corruption".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    });
    store.apply_delta(delta)?;
    store
            .connection
            .execute("UPDATE syntaxmesh_temporal_tree_pages SET payload = x'00' WHERE page_id = (SELECT root_id FROM syntaxmesh_temporal_roots WHERE generation = ?1)", params![generation.0.0.as_slice()])
            .map_err(sql_error)?;

    let historical_read_failed = store.historical_snapshot(generation).is_err();
    let integrity = store.backend_integrity_check()?;
    drop(store);
    std::fs::remove_file(&path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if !historical_read_failed
        || integrity.passed
        || !integrity
            .findings
            .iter()
            .any(|finding| finding.contains("persistent") || finding.contains("tree"))
    {
        return Err(StoreError::Integrity(
            "persistent-root corruption was not rejected by reads and integrity validation"
                .to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn legacy_manifest_becomes_replayable_history_anchor() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-history-anchor.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"history-anchor-repo"]);
    let worktree = WorktreeId::derive(&[b"history-anchor-worktree"]);
    let first = GenerationId::derive(&[b"history-anchor-first"]);
    let next = GenerationId::derive(&[b"history-anchor-next"]);
    let mut writer = open_migrated(&path)?;
    writer.apply_delta(empty_delta(
        repository,
        worktree,
        None,
        first,
        b"history-anchor-first-run",
    ))?;
    writer
        .connection
        .execute_batch(
            "DELETE FROM syntaxmesh_generation_history;
                 DELETE FROM syntaxmesh_generation_lineage;
                 DELETE FROM syntaxmesh_schema_migrations WHERE version = 12;
                 DELETE FROM syntaxmesh_schema_migrations WHERE version > 11;
                 UPDATE syntaxmesh_schema SET version = 11 WHERE id = 1;",
        )
        .map_err(sql_error)?;
    drop(writer);

    let mut migrated = open_migrated(&path)?;
    let history = migrated.generation_history()?;
    let anchor_state = migrated.historical_snapshot(first)?;
    migrated.apply_delta(empty_delta(
        repository,
        worktree,
        Some(first),
        next,
        b"history-anchor-next-run",
    ))?;
    migrated
        .connection
        .execute("DELETE FROM syntaxmesh_fact_versions", [])
        .map_err(sql_error)?;
    migrated
            .connection
            .execute_batch("DROP TABLE syntaxmesh_schema_migrations; UPDATE syntaxmesh_schema SET version = 2 WHERE id = 1;")
            .map_err(sql_error)?;
    drop(migrated);
    let reopened = open_migrated(&path)?;
    let retained_anchor_state = reopened.historical_snapshot(first)?;
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite history fixture: {error}")))?;

    if history.len() != 1
        || !history.first().is_some_and(|entry| {
            entry.manifest.generation == first && entry.delta.is_none() && entry.anchor.is_some()
        })
        || anchor_state != retained_anchor_state
    {
        return Err(StoreError::Integrity(
            "legacy SQLite generation was not retained as a replayable anchor".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn rejects_reads_from_a_superseded_open_snapshot() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-{}-stale-reader.db",
        std::process::id()
    ));
    let repository = RepositoryId::derive(&[b"sqlite-reader-repo"]);
    let worktree = WorktreeId::derive(&[b"sqlite-reader-worktree"]);
    let first = GenerationId::derive(&[b"sqlite-reader-first"]);
    let next = GenerationId::derive(&[b"sqlite-reader-next"]);
    let mut writer = open_migrated(&path)?;
    writer.apply_delta(empty_delta(repository, worktree, None, first, b"first-run"))?;
    let stale_reader = open_migrated(&path)?;
    writer.apply_delta(empty_delta(
        repository,
        worktree,
        Some(first),
        next,
        b"next-run",
    ))?;
    let expected_history = writer.generation_history()?;
    let stale_read = stale_reader.nodes(first);
    drop(stale_reader);
    drop(writer);
    let reopened = open_migrated(&path)?;
    let restored_history = reopened.generation_history()?;
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite fixture: {error}")))?;
    if !matches!(
        stale_read,
        Err(StoreError::StaleBase {
            expected: Some(expected),
            actual: Some(actual),
        }) if expected == first && actual == next
    ) {
        return Err(StoreError::Integrity(
            "SQLite served a superseded in-memory snapshot".to_owned(),
        ));
    }
    if expected_history.len() != 2 || restored_history != expected_history {
        return Err(StoreError::Integrity(
            "SQLite generation history did not survive restart".to_owned(),
        ));
    }
    Ok(())
}

fn schema_v1_root(
    generation: GenerationId,
    nodes: &[Node],
    edges: &[Edge],
) -> Result<[u8; 32], StoreError> {
    let mut bytes = Vec::new();
    let mut nodes = nodes.to_vec();
    nodes.sort_by_key(|node| node.id);
    let mut edges = edges.to_vec();
    edges.sort_by_key(|edge| edge.id);
    for node in nodes {
        bytes.extend_from_slice(&serde_json::to_vec(&node).map_err(|error| {
            StoreError::Backend(format!("serialize schema-v1 test node: {error}"))
        })?);
    }
    for edge in edges {
        bytes.extend_from_slice(&serde_json::to_vec(&edge).map_err(|error| {
            StoreError::Backend(format!("serialize schema-v1 test edge: {error}"))
        })?);
    }
    Ok(*blake3::hash(&[generation.0.0.as_slice(), bytes.as_slice()].concat()).as_bytes())
}

fn empty_delta(
    repository: RepositoryId,
    worktree: WorktreeId,
    expected_base: Option<GenerationId>,
    next_generation: GenerationId,
    run_name: &[u8],
) -> GraphDelta {
    GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[run_name]),
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
