use syntaxmesh_core::{GenerationId, Node};

use crate::{GraphStore, MAX_HISTORICAL_NODE_PAGE_SIZE, StoreError};

pub(crate) fn visit<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
    max_nodes: usize,
    visitor: &mut dyn FnMut(Node) -> Result<(), StoreError>,
) -> Result<usize, StoreError> {
    if max_nodes == 0 {
        return Err(StoreError::InvalidPageLimit);
    }
    let mut after = None;
    let mut visited = 0_usize;
    loop {
        let limit = max_nodes
            .saturating_sub(visited)
            .min(MAX_HISTORICAL_NODE_PAGE_SIZE);
        if limit == 0 {
            return Err(StoreError::InvalidPageLimit);
        }
        let page = store.historical_nodes_page(generation, after, limit)?;
        if page.items.len() > limit || (page.has_more && page.items.is_empty()) {
            return Err(StoreError::Integrity(
                "invalid historical node scan page".to_owned(),
            ));
        }
        for node in page.items {
            if after.is_some_and(|previous| node.id <= previous) {
                return Err(StoreError::Integrity(
                    "nonadvancing historical node scan".to_owned(),
                ));
            }
            after = Some(node.id);
            visitor(node)?;
            visited = visited.saturating_add(1);
        }
        if !page.has_more {
            return Ok(visited);
        }
    }
}
