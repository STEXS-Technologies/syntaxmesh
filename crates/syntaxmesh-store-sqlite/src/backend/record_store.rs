//! Durable opaque record access for SQLite-backed workflows.

use rusqlite::{OptionalExtension, params};
use syntaxmesh_store::{DurableRecordPage, DurableRecordStore, StoreError};

use super::{SqliteGraphStore, sql_error};

pub(super) fn prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut chars = prefix.chars().collect::<Vec<_>>();
    while let Some(last) = chars.pop() {
        let codepoint = u32::from(last);
        let next = codepoint
            .checked_add(1)
            .map(|next| if next == 0xD800 { 0xE000 } else { next })
            .and_then(char::from_u32);
        if let Some(next) = next {
            chars.push(next);
            return Some(chars.into_iter().collect());
        }
    }
    None
}

impl DurableRecordStore for SqliteGraphStore {
    fn read_record(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.connection
            .query_row(
                "SELECT payload FROM syntaxmesh_records WHERE record_key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)
    }

    fn records_with_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let row =
            |row: &rusqlite::Row<'_>| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?));
        if let Some(upper) = prefix_upper_bound(prefix) {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT record_key, payload FROM syntaxmesh_records \
                     WHERE record_key >= ?1 AND record_key < ?2 ORDER BY record_key",
                )
                .map_err(sql_error)?;
            statement
                .query_map(params![prefix, upper], row)
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)
        } else {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT record_key, payload FROM syntaxmesh_records \
                     WHERE record_key >= ?1 ORDER BY record_key",
                )
                .map_err(sql_error)?;
            statement
                .query_map([prefix], row)
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)
        }
    }

    fn records_with_prefix_page(
        &self,
        prefix: &str,
        after_key: Option<&str>,
        limit: usize,
    ) -> Result<DurableRecordPage, StoreError> {
        if limit == 0 {
            return Ok(DurableRecordPage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let sql_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
        let row =
            |row: &rusqlite::Row<'_>| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?));
        let mut records = if let Some(upper) = prefix_upper_bound(prefix) {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT record_key, payload FROM syntaxmesh_records \
                 WHERE record_key >= ?1 AND record_key < ?2 AND (?3 IS NULL OR record_key > ?3) \
                 ORDER BY record_key LIMIT ?4",
                )
                .map_err(sql_error)?;
            statement
                .query_map(params![prefix, upper, after_key, sql_limit], row)
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?
        } else {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT record_key, payload FROM syntaxmesh_records \
                 WHERE record_key >= ?1 AND (?2 IS NULL OR record_key > ?2) \
                 ORDER BY record_key LIMIT ?3",
                )
                .map_err(sql_error)?;
            statement
                .query_map(params![prefix, after_key, sql_limit], row)
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?
        };
        let has_more = records.len() > limit;
        if has_more {
            records.pop();
        }
        let next_cursor = if has_more {
            records.last().map(|(key, _)| key.clone())
        } else {
            None
        };
        Ok(DurableRecordPage {
            records,
            next_cursor,
        })
    }

    fn last_record_with_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        let result = prefix_upper_bound(prefix).map_or_else(
            || {
                self.connection
                    .query_row(
                        "SELECT record_key, payload FROM syntaxmesh_records WHERE record_key = ?1",
                        [prefix],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
                    )
                    .optional()
            },
            |upper| {
                self.connection
                    .query_row(
                        "SELECT record_key, payload FROM syntaxmesh_records \
                 WHERE record_key >= ?1 AND record_key < ?2 ORDER BY record_key DESC LIMIT 1",
                        params![prefix, upper],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
                    )
                    .optional()
            },
        );
        result.map_err(sql_error)
    }

    fn compare_exchange_record(
        &mut self,
        key: &str,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let actual = transaction
            .query_row(
                "SELECT payload FROM syntaxmesh_records WHERE record_key = ?1",
                [key],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(sql_error)?;
        if actual.as_deref() != expected {
            return Err(StoreError::RecordConflict(key.to_owned()));
        }
        transaction
            .execute(
                "INSERT INTO syntaxmesh_records (record_key, payload) VALUES (?1, ?2) ON CONFLICT(record_key) DO UPDATE SET payload = excluded.payload",
                params![key, replacement],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        let cached = self.memory.read_record(key)?;
        self.memory
            .compare_exchange_record(key, cached.as_deref(), replacement)
    }
}
