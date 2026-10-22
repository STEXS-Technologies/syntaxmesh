use crate::Result;
use bincode::Options;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use syntaxmesh_core::StableId;

mod migration;
pub(crate) use migration::backfill;

#[derive(Clone, Copy, Serialize, Deserialize)]
enum Kind {
    Leaf,
    Branch {
        bit: u16,
        left: StableId,
        right: StableId,
    },
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct Page {
    version: u16,
    key: StableId,
    count: u64,
    kind: Kind,
}

pub(super) struct ActiveTree<'connection> {
    connection: &'connection Connection,
    remaining: usize,
}

impl<'connection> ActiveTree<'connection> {
    pub(super) fn new(connection: &'connection Connection, budget: usize) -> Result<Self> {
        if budget == 0 {
            return Err("positive active-tree page budget required".into());
        }
        Ok(Self {
            connection,
            remaining: budget,
        })
    }

    fn charge(&mut self) -> Result<()> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or("active-tree page budget exhausted")?;
        Ok(())
    }

    fn load(&mut self, identity: StableId) -> Result<Page> {
        self.charge()?;
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM inference_active_pages WHERE page = ?1")?;
        let mut rows = statement.query([identity.to_hex()])?;
        let row = rows.next()?.ok_or("active-tree page missing")?;
        let payload = row.get_ref(0)?.as_blob()?;
        if payload.len() > 112
            || StableId::derive("inference-active-page-v1", &[payload]) != identity
        {
            return Err("active-tree page size or identity mismatch".into());
        }
        let page: Page = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(u64::try_from(payload.len())?)
            .reject_trailing_bytes()
            .deserialize(payload)?;
        if page.version != 1 {
            return Err("active-tree page version unsupported".into());
        }
        if matches!(page.kind, Kind::Leaf) && page.count != 1 {
            return Err("active-tree leaf count inconsistent".into());
        }
        if let Kind::Branch { bit, left, right } = page.kind
            && (bit >= 256 || left == right || page.count < 2 || key_bit(page.key, bit)?)
        {
            return Err("active-tree branch malformed".into());
        }
        Ok(page)
    }

    fn put(&mut self, page: Page) -> Result<StableId> {
        self.charge()?;
        let payload = bincode::serialize(&page)?;
        let identity = StableId::derive("inference-active-page-v1", &[&payload]);
        self.connection.execute(
            "INSERT INTO inference_active_pages VALUES (?1, ?2) ON CONFLICT(page) DO NOTHING",
            params![identity.to_hex(), payload],
        )?;
        let equal: bool = self.connection.query_row(
            "SELECT payload = ?2 FROM inference_active_pages WHERE page = ?1",
            params![identity.to_hex(), payload],
            |row| row.get(0),
        )?;
        if !equal {
            return Err("active-tree immutable page collision".into());
        }
        Ok(identity)
    }

    fn children(&mut self, page: Page) -> Result<(StableId, Page, StableId, Page, u16)> {
        let Kind::Branch { bit, left, right } = page.kind else {
            return Err("active-tree leaf has no children".into());
        };
        let left_page = self.load(left)?;
        let right_page = self.load(right)?;
        if left_page.key != page.key
            || left_page.count.checked_add(right_page.count) != Some(page.count)
            || common_prefix(left_page.key, right_page.key)? != bit
            || key_bit(left_page.key, bit)?
            || !key_bit(right_page.key, bit)?
        {
            return Err("active-tree child prefixes inconsistent".into());
        }
        for child in [left_page, right_page] {
            if let Kind::Branch { bit: child_bit, .. } = child.kind
                && child_bit <= bit
            {
                return Err("active-tree pivot order or cycle invalid".into());
            }
        }
        Ok((left, left_page, right, right_page, bit))
    }

    fn branch(&mut self, bit: u16, left: StableId, right: StableId) -> Result<StableId> {
        let left_page = self.load(left)?;
        let right_page = self.load(right)?;
        let page = Page {
            version: 1,
            key: left_page.key,
            count: left_page
                .count
                .checked_add(right_page.count)
                .ok_or("active-tree cardinality overflow")?,
            kind: Kind::Branch { bit, left, right },
        };
        if common_prefix(left_page.key, right_page.key)? != bit
            || key_bit(left_page.key, bit)?
            || !key_bit(right_page.key, bit)?
        {
            return Err("active-tree branch construction inconsistent".into());
        }
        self.put(page)
    }

    pub(super) fn insert(&mut self, root: Option<StableId>, key: StableId) -> Result<StableId> {
        let Some(identity) = root else {
            return self.put(Page {
                version: 1,
                key,
                count: 1,
                kind: Kind::Leaf,
            });
        };
        let page = self.load(identity)?;
        let prefix = common_prefix(page.key, key)?;
        let pivot = match page.kind {
            Kind::Leaf => 256,
            Kind::Branch { bit, .. } => bit,
        };
        if prefix < pivot {
            let leaf = self.put(Page {
                version: 1,
                key,
                count: 1,
                kind: Kind::Leaf,
            })?;
            return if key_bit(key, prefix)? {
                self.branch(prefix, identity, leaf)
            } else {
                self.branch(prefix, leaf, identity)
            };
        }
        if matches!(page.kind, Kind::Leaf) {
            return Err("active-tree duplicate key".into());
        }
        let (left, _left_page, right, _right_page, bit) = self.children(page)?;
        if key_bit(key, bit)? {
            let changed = self.insert(Some(right), key)?;
            self.branch(bit, left, changed)
        } else {
            let changed = self.insert(Some(left), key)?;
            self.branch(bit, changed, right)
        }
    }

    pub(super) fn remove(
        &mut self,
        root: Option<StableId>,
        key: StableId,
    ) -> Result<Option<StableId>> {
        let identity = root.ok_or("active-tree removal from empty set")?;
        let page = self.load(identity)?;
        if matches!(page.kind, Kind::Leaf) {
            return if page.key == key {
                Ok(None)
            } else {
                Err("active-tree removal key missing".into())
            };
        }
        let (left, _left_page, right, _right_page, bit) = self.children(page)?;
        if common_prefix(page.key, key)? < bit {
            return Err("active-tree removal prefix missing".into());
        }
        if key_bit(key, bit)? {
            let changed = self.remove(Some(right), key)?;
            changed.map_or_else(
                || Ok(Some(left)),
                |replacement| self.branch(bit, left, replacement).map(Some),
            )
        } else {
            let changed = self.remove(Some(left), key)?;
            changed.map_or_else(
                || Ok(Some(right)),
                |replacement| self.branch(bit, replacement, right).map(Some),
            )
        }
    }

    pub(super) fn keys(&mut self, root: Option<StableId>, limit: usize) -> Result<Vec<StableId>> {
        let mut keys = Vec::new();
        let mut visited = BTreeSet::new();
        if let Some(identity) = root {
            self.enumerate(identity, limit, &mut visited, &mut keys)?;
        }
        Ok(keys)
    }

    pub(super) fn count(&mut self, root: Option<StableId>) -> Result<usize> {
        let Some(identity) = root else {
            return Ok(0);
        };
        let page = self.load(identity)?;
        if matches!(page.kind, Kind::Branch { .. }) {
            self.children(page)?;
        }
        Ok(usize::try_from(page.count)?)
    }

    pub(super) fn membership(
        &mut self,
        root: Option<StableId>,
        key: StableId,
        expected: usize,
    ) -> Result<bool> {
        let Some(identity) = root else {
            return if expected == 0 {
                Ok(false)
            } else {
                Err("active-tree empty root count inconsistent".into())
            };
        };
        let mut page = self.load(identity)?;
        if usize::try_from(page.count)? != expected {
            return Err("active-tree root count inconsistent".into());
        }
        loop {
            match page.kind {
                Kind::Leaf => return Ok(page.key == key),
                Kind::Branch { .. } => {
                    let (_, left, _, right, bit) = self.children(page)?;
                    if common_prefix(page.key, key)? < bit {
                        return Ok(false);
                    }
                    page = if key_bit(key, bit)? { right } else { left };
                }
            }
        }
    }

    pub(super) fn page(
        &mut self,
        root: Option<StableId>,
        after: Option<StableId>,
        limit: usize,
        expected: usize,
    ) -> Result<Vec<StableId>> {
        let Some(identity) = root else {
            return if expected == 0 {
                Ok(Vec::new())
            } else {
                Err("active-tree empty root count inconsistent".into())
            };
        };
        let page = self.load(identity)?;
        if usize::try_from(page.count)? != expected {
            return Err("active-tree root count inconsistent".into());
        }
        let mut keys = Vec::new();
        self.page_from(page, after, limit, &mut keys)?;
        Ok(keys)
    }

    fn page_from(
        &mut self,
        page: Page,
        after: Option<StableId>,
        limit: usize,
        keys: &mut Vec<StableId>,
    ) -> Result<()> {
        if keys.len() >= limit {
            return Ok(());
        }
        match page.kind {
            Kind::Leaf => {
                if after.is_none_or(|key| page.key > key) {
                    keys.push(page.key);
                }
            }
            Kind::Branch { .. } => {
                let (_, left, _, right, _) = self.children(page)?;
                // The right child's representative is its minimum key. All left
                // keys are smaller; skip that entire subtree for later cursors.
                if after.is_none_or(|key| key < right.key) {
                    self.page_from(left, after, limit, keys)?;
                }
                self.page_from(right, after, limit, keys)?;
            }
        }
        Ok(())
    }

    fn enumerate(
        &mut self,
        identity: StableId,
        limit: usize,
        visited: &mut BTreeSet<StableId>,
        keys: &mut Vec<StableId>,
    ) -> Result<()> {
        if !visited.insert(identity) {
            return Err("active-tree repeated page or cycle".into());
        }
        let page = self.load(identity)?;
        match page.kind {
            Kind::Leaf => {
                if keys.len() >= limit {
                    return Err("active-tree leaf budget exhausted".into());
                }
                keys.push(page.key);
            }
            Kind::Branch { .. } => {
                let (left, _left_page, right, _right_page, _bit) = self.children(page)?;
                self.enumerate(left, limit, visited, keys)?;
                self.enumerate(right, limit, visited, keys)?;
            }
        }
        Ok(())
    }
}

