use super::*;
use crate::InferenceMaterializer;
use rusqlite::Connection;

#[test]
fn v6_to_v7_outcome_episode_migration_is_explicit_and_atomic() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("v6.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let connection = Connection::open(&path)?;
    connection.execute_batch(
        "DROP INDEX inference_contract_outcome_episodes_as_of;
         DROP TABLE inference_contract_outcome_episodes;
         DELETE FROM inference_migrations WHERE version = 7;
         PRAGMA user_version = 6;",
    )?;
    if open(&path).is_ok() {
        return Err("ordinary open applied the outcome episode migration".into());
    }
    connection.execute_batch(
        "CREATE TRIGGER interrupt_outcome_episode_migration BEFORE INSERT ON inference_migrations
         WHEN NEW.version = 7 BEGIN SELECT RAISE(ABORT, 'injected outcome episode migration failure'); END;",
    )?;
    if migrate(&path).is_ok() {
        return Err("outcome episode migration ledger failure was ignored".into());
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let table_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
         AND name = 'inference_contract_outcome_episodes'",
        [],
        |row| row.get(0),
    )?;
    if version != 6 || table_count != 0 {
        return Err("failed outcome episode migration left a partial schema".into());
    }
    connection.execute_batch("DROP TRIGGER interrupt_outcome_episode_migration")?;
    migrate(&path)?;
    let materializer = InferenceMaterializer::open(&path)?;
    let indexes: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'
         AND name = 'inference_contract_outcome_episodes_as_of'",
        [],
        |row| row.get(0),
    )?;
    if indexes != 1 {
        return Err("successful outcome episode migration omitted its index".into());
    }
    Ok(())
}
