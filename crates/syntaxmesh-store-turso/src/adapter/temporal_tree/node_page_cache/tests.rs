use super::{MAX_PAGES, MAX_PAYLOAD_BYTES, NodePageCache};
use syntaxmesh_core::StableId;
use syntaxmesh_store::{
    PersistentFactKey, PersistentFactMutation, PersistentFactNode, PersistentFactTree,
    PersistentFactTreeCache, PersistentFactTreeRangeWalker,
};

fn leaf(
    seed: usize,
    bytes: usize,
) -> Result<(StableId, PersistentFactNode), Box<dyn std::error::Error>> {
    let mutation = PersistentFactMutation::Upsert {
        key: PersistentFactKey {
            fact_kind: 1,
            fact_id: StableId::derive("node-cache-test", &[&seed.to_le_bytes()]),
        },
        value: vec![0; bytes],
    };
    let mut cache = PersistentFactTreeCache::default();
    PersistentFactTree::apply(None, &[mutation], &mut cache).map_err(|error| tree_error(&error))?;
    cache
        .take_dirty()
        .into_iter()
        .next()
        .ok_or_else(|| std::io::Error::other("leaf fixture omitted its page").into())
}

fn available(cache: &NodePageCache, id: StableId) -> bool {
    PersistentFactTreeRangeWalker::new(Some(id), None)
        .next(cache.pages())
        .is_ok_and(|page| page.is_some())
}

fn tree_error(error: &syntaxmesh_store::PersistentFactTreeError) -> std::io::Error {
    std::io::Error::other(format!("{error:?}"))
}

#[test]
fn entry_bound_retains_then_evicts_validated_pages() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = NodePageCache::default();
    let (first_id, first) = leaf(0, 1)?;
    cache
        .insert_loaded(first_id, first)
        .map_err(|error| tree_error(&error))?;
    for seed in 1..MAX_PAGES {
        let (id, page) = leaf(seed, 1)?;
        cache
            .insert_loaded(id, page)
            .map_err(|error| tree_error(&error))?;
        cache.yielded();
    }
    if cache.retained_pages != MAX_PAGES || !available(&cache, first_id) {
        return Err(std::io::Error::other("entry bound discarded its working set early").into());
    }
    let (next_id, next) = leaf(MAX_PAGES, 1)?;
    cache
        .insert_loaded(next_id, next)
        .map_err(|error| tree_error(&error))?;
    if cache.retained_pages != 1 || available(&cache, first_id) || !available(&cache, next_id) {
        return Err(std::io::Error::other("entry bound did not evict before insertion").into());
    }
    Ok(())
}

#[test]
fn byte_bound_and_oversized_yield_keep_retention_bounded() -> Result<(), Box<dyn std::error::Error>>
{
    let mut cache = NodePageCache::default();
    let (first_id, first) = leaf(0, MAX_PAYLOAD_BYTES / 2)?;
    cache
        .insert_loaded(first_id, first)
        .map_err(|error| tree_error(&error))?;
    let (second_id, second) = leaf(1, MAX_PAYLOAD_BYTES / 2)?;
    cache
        .insert_loaded(second_id, second)
        .map_err(|error| tree_error(&error))?;
    if cache.payload_bytes > MAX_PAYLOAD_BYTES
        || available(&cache, first_id)
        || !available(&cache, second_id)
    {
        return Err(std::io::Error::other("byte bound failed to evict previous payload").into());
    }
    let (large_id, large) = leaf(2, MAX_PAYLOAD_BYTES.saturating_add(1))?;
    cache
        .insert_loaded(large_id, large)
        .map_err(|error| tree_error(&error))?;
    if cache.retained_pages != 1 || !available(&cache, large_id) || available(&cache, second_id) {
        return Err(
            std::io::Error::other("oversized page was not isolated for walker progress").into(),
        );
    }
    cache.yielded();
    if cache.retained_pages != 0 || cache.payload_bytes != 0 || available(&cache, large_id) {
        return Err(std::io::Error::other("oversized page survived its yield").into());
    }
    Ok(())
}

#[test]
fn corrupted_page_is_not_retained() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = NodePageCache::default();
    let (id, mut page) = leaf(0, 1)?;
    page.value.push(1);
    if cache.insert_loaded(id, page).is_ok() || cache.retained_pages != 0 || available(&cache, id) {
        return Err(std::io::Error::other("corrupt page entered the working set").into());
    }
    Ok(())
}
