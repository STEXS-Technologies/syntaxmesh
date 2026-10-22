use crate::Result;
use rusqlite::{Connection, OpenFlags, params};
use std::path::Path;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
    backfill: Option<fn(&Connection) -> Result<()>>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "complete_claim_selections",
        sql: include_str!("../migrations/0001_complete_claim_selections.up.sql"),
        backfill: None,
    },
    Migration {
        version: 2,
        name: "premise_closure_lifecycle",
        sql: include_str!("../migrations/0002_premise_closure_lifecycle.up.sql"),
        backfill: None,
    },
    Migration {
        version: 3,
        name: "retained_lifecycle_receipts",
        sql: include_str!("../migrations/0003_retained_lifecycle_receipts.up.sql"),
        backfill: None,
    },
    Migration {
        version: 4,
        name: "incremental_stream_baselines",
        sql: include_str!("../migrations/0004_incremental_stream_baselines.up.sql"),
        backfill: None,
    },
    Migration {
        version: 5,
        name: "persistent_active_claim_roots",
        sql: include_str!("../migrations/0005_persistent_active_claim_roots.up.sql"),
        backfill: Some(crate::stream::tree::backfill),
    },
    Migration {
        version: 6,
        name: "contract_result_history",
        sql: include_str!("../migrations/0006_contract_result_history.up.sql"),
        backfill: None,
    },
    Migration {
        version: 7,
        name: "contract_outcome_episodes",
        sql: include_str!("../migrations/0007_contract_outcome_episodes.up.sql"),
        backfill: Some(crate::InferenceMaterializer::backfill_outcome_episodes),
    },
];

pub(crate) fn migrate(path: &Path) -> Result<()> {
    let mut connection = Connection::open(path)?;
    configure(&connection)?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 0 {
        validate_version(&transaction, version)?;
    }
    let tables: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT GLOB 'sqlite_*'",
        [],
        |row| row.get(0),
    )?;
    if version == 0 && tables != 0 {
        return Err("refusing to bootstrap inference schema into an unrelated database".into());
    }
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > version)
    {
        transaction.execute_batch(migration.sql)?;
        if let Some(backfill) = migration.backfill {
            backfill(&transaction)?;
        }
        transaction.execute(
            "INSERT INTO inference_migrations(version, name, checksum) VALUES (?1, ?2, ?3)",
            params![
                migration.version,
                migration.name,
                blake3::hash(migration.sql.as_bytes()).to_hex().to_string()
            ],
        )?;
        transaction.pragma_update(None, "user_version", migration.version)?;
    }
    transaction.commit()?;
    validate(&connection)
}

pub(crate) fn open(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    configure(&connection)?;
    validate(&connection)?;
    Ok(connection)
}

fn configure(connection: &Connection) -> Result<()> {
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

fn validate(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if Some(version) != MIGRATIONS.last().map(|migration| migration.version) {
        return Err(
            "inference schema requires explicit migration or is newer than this adapter".into(),
        );
    }
    validate_version(connection, version)
}

fn validate_version(connection: &Connection, version: i64) -> Result<()> {
    if !MIGRATIONS
        .iter()
        .any(|migration| migration.version == version)
    {
        return Err("unknown inference migration version".into());
    }
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM inference_migrations", [], |row| {
            row.get(0)
        })?;
    if count != version {
        return Err("inference migration ledger has unexpected entries".into());
    }
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version <= version)
    {
        let (name, checksum): (String, String) = connection.query_row(
            "SELECT name, checksum FROM inference_migrations WHERE version = ?1",
            [migration.version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if name != migration.name
            || checksum != blake3::hash(migration.sql.as_bytes()).to_hex().as_str()
        {
            return Err("inference migration ledger checksum mismatch".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_contract_history;
#[cfg(test)]
mod tests_outcome_episodes;
#[cfg(test)]
mod tests_receipts;
#[cfg(test)]
mod tests_roots;
#[cfg(test)]
mod tests_stream;
