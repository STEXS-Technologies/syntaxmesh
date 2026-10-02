use syntaxmesh_api_model::{ContextItem, ContextPack, OmittedContextSummary};

use super::{ContextTokenCounter, finalize_count};
use crate::QueryError;

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
