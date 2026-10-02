use super::{CliError, Client};
use crate::cli::identifiers::{parse_file_id, parse_repository_id, parse_worktree_id};
use serde::Deserialize;
use syntaxmesh_core::{FileVersion, GenerationManifest};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    repository: String,
    worktree: String,
    items: Vec<FileVersion>,
    has_more: bool,
    next_after: Option<String>,
}

pub(super) fn inventory(
    client: &Client<'_>,
    manifest: &GenerationManifest,
    count: usize,
) -> Result<Vec<FileVersion>, CliError> {
    let generation = manifest.generation.0.to_hex();
    let mut after = None;
    let mut items = Vec::new();
    loop {
        let encoded_after = after.map(|file: syntaxmesh_core::FileId| file.0.to_hex());
        let mut parameters = vec![("generation", generation.as_str()), ("limit", "100")];
        if let Some(cursor) = encoded_after.as_deref() {
            parameters.push(("after_file", cursor));
        }
        let (actual, page): (_, Page) = client.get_data("/api/v1/files", &parameters)?;
        let next = page.next_after.as_deref().map(parse_file_id).transpose()?;
        if actual != manifest.generation
            || parse_repository_id(&page.repository)? != manifest.repository
            || parse_worktree_id(&page.worktree)? != manifest.worktree
            || page.items.len() > 100
            || page.has_more != next.is_some()
            || (page.has_more && page.items.len() != 100)
            || items.len().saturating_add(page.items.len()) > count
        {
            return Err(invalid());
        }
        let mut last = after;
        for file in page.items {
            if last.is_some_and(|previous| file.file_id <= previous) {
                return Err(invalid());
            }
            last = Some(file.file_id);
            items.push(file);
        }
        if let Some(cursor) = next {
            if Some(cursor) != last || items.len() >= count {
                return Err(invalid());
            }
            after = Some(cursor);
        } else {
            if items.len() != count {
                return Err(invalid());
            }
            return Ok(items);
        }
    }
}

fn invalid() -> CliError {
    CliError::Usage("daemon attachment failed: inconsistent file inventory page".to_owned())
}
