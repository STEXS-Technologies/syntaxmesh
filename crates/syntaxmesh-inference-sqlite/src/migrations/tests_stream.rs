use super::*;

#[test]
fn v3_stream_upgrade_is_explicit_empty_and_atomic() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("v3.sqlite");
    let connection = Connection::open(&path)?;
    for migration in MIGRATIONS.iter().filter(|entry| entry.version <= 3) {
        connection.execute_batch(migration.sql)?;
        connection.execute(
            "INSERT INTO inference_migrations VALUES (?1, ?2, ?3)",
            params![
                migration.version,
                migration.name,
                blake3::hash(migration.sql.as_bytes()).to_hex().to_string()
            ],
        )?;
    }
    connection.pragma_update(None, "user_version", 3)?;
    if open(&path).is_ok() {
        return Err("ordinary open silently upgraded v3".into());
    }
    connection.execute_batch("CREATE TRIGGER interrupt_stream_migration BEFORE INSERT ON inference_migrations WHEN NEW.version = 4 BEGIN SELECT RAISE(ABORT, 'injected stream upgrade failure'); END;")?;
    if migrate(&path).is_ok() {
        return Err("stream upgrade failure ignored".into());
    }
    let partial: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'inference_stream%'",
        [],
        |row| row.get(0),
    )?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if partial != 0 || version != 3 {
        return Err("failed stream migration was not atomic".into());
    }
    connection.execute_batch("DROP TRIGGER interrupt_stream_migration")?;
    migrate(&path)?;
    let opened = open(&path)?;
    let streams: i64 = opened.query_row("SELECT COUNT(*) FROM inference_streams", [], |row| {
        row.get(0)
    })?;
    if streams != 0 {
        return Err("upgrade fabricated evaluated streams".into());
    }
    Ok(())
}
