use syntaxmesh_api_model::{ContextItem, ContextItemKind, ContextPack, OmittedContextSummary};

use super::{Candidate, ContextTokenCounter, finalize_count};
use crate::QueryError;

const COMPLETE_CANDIDATE_RETRY_LIMIT: usize = 6;

#[cfg(test)]
mod tests;

/// Trial admission changes only the appended item, omission counts, and token count.
/// Restore all three on rejection or tokenizer failure without cloning accepted text.
pub(super) fn try_item(
    pack: &mut ContextPack,
    mut item: ContextItem,
    omitted: OmittedContextSummary,
    counter: &impl ContextTokenCounter,
) -> Result<bool, QueryError> {
    let previous_len = pack.items.len();
    let previous_count = pack.token_count;
    let previous_omitted = pack.omitted;
    item.rank = u32::try_from(previous_len.saturating_add(1)).unwrap_or(u32::MAX);
    pack.items.push(item);
    pack.omitted = omitted;
    let result = finalize_count(pack, counter);
    if result.is_ok() && pack.token_count <= pack.token_budget {
        return Ok(true);
    }
    pack.items.truncate(previous_len);
    pack.token_count = previous_count;
    pack.omitted = previous_omitted;
    result.map(|()| false)
}

/// Admit complete source evidence first, then its deterministic excerpts.
pub(super) fn try_candidate(
    pack: &mut ContextPack,
    candidate: &Candidate,
    omitted: OmittedContextSummary,
    counter: &impl ContextTokenCounter,
    remaining_scan_bytes: &mut usize,
    remaining_excerpt_bytes: &mut usize,
) -> Result<bool, QueryError> {
    if try_item(pack, candidate.item.clone(), omitted, counter)? {
        return Ok(true);
    }
    if candidate.kind != ContextItemKind::SourceEvidence {
        return Ok(false);
    }
    let (Some(source_path), Some(first_line)) = (
        candidate.item.source_path.as_deref(),
        candidate.item.line_start,
    ) else {
        return Ok(false);
    };
    let Some((_, source)) = candidate.item.text.split_once('\n') else {
        return Ok(false);
    };
    let excerpts = if source.len() <= 512 || source.len() > *remaining_scan_bytes {
        Vec::new()
    } else {
        *remaining_scan_bytes = remaining_scan_bytes.saturating_sub(source.len());
        super::excerpts::query_windows(source, first_line, &pack.query, remaining_excerpt_bytes)
    };
    for (excerpt, line_start, line_end) in &excerpts {
        let mut item = candidate.item.clone();
        item.text = format!("{source_path}:{line_start}-{line_end}\n{excerpt}");
        item.line_start = Some(*line_start);
        item.line_end = Some(*line_end);
        if try_item(pack, item, omitted, counter)? {
            return Ok(true);
        }
    }
    admit_after_compacting_source(
        pack,
        candidate,
        omitted,
        counter,
        &excerpts,
        remaining_scan_bytes,
        remaining_excerpt_bytes,
    )
}

/// Preserve candidate order while compacting the lowest-priority admitted
/// source spans only as much as needed for the next selected candidate.
fn admit_after_compacting_source(
    pack: &mut ContextPack,
    candidate: &Candidate,
    omitted: OmittedContextSummary,
    counter: &impl ContextTokenCounter,
    candidate_excerpts: &[(String, u32, u32)],
    remaining_scan_bytes: &mut usize,
    remaining_excerpt_bytes: &mut usize,
) -> Result<bool, QueryError> {
    let original_items = pack.items.clone();
    let original_token_count = pack.token_count;
    let original_omitted = pack.omitted;
    let mut best_excerpt_pack = None;
    let mut complete_retries_remaining = COMPLETE_CANDIDATE_RETRY_LIMIT;
    'existing_items: for item_index in (0..pack.items.len()).rev() {
        let Some(existing) = pack.items.get(item_index).cloned() else {
            continue;
        };
        if existing.kind != ContextItemKind::SourceEvidence {
            continue;
        }
        let (Some(source_path), Some(first_line)) =
            (existing.source_path.as_deref(), existing.line_start)
        else {
            continue;
        };
        let Some((_, source_text)) = existing.text.split_once('\n') else {
            continue;
        };
        let mut source_lines = source_text.lines().collect::<Vec<_>>();
        if source_lines.first().copied() == Some("[…]") {
            source_lines.remove(0);
        }
        if source_lines.last().copied() == Some("[…]") {
            source_lines.pop();
        }
        let compact_source = source_lines.join("\n");
        if compact_source.lines().count() <= 1 || compact_source.len() > *remaining_scan_bytes {
            continue;
        }
        *remaining_scan_bytes = remaining_scan_bytes.saturating_sub(compact_source.len());
        let existing_excerpts = super::excerpts::query_windows(
            &compact_source,
            first_line,
            &pack.query,
            remaining_excerpt_bytes,
        );
        for (excerpt, line_start, line_end) in existing_excerpts {
            let mut replacement = existing.clone();
            replacement.text = format!("{source_path}:{line_start}-{line_end}\n{excerpt}");
            replacement.line_start = Some(line_start);
            replacement.line_end = Some(line_end);
            let Some(slot) = pack.items.get_mut(item_index) else {
                continue;
            };
            *slot = replacement;
            if let Err(error) = finalize_count(pack, counter) {
                pack.items = original_items;
                pack.token_count = original_token_count;
                pack.omitted = original_omitted;
                return Err(error);
            }
            if complete_retries_remaining == 0 {
                break 'existing_items;
            }
            complete_retries_remaining = complete_retries_remaining.saturating_sub(1);
            match try_item(pack, candidate.item.clone(), omitted, counter) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(error) => {
                    pack.items = original_items;
                    pack.token_count = original_token_count;
                    pack.omitted = original_omitted;
                    return Err(error);
                }
            }
            if best_excerpt_pack.is_none() {
                for (candidate_excerpt, candidate_start, candidate_end) in candidate_excerpts {
                    let Some(candidate_path) = candidate.item.source_path.as_deref() else {
                        continue;
                    };
                    let mut item = candidate.item.clone();
                    item.text = format!(
                        "{candidate_path}:{candidate_start}-{candidate_end}\n{candidate_excerpt}"
                    );
                    item.line_start = Some(*candidate_start);
                    item.line_end = Some(*candidate_end);
                    let previous_len = pack.items.len();
                    let previous_count = pack.token_count;
                    let previous_omitted = pack.omitted;
                    match try_item(pack, item, omitted, counter) {
                        Ok(true) => {
                            best_excerpt_pack = Some(pack.clone());
                            pack.items.truncate(previous_len);
                            pack.token_count = previous_count;
                            pack.omitted = previous_omitted;
                            break;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            pack.items = original_items;
                            pack.token_count = original_token_count;
                            pack.omitted = original_omitted;
                            return Err(error);
                        }
                    }
                }
            }
            if complete_retries_remaining == 0 {
                break 'existing_items;
            }
        }
    }
    if let Some(best_excerpt_pack) = best_excerpt_pack {
        *pack = best_excerpt_pack;
        return Ok(true);
    }
    pack.items = original_items;
    pack.token_count = original_token_count;
    pack.omitted = original_omitted;
    Ok(false)
}
