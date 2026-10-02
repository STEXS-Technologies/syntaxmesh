use std::sync::Mutex;

use syntaxmesh_core::GenerationId;

use crate::TursoStoreError;

pub(super) async fn read_schema(
    connection: &turso::Connection,
    cache: &HistorySchemaCache,
    generation: GenerationId,
) -> Result<u32, TursoStoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let profile_started = std::env::var_os("SYNTAXMESH_TURSO_NODE_READ_PROFILE")
        .map(|_enabled| std::time::Instant::now());
    let mut rows = connection
        .query(
            "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query history schema: {error}")))?;
    let row = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read history schema: {error}")))?
        .ok_or(TursoStoreError::Store(
            syntaxmesh_store::StoreError::StaleBase {
                expected: Some(generation),
                actual: None,
            },
        ))?;
    let payload = row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("read history payload: {error}")))?;
    match payload {
        turso::Value::Blob(bytes) => {
            #[cfg(feature = "benchmark-instrumentation")]
            let transfer_elapsed = profile_started.map(|started| started.elapsed());
            #[cfg(feature = "benchmark-instrumentation")]
            let validation_started = profile_started.map(|_started| std::time::Instant::now());
            let schema = cache.schema(generation, &bytes, || {
                let entry: syntaxmesh_core::GenerationHistoryEntry = bincode::deserialize(&bytes)
                    .map_err(|error| {
                    TursoStoreError::Snapshot(format!("decode Turso row: {error}"))
                })?;
                if entry.manifest.generation != generation {
                    return Err(TursoStoreError::Snapshot(
                        "history schema manifest generation differs from row key".to_owned(),
                    ));
                }
                Ok(entry.manifest.schema_version)
            })?;
            #[cfg(feature = "benchmark-instrumentation")]
            if let (Some(transfer), Some(validation)) = (transfer_elapsed, validation_started) {
                eprintln!(
                    "context_history_schema payload_bytes={} transfer_us={} hash_decode_us={}",
                    bytes.len(),
                    transfer.as_micros(),
                    validation.elapsed().as_micros()
                );
            }
            Ok(schema)
        }
        turso::Value::Null
        | turso::Value::Integer(_)
        | turso::Value::Real(_)
        | turso::Value::Text(_) => Err(TursoStoreError::Snapshot(
            "history schema payload is not a blob".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct HistorySchemaCache {
    entry: Mutex<Option<(GenerationId, blake3::Hash, u32)>>,
}

impl HistorySchemaCache {
    pub(super) fn schema(
        &self,
        generation: GenerationId,
        bytes: &[u8],
        decode: impl FnOnce() -> Result<u32, TursoStoreError>,
    ) -> Result<u32, TursoStoreError> {
        let digest = blake3::hash(bytes);
        let mut entry = self.entry.lock().map_err(|_error| {
            TursoStoreError::Snapshot("history schema cache lock poisoned".to_owned())
        })?;
        if let Some((cached_generation, cached_digest, schema)) = *entry
            && cached_generation == generation
            && cached_digest == digest
        {
            return Ok(schema);
        }
        let schema = decode()?;
        *entry = Some((generation, digest, schema));
        Ok(schema)
    }
}
