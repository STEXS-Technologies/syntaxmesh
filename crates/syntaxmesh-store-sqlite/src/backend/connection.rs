//! SQLite connection lifecycle and process-safety configuration.
//!
//! Keep path validation, open flags, and connection-local pragmas together so
//! every public store lifecycle operation applies the same policy.

use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, OpenFlags, config::DbConfig};
use syntaxmesh_store::StoreError;

use super::sql_error;

pub(super) fn ensure_wal_mode(connection: &Connection) -> Result<(), StoreError> {
    let started = Instant::now();
    loop {
        let mode = connection
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        if mode.eq_ignore_ascii_case("wal") {
            return Ok(());
        }
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(error)
                if matches!(
                    error.sqlite_error_code(),
                    Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
                ) && started.elapsed() < Duration::from_secs(30) =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(sql_error(error)),
        }
    }
}

pub(super) fn ensure_sqlite_database_path_is_safe(path: &Path) -> Result<(), StoreError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StoreError::Backend(format!(
                "SQLite database path is not a regular, non-symlink file: {}",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StoreError::Backend(format!(
            "inspect SQLite database path {}: {error}",
            path.display()
        ))),
    }
}

pub(super) const fn sqlite_create_flags() -> OpenFlags {
    OpenFlags::SQLITE_OPEN_READ_WRITE
        .union(OpenFlags::SQLITE_OPEN_CREATE)
        .union(OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .union(OpenFlags::SQLITE_OPEN_NOFOLLOW)
        .union(OpenFlags::SQLITE_OPEN_EXRESCODE)
}

pub(super) const fn sqlite_existing_flags() -> OpenFlags {
    OpenFlags::SQLITE_OPEN_READ_WRITE
        .union(OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .union(OpenFlags::SQLITE_OPEN_NOFOLLOW)
        .union(OpenFlags::SQLITE_OPEN_EXRESCODE)
}

pub(super) const fn sqlite_read_only_flags() -> OpenFlags {
    OpenFlags::SQLITE_OPEN_READ_ONLY
        .union(OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .union(OpenFlags::SQLITE_OPEN_NOFOLLOW)
        .union(OpenFlags::SQLITE_OPEN_EXRESCODE)
}

pub(super) fn prepare_sqlite_connection(connection: &Connection) -> Result<(), StoreError> {
    connection
        .busy_timeout(Duration::from_secs(30))
        .map_err(sql_error)?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(sql_error)?;
    connection
        .execute_batch(
            "PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON; PRAGMA trusted_schema = OFF; PRAGMA cell_size_check = ON;",
        )
        .map_err(sql_error)
}
