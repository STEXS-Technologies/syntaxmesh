use super::*;

#[test]
fn child_corruption_rolls_back_partial_scan_and_repair_recovers()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let mut store = open_migrated(scratch.path().join("speculative-corruption.db"))?;
    let generation = GenerationId::derive(&[b"speculative-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"speculative-provenance"]),
        producer_namespace: "scan-test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let mut delta = empty_delta(
        RepositoryId::derive(&[b"speculative-repo"]),
        WorktreeId::derive(&[b"speculative-worktree"]),
        syntaxmesh_core::IndexRunId::derive(&[b"speculative-run"]),
        None,
        generation,
    );
    delta.upsert_nodes = (0_u64..128)
        .map(|seed| Node {
            id: NodeId::derive(&[&seed.to_le_bytes()]),
            kind: NodeKind::Function,
            name: format!("node_{seed}"),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        })
        .collect();
    delta.upsert_provenance.push(provenance);
    store.apply_delta(delta)?;
    let (target, original, corrupt) = store.runtime.block_on(async {
        let mut cursor =
            super::super::temporal_tree::read_persistent_root(&store.connection, generation)
                .await?;
        while let Some(id) = cursor {
            let page = read_many_with_params::<syntaxmesh_store::PersistentFactNode>(
                &store.connection,
                "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                [id.0.to_vec()],
            )
            .await?
            .pop()
            .ok_or("missing fixture traversal page")?;
            if i64::from(page.key.fact_kind) == NODE_FACT
                && page.left.is_some()
                && let Some(right) = page.right
            {
                let mut child = read_many_with_params::<syntaxmesh_store::PersistentFactNode>(
                    &store.connection,
                    "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                    [right.0.to_vec()],
                )
                .await?
                .pop()
                .ok_or("missing speculative child")?;
                let original = bincode::serialize(&child)?;
                child.priority = syntaxmesh_core::StableId([0; 32]);
                return Ok::<_, Box<dyn std::error::Error>>((
                    right,
                    original,
                    bincode::serialize(&child)?,
                ));
            }
            cursor = if i64::from(page.key.fact_kind) < NODE_FACT {
                page.right
            } else {
                page.left
            };
        }
        Err("fixture lacks a speculative right branch before first delivery".into())
    })?;
    store.runtime.block_on(store.connection.execute(
        "UPDATE syntaxmesh_temporal_tree_pages SET payload = ?1 WHERE page_id = ?2",
        (corrupt, target.0.to_vec()),
    ))?;
    let mut callbacks = 0_usize;
    let rejected = store.visit_historical_nodes(generation, 128, &mut |_node| {
        callbacks = callbacks.saturating_add(1);
        Ok(())
    });
    if rejected.is_ok() || callbacks >= 128 || !store.connection.is_autocommit()? {
        return Err("child corruption did not reject incomplete scan and clean up".into());
    }
    store.runtime.block_on(store.connection.execute(
        "UPDATE syntaxmesh_temporal_tree_pages SET payload = ?1 WHERE page_id = ?2",
        (original, target.0.to_vec()),
    ))?;
    if store.visit_historical_nodes(generation, 128, &mut |_node| Ok(()))? != 128
        || !store.connection.is_autocommit()?
    {
        return Err("repair did not restore complete scan and cleanup".into());
    }
    Ok(())
}

#[test]
fn deferred_read_transaction_pins_rows_across_peer_commit() -> Result<(), Box<dyn std::error::Error>>
{
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("scan-snapshot.db");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let database = turso::Builder::new_local(path.to_str().ok_or("non-UTF-8 snapshot path")?)
            .build()
            .await?;
        let reader = database.connect()?;
        let writer = database.connect()?;
        reader.pragma_update("journal_mode", "'wal'").await?;
        reader
            .execute(
                "CREATE TABLE scan_snapshot_probe (id INTEGER PRIMARY KEY, value INTEGER)",
                (),
            )
            .await?;
        reader
            .execute("INSERT INTO scan_snapshot_probe VALUES (1, 7)", ())
            .await?;
        let transaction = turso::transaction::Transaction::new_unchecked(
            &reader,
            turso::transaction::TransactionBehavior::Deferred,
        )
        .await?;
        if read_probe_value(&transaction).await? != 7 {
            return Err("initial deferred snapshot differs".into());
        }
        writer
            .execute("UPDATE scan_snapshot_probe SET value = 8", ())
            .await?;
        if read_probe_value(&transaction).await? != 7 {
            return Err("peer commit changed active read snapshot".into());
        }
        transaction.rollback().await?;
        if !reader.is_autocommit()? || read_probe_value(&reader).await? != 8 {
            return Err("next read did not observe committed peer state".into());
        }
        Ok(())
    })
}