fn common_prefix(left: StableId, right: StableId) -> Result<u16> {
    for (index, (left_byte, right_byte)) in left.0.iter().zip(right.0).enumerate() {
        let different = *left_byte ^ right_byte;
        if different != 0 {
            return u16::try_from(index)?
                .checked_mul(8)
                .and_then(|prefix| {
                    prefix.checked_add(u16::try_from(different.leading_zeros()).ok()?)
                })
                .ok_or_else(|| "active-tree prefix overflow".into());
        }
    }
    Ok(256)
}

fn key_bit(key: StableId, bit: u16) -> Result<bool> {
    if bit >= 256 {
        return Err("active-tree key bit out of bounds".into());
    }
    let byte = key
        .0
        .get(usize::from(bit / 8))
        .ok_or("active-tree key byte missing")?;
    let shift = u32::from(
        7_u16
            .checked_sub(bit % 8)
            .ok_or("active-tree bit shift invalid")?,
    );
    Ok(byte
        .checked_shr(shift)
        .ok_or("active-tree key shift overflow")?
        & 1
        != 0)
}

pub(super) fn root(
    connection: &Connection,
    stream: StableId,
    generation: syntaxmesh_core::GenerationId,
) -> Result<Option<StableId>> {
    let text: Option<String> = connection.query_row(
        "SELECT root FROM inference_stream_roots WHERE stream = ?1 AND generation = ?2",
        params![stream.to_hex(), generation.0.to_hex()],
        |row| row.get(0),
    )?;
    text.as_deref()
        .map(|value| super::checkpoint::generation(value).map(|typed| typed.0))
        .transpose()
}

#[cfg(test)]
mod tests;
