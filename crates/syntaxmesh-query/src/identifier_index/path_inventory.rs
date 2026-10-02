use crate::QueryError;
use std::collections::BTreeMap;
use syntaxmesh_core::{FileId, GenerationId};
use syntaxmesh_store::{GraphStore, HistoricalFilePage, StoreError};

#[cfg(test)]
mod tests;

pub(super) fn load<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
    file_budget: usize,
    payload_budget: usize,
) -> Result<BTreeMap<FileId, String>, QueryError> {
    if file_budget == 0 || payload_budget == 0 {
        return Err(QueryError::InvalidLimit);
    }
    let mut paths = BTreeMap::new();
    let mut after = None;
    let mut bytes = 0_usize;
    loop {
        let limit = file_budget.saturating_sub(paths.len()).min(1000);
        if limit == 0 {
            return Err(QueryError::Context(
                "path inventory file budget exhausted".to_owned(),
            ));
        }
        let page = store.historical_files_page(generation, after, limit)?;
        validate_page(&page, after, limit)?;
        for file in page.items {
            bytes = super::charged_payload(bytes, file.normalized_path.len(), payload_budget)?;
            after = Some(file.file_id);
            paths.insert(file.file_id, file.normalized_path);
        }
        if !page.has_more {
            return Ok(paths);
        }
    }
}

fn validate_page(
    page: &HistoricalFilePage,
    after: Option<FileId>,
    limit: usize,
) -> Result<(), QueryError> {
    if page.items.len() > limit || (page.has_more && page.items.is_empty()) {
        return Err(StoreError::Integrity("invalid path inventory page".to_owned()).into());
    }
    let mut previous = after;
    for file in &page.items {
        if previous.is_some_and(|id| file.file_id <= id) {
            return Err(
                StoreError::Integrity("nonadvancing path inventory page".to_owned()).into(),
            );
        }
        previous = Some(file.file_id);
    }
    Ok(())
}
