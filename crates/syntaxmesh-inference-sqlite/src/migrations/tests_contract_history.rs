use super::*;
use crate::InferenceMaterializer;
use rusqlite::Connection;

#[test]
fn v5_to_v6_contract_history_migration_is_explicit_and_atomic() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("v5.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let connection = Connection::open(&path)?;
    connection.execute_batch(
        "DROP INDEX inference_contract_outcome_episodes_as_of;
         DROP TABLE inference_contract_outcome_episodes;
         DROP INDEX inference_contract_evaluations_at;
         DROP INDEX inference_contract_evaluations_history;
         DROP TABLE inference_contract_evaluations;
         DROP TABLE inference_contract_evaluation_sets;
         DELETE FROM inference_migrations WHERE version >= 6;
         PRAGMA user_version = 5;",
    )?;
    if open(&path).is_ok() {
        return Err("ordinary open applied the contract-history migration".into());
    }
    connection.execute_batch(
        "CREATE TRIGGER interrupt_contract_history BEFORE INSERT ON inference_migrations
         WHEN NEW.version = 6 BEGIN SELECT RAISE(ABORT, 'injected contract-history migration failure'); END;",
    )?;
    if migrate(&path).is_ok() {
        return Err("contract-history migration ledger failure was ignored".into());
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let tables: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
         ('inference_contract_evaluation_sets', 'inference_contract_evaluations')",
        [],
        |row| row.get(0),
    )?;
    if version != 5 || tables != 0 {
        return Err("failed contract-history migration left partial schema".into());
    }
    connection.execute_batch("DROP TRIGGER interrupt_contract_history")?;
    migrate(&path)?;
    let materializer = InferenceMaterializer::open(&path)?;
    let indexes: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN
         ('inference_contract_evaluations_at', 'inference_contract_evaluations_history')",
        [],
        |row| row.get(0),
    )?;
    if indexes != 2 {
        return Err("successful migration omitted contract-history indexes".into());
    }
    Ok(())
}

#[test]
fn v7_to_v8_violation_index_migration_is_explicit_and_atomic() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("v7.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let connection = Connection::open(&path)?;
    connection.execute_batch(
        "DROP INDEX inference_contract_violation_episodes_history;
         DELETE FROM inference_migrations WHERE version = 8;
         PRAGMA user_version = 7;",
    )?;
    if open(&path).is_ok() {
        return Err("ordinary open applied the violation-history migration".into());
    }
    connection.execute_batch(
        "CREATE TRIGGER interrupt_violation_history BEFORE INSERT ON inference_migrations
         WHEN NEW.version = 8 BEGIN SELECT RAISE(ABORT, 'injected violation-history migration failure'); END;",
    )?;
    if migrate(&path).is_ok() {
        return Err("violation-history migration ledger failure was ignored".into());
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let indexes_after_rollback: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'
         AND name = 'inference_contract_violation_episodes_history'",
        [],
        |row| row.get(0),
    )?;
    if version != 7 || indexes_after_rollback != 0 {
        return Err("failed violation-history migration left partial schema".into());
    }
    connection.execute_batch("DROP TRIGGER interrupt_violation_history")?;
    migrate(&path)?;
    let materializer = InferenceMaterializer::open(&path)?;
    let final_index_count: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'
         AND name = 'inference_contract_violation_episodes_history'",
        [],
        |row| row.get(0),
    )?;
    if final_index_count != 1 {
        return Err("successful migration omitted violation-history index".into());
    }
    Ok(())
}
