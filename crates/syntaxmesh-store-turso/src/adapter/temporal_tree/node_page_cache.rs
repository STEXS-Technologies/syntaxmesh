use syntaxmesh_core::StableId;
use syntaxmesh_store::{PersistentFactNode, PersistentFactTreeCache, PersistentFactTreeError};

const MAX_PAGES: usize = 128;
const MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub(super) struct NodePageCache {
    pages: PersistentFactTreeCache,
    retained_pages: usize,
    payload_bytes: usize,
}

impl NodePageCache {
    pub(super) const fn pages(&self) -> &PersistentFactTreeCache {
        &self.pages
    }

    pub(super) fn insert_loaded(
        &mut self,
        id: StableId,
        node: PersistentFactNode,
    ) -> Result<(), PersistentFactTreeError> {
        let charged = node
            .value
            .len()
            .saturating_add(std::mem::size_of::<PersistentFactNode>());
        if self.retained_pages >= MAX_PAGES
            || self.payload_bytes.saturating_add(charged) > MAX_PAYLOAD_BYTES
        {
            *self = Self::default();
        }
        self.pages.insert_loaded(id, node)?;
        self.retained_pages = self.retained_pages.saturating_add(1);
        self.payload_bytes = self.payload_bytes.saturating_add(charged);
        Ok(())
    }

    pub(super) fn yielded(&mut self) {
        if self.payload_bytes > MAX_PAYLOAD_BYTES {
            *self = Self::default();
        }
    }
}

#[cfg(test)]
mod tests;