async fn read_probe_value(
    connection: &turso::Connection,
) -> Result<i64, Box<dyn std::error::Error>> {
    let mut rows = connection
        .query("SELECT value FROM scan_snapshot_probe WHERE id = 1", ())
        .await?;
    Ok(rows
        .next()
        .await?
        .ok_or("missing snapshot probe row")?
        .get(0)?)
}

#[test]
fn node_scan_cleans_up_success_budget_callback_and_corrupt_history_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let mut store = open_migrated(scratch.path().join("node-scan.db"))?;
    let generation = GenerationId::derive(&[b"scan-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"scan-provenance"]),
        producer_namespace: "scan-test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let mut delta = empty_delta(
        RepositoryId::derive(&[b"scan-repo"]),
        WorktreeId::derive(&[b"scan-worktree"]),
        syntaxmesh_core::IndexRunId::derive(&[b"scan-run"]),
        None,
        generation,
    );
    delta.upsert_nodes = ["a", "b", "c"]
        .into_iter()
        .map(|name| Node {
            id: NodeId::derive(&[name.as_bytes()]),
            kind: NodeKind::Function,
            name: if name == "a" {
                "a".repeat(300_000)
            } else {
                name.to_owned()
            },
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        })
        .collect();
    delta.upsert_provenance.push(provenance);
    store.apply_delta(delta)?;
    let mut large_delivered = false;
    if store.visit_historical_nodes(generation, 3, &mut |node| {
        large_delivered |= node.name.len() == 300_000;
        Ok(())
    })? != 3
        || !large_delivered
        || !store.connection.is_autocommit()?
    {
        return Err("successful scan left transaction open or lost nodes".into());
    }
    let mut callbacks = 0_usize;
    let stopped = store.visit_historical_nodes(generation, 3, &mut |_node| {
        callbacks = callbacks.saturating_add(1);
        Err(StoreError::Integrity("stop scan".to_owned()))
    });
    if callbacks != 1
        || !matches!(stopped, Err(StoreError::Integrity(ref message)) if message == "stop scan")
        || !store.connection.is_autocommit()?
    {
        return Err("callback failure did not stop/clean up scan".into());
    }
    if !matches!(
        store.visit_historical_nodes(generation, 1, &mut |_node| Ok(())),
        Err(StoreError::InvalidPageLimit)
    ) || !store.connection.is_autocommit()?
    {
        return Err("budget failure left transaction open".into());
    }
    let history = store.generation_history()?;
    let original = bincode::serialize(history.first().ok_or("missing scan history")?)?;
    store.runtime.block_on(store.connection.execute(
        "UPDATE syntaxmesh_generation_history SET payload = x'00'",
        (),
    ))?;
    let mut corrupt_callbacks = 0_usize;
    let corrupted = store.visit_historical_nodes(generation, 3, &mut |_node| {
        corrupt_callbacks = corrupt_callbacks.saturating_add(1);
        Ok(())
    });
    if corrupted.is_ok() || corrupt_callbacks != 0 || !store.connection.is_autocommit()? {
        return Err("warm scan hid corruption or did not clean up".into());
    }
    store.runtime.block_on(store.connection.execute(
        "UPDATE syntaxmesh_generation_history SET payload = ?1",
        [original],
    ))?;
    if store.visit_historical_nodes(generation, 3, &mut |_node| Ok(()))? != 3
        || !store.connection.is_autocommit()?
    {
        return Err("repaired scan did not recover cleanly".into());
    }
    Ok(())
}
