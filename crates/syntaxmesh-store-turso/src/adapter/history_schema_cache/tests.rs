use std::cell::Cell;

use super::*;

#[test]
#[ignore = "isolated native page-cache timing diagnostic"]
fn native_history_payload_page_cache_probe() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("history-cache.db");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let database = turso::Builder::new_local(path.to_str().ok_or("non-UTF-8 probe path")?).build().await?;
        let connection = database.connect()?;
        let journal_mode = connection.pragma_update("journal_mode", "'wal'").await?;
        if journal_mode.first().ok_or("missing probe journal mode")?.get_value(0)?
            != turso::Value::Text("wal".to_owned()) {
            return Err("history probe did not enter WAL mode".into());
        }
        connection.execute("CREATE TABLE history_probe (id INTEGER PRIMARY KEY, payload BLOB NOT NULL)", ()).await?;
        let payload_bytes = 33_580_768_usize;
        connection.execute("INSERT INTO history_probe VALUES (1, ?1)", [vec![42_u8; payload_bytes]]).await?;
        for cache_kib in [-2_000_i64, -65_536_i64] {
            connection.pragma_update("cache_size", cache_kib.to_string()).await?;
            let mut setting = connection.query("PRAGMA cache_size", ()).await?;
            let observed: i64 = setting.next().await?.ok_or("missing cache-size row")?.get(0)?;
            drop(setting);
            let started = std::time::Instant::now();
            for _ in 0..5 {
                let mut rows = connection.query("SELECT payload FROM history_probe WHERE id = 1", ()).await?;
                let value = rows.next().await?.ok_or("missing probe payload")?.get_value(0)?;
                if !matches!(value, turso::Value::Blob(ref bytes) if bytes.len() == payload_bytes) {
                    return Err("history probe payload differs".into());
                }
            }
            eprintln!("history_page_cache_probe cache_setting={observed} payload_bytes={payload_bytes} reads=5 total_us={}", started.elapsed().as_micros());
        }
        Ok(())
    })
}

#[test]
fn warm_schema_checks_full_blob_type_changes_and_missing_rows() -> Result<(), TursoStoreError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            TursoStoreError::Backend(format!("build comparison test runtime: {error}"))
        })?;
    runtime.block_on(async {
        let comparison_error = |error: turso::Error| {
            TursoStoreError::Backend(format!("history comparison fixture: {error}"))
        };
        let database = turso::Builder::new_local(":memory:")
            .build()
            .await
            .map_err(comparison_error)?;
        let connection = database.connect().map_err(comparison_error)?;
        connection
            .execute(
                "CREATE TABLE syntaxmesh_generation_history (generation BLOB PRIMARY KEY, payload)",
                (),
            )
            .await
            .map_err(comparison_error)?;
        let generation = GenerationId::derive(&[b"comparison-generation"]);
        connection
            .execute(
                "INSERT INTO syntaxmesh_generation_history VALUES (?1, ?2)",
                turso::params![generation.0.0.to_vec(), b"record".to_vec()],
            )
            .await
            .map_err(comparison_error)?;
        let cache = HistorySchemaCache::default();
        cache.schema(generation, b"record", || Ok(2))?;
        if read_schema(&connection, &cache, generation).await? != 2 {
            return Err(TursoStoreError::Snapshot(
                "schema cache missed unchanged authoritative bytes".to_owned(),
            ));
        }
        for update in [
            "UPDATE syntaxmesh_generation_history SET payload = x'7265636f7265'",
            "UPDATE syntaxmesh_generation_history SET payload = 'record'",
            "UPDATE syntaxmesh_generation_history SET payload = NULL",
        ] {
            connection
                .execute(update, ())
                .await
                .map_err(comparison_error)?;
            if read_schema(&connection, &cache, generation).await.is_ok() {
                return Err(TursoStoreError::Snapshot(
                    "changed authoritative payload bypassed validation".to_owned(),
                ));
            }
        }
        connection
            .execute("DELETE FROM syntaxmesh_generation_history", ())
            .await
            .map_err(comparison_error)?;
        if !matches!(
            read_schema(&connection, &cache, generation).await,
            Err(TursoStoreError::Store(
                syntaxmesh_store::StoreError::StaleBase { .. }
            ))
        ) {
            return Err(TursoStoreError::Snapshot(
                "missing warm row did not reject stale base".to_owned(),
            ));
        }
        Ok(())
    })
}

#[test]
fn content_and_generation_keys_bound_decode_reuse_and_failures() {
    let cache = HistorySchemaCache::default();
    let first = GenerationId::derive(&[b"first"]);
    let second = GenerationId::derive(&[b"second"]);
    let decodes = Cell::new(0_usize);
    let decode = || {
        decodes.set(decodes.get().saturating_add(1));
        Ok(2)
    };
    assert_eq!(cache.schema(first, b"record", decode).ok(), Some(2));
    assert_eq!(cache.schema(first, b"record", decode).ok(), Some(2));
    assert_eq!(decodes.get(), 1);
    assert_eq!(cache.schema(first, b"changed", decode).ok(), Some(2));
    assert_eq!(cache.schema(second, b"changed", decode).ok(), Some(2));
    // Single-entry replacement: switching back must decode again.
    assert_eq!(cache.schema(first, b"changed", decode).ok(), Some(2));
    assert_eq!(decodes.get(), 4);
    for _ in 0..2 {
        assert!(
            cache
                .schema(first, b"corrupt", || {
                    decodes.set(decodes.get().saturating_add(1));
                    Err(TursoStoreError::Snapshot("corrupt".to_owned()))
                })
                .is_err()
        );
    }
    assert_eq!(decodes.get(), 6);
    assert_eq!(cache.schema(first, b"repaired", decode).ok(), Some(2));
    assert_eq!(decodes.get(), 7);
}
