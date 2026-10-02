use super::*;
use syntaxmesh_core::StableId;
use syntaxmesh_store::MAX_HISTORICAL_FILE_PAGE_SIZE;

pub(super) fn verify<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
) -> Result<(), StoreError> {
    let mut expected = store.historical_snapshot(generation)?.files;
    expected.sort_by_key(|file| file.file_id);
    let mut after = None;
    let mut collected = Vec::new();
    let mut pages = 0usize;
    loop {
        let limit = 3;
        let page = store.historical_files_page(generation, after, limit)?;
        let eligible = expected
            .iter()
            .filter(|file| after.is_none_or(|id| file.file_id > id))
            .cloned()
            .collect::<Vec<_>>();
        if page.items != eligible.iter().take(limit).cloned().collect::<Vec<_>>()
            || page.has_more != (eligible.len() > limit)
        {
            return Err(StoreError::Integrity(
                "file page differs from snapshot".to_owned(),
            ));
        }
        after = page.items.last().map(|file| file.file_id);
        collected.extend(page.items);
        if !page.has_more {
            break;
        }
        pages = pages.saturating_add(1);
        if pages > expected.len() {
            return Err(StoreError::Integrity(
                "file pages did not terminate".to_owned(),
            ));
        }
    }
    if collected != expected {
        return Err(StoreError::Integrity("file pages lost versions".to_owned()));
    }
    for seek in [
        FileId(StableId([0; 32])),
        FileId(StableId([0x80; 32])),
        FileId(StableId([u8::MAX; 32])),
    ] {
        let page =
            store.historical_files_page(generation, Some(seek), MAX_HISTORICAL_FILE_PAGE_SIZE)?;
        let eligible = expected
            .iter()
            .filter(|file| file.file_id > seek)
            .cloned()
            .collect::<Vec<_>>();
        if page.items != eligible || page.has_more {
            return Err(StoreError::Integrity("file seek differs".to_owned()));
        }
    }
    for limit in [0, MAX_HISTORICAL_FILE_PAGE_SIZE.saturating_add(1)] {
        if !matches!(
            store.historical_files_page(generation, None, limit),
            Err(StoreError::InvalidPageLimit)
        ) {
            return Err(StoreError::Integrity(
                "invalid file page limit accepted".to_owned(),
            ));
        }
    }
    if store
        .historical_files_page(GenerationId::derive(&[b"unknown-file-page"]), None, 1)
        .is_ok()
    {
        return Err(StoreError::Integrity(
            "unknown file generation accepted".to_owned(),
        ));
    }
    Ok(())
}
