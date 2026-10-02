use syntaxmesh_core::{FileId, FileVersion, GenerationHistoryEntry, GenerationId};
use syntaxmesh_store::{
    HistoricalFilePage, MAX_HISTORICAL_FILE_PAGE_SIZE, PersistentFactKey, PersistentFactNode,
    PersistentFactTreeCache, PersistentFactTreeError, PersistentFactTreeRangeWalker, StoreError,
};

use super::{FILE_FACT, TursoStoreError, read_many_with_params};

pub(super) async fn historical_files_page(
    connection: &turso::Connection,
    generation: GenerationId,
    after: Option<FileId>,
    limit: usize,
) -> Result<HistoricalFilePage, TursoStoreError> {
    if limit == 0 || limit > MAX_HISTORICAL_FILE_PAGE_SIZE {
        return Err(TursoStoreError::Store(StoreError::InvalidPageLimit));
    }
    let entry = read_many_with_params::<GenerationHistoryEntry>(
        connection,
        "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
        [generation.0.0.to_vec()],
    )
    .await?
    .pop()
    .ok_or(TursoStoreError::Store(StoreError::StaleBase {
        expected: Some(generation),
        actual: None,
    }))?;
    let root = super::temporal_tree::read_persistent_root(connection, generation).await?;
    let kind =
        u8::try_from(FILE_FACT).map_err(|error| TursoStoreError::Snapshot(error.to_string()))?;
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
                let file: FileVersion = super::temporal_tree::decode_tree_fact(
                    page.value_payload(),
                    entry.manifest.schema_version,
                )?;
                if file.file_id.0 != page.key.fact_id {
                    return Err(TursoStoreError::Snapshot(
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
                let page = read_many_with_params::<PersistentFactNode>(
                    connection,
                    "SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1",
                    [id.0.to_vec()],
                )
                .await?
                .pop()
                .ok_or_else(|| {
                    TursoStoreError::Snapshot(
                        "historical file inventory references a missing page".to_owned(),
                    )
                })?;
                cache
                    .insert_loaded(id, page)
                    .map_err(|error| super::temporal_tree::persistent_tree_error(&error))?;
            }
            Err(error) => return Err(super::temporal_tree::persistent_tree_error(&error)),
        }
    }
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(HistoricalFilePage { items, has_more })
}
