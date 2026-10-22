use super::*;

#[test]
fn explicit_v1_upgrade_seeds_only_origin_and_rolls_back_failed_ledger_write() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("v1.sqlite");
    let connection = Connection::open(&path)?;
    let first = MIGRATIONS.first().ok_or("missing first migration")?;
    connection.execute_batch(first.sql)?;
    connection.execute(
        "INSERT INTO inference_migrations VALUES (1, ?1, ?2)",
        params![
            first.name,
            blake3::hash(first.sql.as_bytes()).to_hex().to_string()
        ],
    )?;
    connection.pragma_update(None, "user_version", 1)?;
    connection.execute("INSERT INTO inference_operations VALUES ('operation', 'repo', 'worktree', 'origin', 2, 'configuration', 0)", [])?;
    if open(&path).is_ok() {
        return Err("ordinary open upgraded v1".into());
    }
    connection.execute_batch("CREATE TRIGGER interrupt_upgrade BEFORE INSERT ON inference_migrations WHEN NEW.version = 2 BEGIN SELECT RAISE(ABORT, 'injected upgrade failure'); END;")?;
    if migrate(&path).is_ok() {
        return Err("injected migration failure ignored".into());
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let partial: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'inference_lifecycle'",
        [],
        |row| row.get(0),
    )?;
    if version != 1 || partial != 0 {
        return Err("failed migration left partial schema or advanced version".into());
    }
    connection.execute_batch("DROP TRIGGER interrupt_upgrade")?;
    migrate(&path)?;
    migrate(&path)?;
    let opened = open(&path)?;
    let (through, sequence): (String, i64) = opened.query_row(
        "SELECT through_generation, through_sequence FROM inference_lifecycle",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if through != "origin" || sequence != 2 {
        return Err("migration fabricated checked future history".into());
    }
    Ok(())
}
