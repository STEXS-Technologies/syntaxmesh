use super::*;

#[test]
fn v2_receipt_upgrade_retains_only_last_accepted_effect_and_rolls_back() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("v2.sqlite");
    let connection = Connection::open(&path)?;
    for migration in MIGRATIONS.iter().filter(|entry| entry.version <= 2) {
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
    connection.pragma_update(None, "user_version", 2)?;
    connection.execute("INSERT INTO inference_operations VALUES ('operation', 'repo', 'worktree', 'origin', 2, 'configuration', 0)", [])?;
    connection.execute(
        "INSERT INTO inference_lifecycle VALUES ('operation', 'last', 5, 0, 0)",
        [],
    )?;
    if open(&path).is_ok() {
        return Err("ordinary open silently upgraded v2".into());
    }
    connection.execute_batch("CREATE TRIGGER interrupt_receipts BEFORE INSERT ON inference_migrations WHEN NEW.version = 3 BEGIN SELECT RAISE(ABORT, 'injected receipt upgrade failure'); END;")?;
    if migrate(&path).is_ok() {
        return Err("receipt migration failure ignored".into());
    }
    let partial: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'inference_lifecycle_receipts'",
        [],
        |row| row.get(0),
    )?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if partial != 0 || version != 2 {
        return Err("failed receipt migration was not atomic".into());
    }
    connection.execute_batch("DROP TRIGGER interrupt_receipts")?;
    migrate(&path)?;
    let opened = open(&path)?;
    let (count, generation, sequence): (i64, String, i64) = opened.query_row(
        "SELECT COUNT(*), through_generation, through_sequence FROM inference_lifecycle_receipts",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if count != 1 || generation != "last" || sequence != 5 {
        return Err("migration invented historical effect receipts".into());
    }
    Ok(())
}
