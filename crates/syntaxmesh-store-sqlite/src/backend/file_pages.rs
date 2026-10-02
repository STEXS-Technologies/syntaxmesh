use rusqlite::{Connection, OptionalExtension as _};
use syntaxmesh_core::{FileId, FileVersion, GenerationId};
use syntaxmesh_store::{
    HistoricalFilePage, MAX_HISTORICAL_FILE_PAGE_SIZE, PersistentFactKey, PersistentFactTreeCache,
    PersistentFactTreeError, PersistentFactTreeRangeWalker, StoreError,
};

use super::{FILE_FACT, decode, decode_tree_fact, read_generation_history_entry, sql_error};

pub(super) fn historical_files_page(
    connection: &Connection,
    generation: GenerationId,
    after: Option<FileId>,
    limit: usize,
) -> Result<HistoricalFilePage, StoreError> {
    if limit == 0 || limit > MAX_HISTORICAL_FILE_PAGE_SIZE {
        return Err(StoreError::InvalidPageLimit);
    }
    let entry =
        read_generation_history_entry(connection, generation)?.ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        })?;
    let root = super::tree_store::read_persistent_root(connection, generation)?;
    let kind = u8::try_from(FILE_FACT).map_err(|error| StoreError::Integrity(error.to_string()))?;
    let seek = after.map(|id| PersistentFactKey {
        fact_kind: kind,
        fact_id: id.0,
    });
    let mut walker = PersistentFactTreeRangeWalker::new(root, seek);
    let mut cache = PersistentFactTreeCache::default();
    let mut items = Vec::new();
    loop {
        match walker.next(&cache) {
            Ok(Some(page)) => {
                if page.key.fact_kind != kind {
                    break;
                }
                let file: FileVersion =
                    decode_tree_fact(page.value_payload(), entry.manifest.schema_version)?;
                if file.file_id.0 != page.key.fact_id {
                    return Err(StoreError::Integrity(
                        "historical file identity differs from tree key".to_owned(),
                    ));
                }
                items.push(file);
                cache = PersistentFactTreeCache::default();
                if items.len() > limit {
                    break;
                }
            }
            Ok(None) => break,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                let payload = connection
                    .query_row(
                        "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                        [id.0.as_slice()],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| {
                        StoreError::Integrity(
                            "historical file inventory references a missing page".to_owned(),
                        )
                    })?;
                cache
                    .insert_loaded(id, decode(&payload)?)
                    .map_err(|error| super::tree_store::persistent_tree_error(&error))?;
            }
            Err(error) => return Err(super::tree_store::persistent_tree_error(&error)),
        }
    }
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(HistoricalFilePage { items, has_more })
}
