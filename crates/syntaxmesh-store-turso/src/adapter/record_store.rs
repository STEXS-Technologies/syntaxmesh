//! Durable opaque record access for Turso-backed workflows.

use syntaxmesh_store::{DurableRecordPage, DurableRecordStore, StoreError};

use super::{TursoGraphStore, TursoStoreError};

async fn read_record(
    connection: &turso::Connection,
    key: &str,
) -> Result<Option<Vec<u8>>, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT payload FROM syntaxmesh_records WHERE record_key = ?1",
            [key],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read durable record: {error}")))?;
    let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read durable record row: {error}")))?
    else {
        return Ok(None);
    };
    match row.get_value(0).map_err(|error| {
        TursoStoreError::Backend(format!("read durable record payload: {error}"))
    })? {
        turso::Value::Blob(payload) => Ok(Some(payload)),
        turso::Value::Null
        | turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_) => Err(TursoStoreError::Snapshot(
            "durable record payload is not a blob".to_owned(),
        )),
    }
}

async fn read_records(
    connection: &turso::Connection,
    prefix: &str,
) -> Result<Vec<(String, Vec<u8>)>, TursoStoreError> {
    let upper = prefix_upper_bound(prefix);
    let mut rows = match upper.as_deref() {
        Some(upper) => connection
            .query(
                "SELECT record_key, payload FROM syntaxmesh_records WHERE record_key >= ?1 AND record_key < ?2 ORDER BY record_key",
                (prefix, upper),
            )
            .await,
        None => connection
            .query(
                "SELECT record_key, payload FROM syntaxmesh_records WHERE record_key >= ?1 ORDER BY record_key",
                [prefix],
            )
            .await,
    }
    .map_err(|error| TursoStoreError::Backend(format!("read durable records: {error}")))?;
    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read durable record row: {error}")))?
    {
        let key: String = row.get(0).map_err(|error| {
            TursoStoreError::Backend(format!("decode durable record key: {error}"))
        })?;
        let payload = match row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read durable record payload: {error}"))
        })? {
            turso::Value::Blob(payload) => payload,
            turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "durable record payload is not a blob".to_owned(),
                ));
            }
        };
        records.push((key, payload));
    }
    Ok(records)
}

async fn read_records_page(
    connection: &turso::Connection,
    prefix: &str,
    after_key: Option<&str>,
    limit: usize,
) -> Result<DurableRecordPage, TursoStoreError> {
    if limit == 0 {
        return Ok(DurableRecordPage {
            records: Vec::new(),
            next_cursor: None,
        });
    }
    let sql_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let upper = prefix_upper_bound(prefix);
    let mut rows = match upper.as_deref() {
        Some(upper) => {
            connection
                .query(
                    "SELECT record_key, payload FROM syntaxmesh_records \
             WHERE record_key >= ?1 AND record_key < ?2 AND (?3 IS NULL OR record_key > ?3) \
             ORDER BY record_key LIMIT ?4",
                    (prefix, upper, after_key, sql_limit),
                )
                .await
        }
        None => {
            connection
                .query(
                    "SELECT record_key, payload FROM syntaxmesh_records \
             WHERE record_key >= ?1 AND (?2 IS NULL OR record_key > ?2) \
             ORDER BY record_key LIMIT ?3",
                    (prefix, after_key, sql_limit),
                )
                .await
        }
    }
    .map_err(|error| TursoStoreError::Backend(format!("read durable record page: {error}")))?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read durable record page row: {error}"))
    })? {
        let key: String = row.get(0).map_err(|error| {
            TursoStoreError::Backend(format!("decode durable record key: {error}"))
        })?;
        let payload = match row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read durable record payload: {error}"))
        })? {
            turso::Value::Blob(payload) => payload,
            turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "durable record payload is not a blob".to_owned(),
                ));
            }
        };
        records.push((key, payload));
    }
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

async fn read_last_record(
    connection: &turso::Connection,
    prefix: &str,
) -> Result<Option<(String, Vec<u8>)>, TursoStoreError> {
    let upper = prefix_upper_bound(prefix);
    let mut rows = match upper.as_deref() {
        Some(upper) => {
            connection
                .query(
                    "SELECT record_key, payload FROM syntaxmesh_records \
                 WHERE record_key >= ?1 AND record_key < ?2 ORDER BY record_key DESC LIMIT 1",
                    (prefix, upper),
                )
                .await
        }
        None => {
            connection
                .query(
                    "SELECT record_key, payload FROM syntaxmesh_records WHERE record_key = ?1",
                    [prefix],
                )
                .await
        }
    }
    .map_err(|error| TursoStoreError::Backend(format!("read latest durable record: {error}")))?;
    let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read latest durable record row: {error}"))
    })?
    else {
        return Ok(None);
    };
    let key = row.get(0).map_err(|error| {
        TursoStoreError::Backend(format!("decode latest durable record key: {error}"))
    })?;
    let payload = match row.get_value(1).map_err(|error| {
        TursoStoreError::Backend(format!("read latest durable record payload: {error}"))
    })? {
        turso::Value::Blob(payload) => payload,
        turso::Value::Null
        | turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_) => {
            return Err(TursoStoreError::Snapshot(
                "latest durable record payload is not a blob".to_owned(),
            ));
        }
    };
    Ok(Some((key, payload)))
}

/// Returns the smallest Unicode string greater than every string with `prefix`.
/// UTF-8 byte ordering preserves Unicode scalar ordering for valid Rust strings.
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

impl DurableRecordStore for TursoGraphStore {
    fn read_record(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.runtime
            .block_on(read_record(&self.connection, key))
            .map_err(Self::map_backend)
    }

    fn records_with_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        self.runtime
            .block_on(read_records(&self.connection, prefix))
            .map_err(Self::map_backend)
    }

    fn records_with_prefix_page(
        &self,
        prefix: &str,
        after_key: Option<&str>,
        limit: usize,
    ) -> Result<DurableRecordPage, StoreError> {
        self.runtime
            .block_on(read_records_page(
                &self.connection,
                prefix,
                after_key,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn last_record_with_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        self.runtime
            .block_on(read_last_record(&self.connection, prefix))
            .map_err(Self::map_backend)
    }

    fn compare_exchange_record(
        &mut self,
        key: &str,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), StoreError> {
        self.runtime
            .block_on(async {
                let transaction = self
                    .connection
                    .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
                    .await
                    .map_err(|error| {
                        TursoStoreError::Backend(format!("begin record transaction: {error}"))
                    })?;
                let actual = read_record(&transaction, key).await?;
                if actual.as_deref() != expected {
                    return Err(TursoStoreError::Store(StoreError::RecordConflict(
                        key.to_owned(),
                    )));
                }
                transaction
                    .execute(
                        "INSERT INTO syntaxmesh_records (record_key, payload) VALUES (?1, ?2) ON CONFLICT(record_key) DO UPDATE SET payload = excluded.payload",
                        (key, replacement),
                    )
                    .await
                    .map_err(|error| {
                        TursoStoreError::Backend(format!("write durable record: {error}"))
                    })?;
                transaction.commit().await.map_err(|error| {
                    TursoStoreError::Backend(format!("commit durable record: {error}"))
                })?;
                Ok::<(), TursoStoreError>(())
            })
            .map_err(Self::map_backend)?;
        Ok(())
    }
}
