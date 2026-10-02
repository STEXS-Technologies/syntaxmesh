//! Deterministic structurally shared fact map used by durable generation roots.

#[cfg(feature = "benchmark-instrumentation")]
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use imbl::OrdMap;

use serde::{Deserialize, Serialize};
use syntaxmesh_core::StableId;

const STORAGE_ENVELOPE_MAGIC: &[u8; 4] = b"SMVE";
const STORAGE_ENVELOPE_HEADER_BYTES: usize = 36;
const APPLY_CACHE_PRUNE_INTERVAL: usize = 4_096;

/// Bind a generation identity to the canonical semantic root of its fact tree.
///
/// The tree's content address is derived from fact keys, canonical JSON values,
/// and content-addressed child roots; it does not include page serialization or
/// database-local identifiers.
#[must_use]
pub fn generation_root_v2(generation: StableId, fact_root: Option<StableId>) -> [u8; 32] {
    let root = fact_root.map_or_else(
        || vec![0],
        |root| {
            let mut bytes = Vec::with_capacity(33);
            bytes.push(1);
            bytes.extend_from_slice(&root.0);
            bytes
        },
    );
    StableId::derive(
        "syntaxmesh-generation-state-root-v2",
        &[&generation.0, &root],
    )
    .0
}

/// Stable fact identity used as the persistent tree's ordered key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PersistentFactKey {
    /// Canonical fact family discriminator.
    pub fact_kind: u8,
    /// Stable identity within the fact family.
    pub fact_id: StableId,
}

/// One immutable search-tree page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistentFactNode {
    /// The key represented by this page.
    pub key: PersistentFactKey,
    /// Fact payload, optionally wrapped in a checksummed storage envelope.
    pub value: Vec<u8>,
    value_hash: StableId,
    /// Deterministic heap priority derived from the key.
    pub priority: StableId,
    /// Content-addressed left child page.
    pub left: Option<StableId>,
    /// Content-addressed right child page.
    pub right: Option<StableId>,
}

struct PendingFactNode {
    node: PersistentFactNode,
    left: Option<usize>,
    right: Option<usize>,
}

impl PersistentFactNode {
    /// Return this immutable page's content address.
    #[must_use]
    pub fn id(&self) -> StableId {
        node_id(self.key, self.value_hash, self.left, self.right)
    }

    fn new(
        key: PersistentFactKey,
        value: Vec<u8>,
        left: Option<StableId>,
        right: Option<StableId>,
    ) -> Self {
        let value_hash = StableId::derive("persistent-fact-value-v1", &[&value]);
        Self {
            key,
            value,
            value_hash,
            priority: priority_for(key),
            left,
            right,
        }
    }

    fn new_with_commitment(
        key: PersistentFactKey,
        value: &[u8],
        value_hash: StableId,
        left: Option<StableId>,
        right: Option<StableId>,
    ) -> Self {
        let storage_hash = StableId::derive("persistent-fact-storage-value-v1", &[value]);
        let mut enveloped_value = Vec::new();
        enveloped_value.extend_from_slice(STORAGE_ENVELOPE_MAGIC);
        enveloped_value.extend_from_slice(&storage_hash.0);
        enveloped_value.extend_from_slice(value);
        Self {
            key,
            value: enveloped_value,
            value_hash,
            priority: priority_for(key),
            left,
            right,
        }
    }

    /// Return the fact payload without the optional physical-storage envelope.
    #[must_use]
    pub fn value_payload(&self) -> &[u8] {
        self.storage_envelope()
            .map_or(self.value.as_slice(), |(payload, _)| payload)
    }

    fn storage_envelope(&self) -> Option<(&[u8], StableId)> {
        if self.value.get(..STORAGE_ENVELOPE_MAGIC.len())? != STORAGE_ENVELOPE_MAGIC {
            return None;
        }
        let checksum_bytes = self
            .value
            .get(STORAGE_ENVELOPE_MAGIC.len()..STORAGE_ENVELOPE_HEADER_BYTES)?;
        let checksum = StableId(checksum_bytes.try_into().ok()?);
        Some((self.value.get(STORAGE_ENVELOPE_HEADER_BYTES..)?, checksum))
    }
}

/// A missing or invalid page encountered while reading a persistent root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistentFactTreeError {
    /// The root references a page absent from the caller's loaded page cache.
    MissingPage(StableId),
    /// A persisted page's key-derived priority or content address is invalid.
    CorruptPage(StableId),
}

/// One fact change to apply when deriving a generation root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistentFactMutation {
    /// Add or replace a payload whose bytes are also its semantic commitment.
    Upsert {
        /// Stable fact identity.
        key: PersistentFactKey,
        /// Encoded payload whose bytes are also its semantic value commitment.
        value: Vec<u8>,
    },
    /// Add or replace a compact payload with an independent canonical-value commitment.
    UpsertWithCommitment {
        /// Stable fact identity.
        key: PersistentFactKey,
        /// Compact encoded payload to store in the tree page.
        value: Vec<u8>,
        /// Hash of the canonical semantic payload, independent of this encoding.
        value_commitment: StableId,
    },
    /// Remove the fact identity from the resulting generation.
    Remove {
        /// Stable fact identity.
        key: PersistentFactKey,
    },
}

impl PersistentFactMutation {
    /// Return the stable key updated by this mutation.
    #[must_use]
    pub const fn key(&self) -> PersistentFactKey {
        match self {
            Self::Upsert { key, .. }
            | Self::UpsertWithCommitment { key, .. }
            | Self::Remove { key } => *key,
        }
    }
}

/// Per-operation cache of immutable tree pages and newly created pages.
#[derive(Debug, Default, Clone)]
pub struct PersistentFactTreeCache {
    pages: OrdMap<StableId, PersistentFactNode>,
    dirty: BTreeMap<StableId, PersistentFactNode>,
    #[cfg(feature = "benchmark-instrumentation")]
    loaded_page_count: usize,
    #[cfg(feature = "benchmark-instrumentation")]
    page_lookup_count: Cell<usize>,
}

impl PersistentFactTreeCache {
    /// Load and validate a page obtained from durable storage.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::CorruptPage`] if content addressing,
    /// the optional storage envelope, or deterministic priority validation fails.
    pub fn insert_loaded(
        &mut self,
        id: StableId,
        node: PersistentFactNode,
    ) -> Result<(), PersistentFactTreeError> {
        let value_is_valid = node.storage_envelope().map_or_else(
            || node.value_hash == StableId::derive("persistent-fact-value-v1", &[&node.value]),
            |(payload, checksum)| {
                checksum == StableId::derive("persistent-fact-storage-value-v1", &[payload])
            },
        );
        if node.id() != id || node.priority != priority_for(node.key) || !value_is_valid {
            return Err(PersistentFactTreeError::CorruptPage(id));
        }
        self.pages.insert(id, node);
        #[cfg(feature = "benchmark-instrumentation")]
        {
            self.loaded_page_count = self.loaded_page_count.saturating_add(1);
        }
        Ok(())
    }

    /// Number of durable tree pages loaded into this cache during the current
    /// operation. Available only to instrumented temporal benchmarks.
    #[cfg(feature = "benchmark-instrumentation")]
    #[must_use]
    pub const fn loaded_page_count(&self) -> usize {
        self.loaded_page_count
    }

    /// Number of persistent-tree page-cache lookups, including misses/retries.
    /// Available only to instrumented temporal benchmarks.
    #[cfg(feature = "benchmark-instrumentation")]
    #[must_use]
    pub const fn page_lookup_count(&self) -> usize {
        self.page_lookup_count.get()
    }

    /// Drain newly created pages so the storage adapter can persist them.
    #[must_use]
    pub fn take_dirty(&mut self) -> Vec<(StableId, PersistentFactNode)> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    pub(crate) fn discard_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Drain only newly created pages reachable from `root`, dropping
    /// intermediate copy-on-write pages superseded during a batch mutation.
    ///
    /// Existing durable pages are traversal boundaries: their descendants are
    /// already persisted and need not be loaded to identify reachable dirty
    /// pages.
    #[must_use]
    pub fn take_reachable_dirty(
        &mut self,
        root: Option<StableId>,
    ) -> Vec<(StableId, PersistentFactNode)> {
        self.retain_reachable_dirty_roots(root);
        self.take_dirty()
    }

    /// Drain new pages reachable from multiple roots when some index roots are
    /// referenced by payload values instead of structural child links.
    #[must_use]
    pub fn take_reachable_dirty_roots(
        &mut self,
        roots: impl IntoIterator<Item = StableId>,
    ) -> Vec<(StableId, PersistentFactNode)> {
        self.retain_reachable_dirty_roots(roots);
        self.take_dirty()
    }

    fn retain_reachable_dirty_roots(&mut self, roots: impl IntoIterator<Item = StableId>) {
        let mut reachable = BTreeSet::new();
        let mut pending = roots.into_iter().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            if let Some(node) = self.dirty.get(&id) {
                pending.extend(node.left);
                pending.extend(node.right);
            } else {
                reachable.remove(&id);
            }
        }
        let unreachable_dirty = self
            .dirty
            .keys()
            .filter(|id| !reachable.contains(id))
            .copied()
            .collect::<Vec<_>>();
        for id in unreachable_dirty {
            self.pages.remove(&id);
        }
        self.dirty.retain(|id, _node| reachable.contains(id));
    }

    fn page(&self, id: StableId) -> Result<&PersistentFactNode, PersistentFactTreeError> {
        #[cfg(feature = "benchmark-instrumentation")]
        self.page_lookup_count
            .set(self.page_lookup_count.get().saturating_add(1));
        self.pages
            .get(&id)
            .ok_or(PersistentFactTreeError::MissingPage(id))
    }

    fn store_new(&mut self, node: PersistentFactNode) -> StableId {
        let id = node.id();
        self.pages.insert(id, node.clone());
        self.dirty.insert(id, node);
        id
    }
}

/// Resumable ordered mutation cursor for a persistent fact tree.
///
/// A storage adapter can advance until a durable page is missing, load that
/// page into the cache, and resume without replaying completed mutations.
pub struct PersistentFactTreeApply<'mutations> {
    mutations: &'mutations [PersistentFactMutation],
    next_mutation: usize,
    root: Option<StableId>,
}

impl<'mutations> PersistentFactTreeApply<'mutations> {
    /// Start applying `mutations` to `root`.
    #[must_use]
    pub const fn new(
        root: Option<StableId>,
        mutations: &'mutations [PersistentFactMutation],
    ) -> Self {
        Self {
            mutations,
            next_mutation: 0,
            root,
        }
    }

    /// Apply at most one mutation, preserving progress if a page is missing.
    ///
    /// Returns `Ok(true)` when one mutation was applied and `Ok(false)` when
    /// the cursor is complete. A missing-page error leaves the cursor unchanged
    /// so the adapter can load the page and retry this mutation.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] if storage must load a
    /// page, or a tree error if the cached page is corrupt.
    pub fn advance(
        &mut self,
        cache: &mut PersistentFactTreeCache,
    ) -> Result<bool, PersistentFactTreeError> {
        let Some(mutation) = self.mutations.get(self.next_mutation) else {
            return Ok(false);
        };
        let root = PersistentFactTree::apply_one(self.root, mutation, cache)?;
        self.root = root;
        self.next_mutation = self.next_mutation.saturating_add(1);
        if self
            .next_mutation
            .is_multiple_of(APPLY_CACHE_PRUNE_INTERVAL)
        {
            cache.retain_reachable_dirty_roots(self.root);
        }
        Ok(true)
    }

    /// Return the current working root, including mutations already applied.
    #[must_use]
    pub const fn root(&self) -> Option<StableId> {
        self.root
    }

    /// Return whether every supplied mutation has been applied.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.next_mutation >= self.mutations.len()
    }
}

/// Incremental depth-first traversal over one immutable root.
#[derive(Debug)]
pub struct PersistentFactTreeWalker {
    pending: Vec<StableId>,
}

impl PersistentFactTreeWalker {
    /// Start walking the supplied root, or an empty tree when absent.
    #[must_use]
    pub fn new(root: Option<StableId>) -> Self {
        Self {
            pending: root.into_iter().collect(),
        }
    }

    /// Return the next page, preserving traversal state when a page must be
    /// loaded into the cache.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the next durable
    /// page has not yet been loaded.
    pub fn next(
        &mut self,
        cache: &PersistentFactTreeCache,
    ) -> Result<Option<(StableId, PersistentFactNode)>, PersistentFactTreeError> {
        let Some(id) = self.pending.pop() else {
            return Ok(None);
        };
        let node = match cache.page(id) {
            Ok(node) => node.clone(),
            Err(error) => {
                self.pending.push(id);
                return Err(error);
            }
        };
        if let Some(right) = node.right {
            self.pending.push(right);
        }
        if let Some(left) = node.left {
            self.pending.push(left);
        }
        Ok(Some((id, node)))
    }
}

/// Resumable point lookup that preserves its search cursor on a cache miss.
#[derive(Debug)]
pub struct PersistentFactTreeLookup {
    key: PersistentFactKey,
    cursor: Option<StableId>,
    result: Option<Option<Vec<u8>>>,
}

impl PersistentFactTreeLookup {
    /// Begin looking up one key in an immutable root.
    #[must_use]
    pub const fn new(root: Option<StableId>, key: PersistentFactKey) -> Self {
        Self {
            key,
            cursor: root,
            result: None,
        }
    }

    /// Continue the lookup. A missing page leaves the cursor at that page, so
    /// loading it and retrying does not revisit the already searched prefix.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the caller must
    /// load the current search page before resuming.
    pub fn resolve(
        &mut self,
        cache: &PersistentFactTreeCache,
    ) -> Result<Option<Vec<u8>>, PersistentFactTreeError> {
        if let Some(result) = &self.result {
            return Ok(result.clone());
        }
        while let Some(id) = self.cursor {
            let node = cache.page(id)?;
            match self.key.cmp(&node.key) {
                Ordering::Less => self.cursor = node.left,
                Ordering::Greater => self.cursor = node.right,
                Ordering::Equal => {
                    let value = node.value_payload().to_vec();
                    self.cursor = None;
                    self.result = Some(Some(value.clone()));
                    return Ok(Some(value));
                }
            }
        }
        self.result = Some(None);
        Ok(None)
    }
}

/// Resumable ascending range cursor for a persistent fact-tree root.
#[derive(Debug)]
pub struct PersistentFactTreeRangeWalker {
    after: Option<PersistentFactKey>,
    search_cursor: Option<StableId>,
    candidates: Vec<StableId>,
    pending: Option<PersistentFactNode>,
    right_cursor: Option<StableId>,
}

impl PersistentFactTreeRangeWalker {
    /// Start yielding keys strictly greater than `after` in ascending order.
    #[must_use]
    pub const fn new(root: Option<StableId>, after: Option<PersistentFactKey>) -> Self {
        Self {
            after,
            search_cursor: root,
            candidates: Vec::new(),
            pending: None,
            right_cursor: None,
        }
    }

    /// Return the next ordered page, preserving both search and successor
    /// paths when durable pages are missing.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the caller must
    /// load a page before continuing the same traversal.
    pub fn next(
        &mut self,
        cache: &PersistentFactTreeCache,
    ) -> Result<Option<PersistentFactNode>, PersistentFactTreeError> {
        while let Some(id) = self.search_cursor {
            let node = cache.page(id)?;
            if self.after.is_none_or(|key| node.key > key) {
                self.candidates.push(id);
                self.search_cursor = node.left;
            } else {
                self.search_cursor = node.right;
            }
        }

        if self.pending.is_none() {
            let Some(id) = self.candidates.pop() else {
                return Ok(None);
            };
            match cache.page(id) {
                Ok(node) => {
                    self.right_cursor = node.right;
                    self.pending = Some(node.clone());
                }
                Err(error) => {
                    self.candidates.push(id);
                    return Err(error);
                }
            }
        }

        while let Some(id) = self.right_cursor {
            let node = cache.page(id)?;
            self.candidates.push(id);
            self.right_cursor = node.left;
        }
        Ok(self.pending.take())
    }
}

/// Persistent deterministic treap operations over content-addressed pages.
pub struct PersistentFactTree;

impl PersistentFactTree {
    /// Apply ordered fact mutations to an old root and return the new root.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the caller must
    /// load a page from durable storage before retrying the operation.
    pub fn apply(
        root: Option<StableId>,
        mutations: &[PersistentFactMutation],
        cache: &mut PersistentFactTreeCache,
    ) -> Result<Option<StableId>, PersistentFactTreeError> {
        if root.is_none()
            && !mutations.is_empty()
            && mutations.iter().all(|mutation| {
                matches!(
                    mutation,
                    PersistentFactMutation::Upsert { .. }
                        | PersistentFactMutation::UpsertWithCommitment { .. }
                )
            })
            && mutations.windows(2).all(|pair| {
                let mut adjacent = pair.iter();
                match (adjacent.next(), adjacent.next()) {
                    (Some(left), Some(right)) => left.key() < right.key(),
                    _ => false,
                }
            })
            && let Some(built_root) =
                Self::build_sorted(mutations.iter().cloned(), mutations.len(), cache)
        {
            return Ok(Some(built_root));
        }

        let mut current = root;
        for batch in mutations.chunks(APPLY_CACHE_PRUNE_INTERVAL) {
            for mutation in batch {
                current = Self::apply_one(current, mutation, cache)?;
            }
            cache.retain_reachable_dirty_roots(current);
        }
        Ok(current)
    }

    fn apply_one(
        root: Option<StableId>,
        mutation: &PersistentFactMutation,
        cache: &mut PersistentFactTreeCache,
    ) -> Result<Option<StableId>, PersistentFactTreeError> {
        match mutation {
            PersistentFactMutation::Upsert { key, value } => {
                Ok(Some(Self::insert(root, *key, value, cache)?))
            }
            PersistentFactMutation::UpsertWithCommitment {
                key,
                value,
                value_commitment,
            } => Ok(Some(Self::insert_with_commitment(
                root,
                *key,
                value.as_slice(),
                *value_commitment,
                cache,
            )?)),
            PersistentFactMutation::Remove { key } => Self::remove(root, *key, cache),
        }
    }

    fn build_sorted(
        mutations: impl IntoIterator<Item = PersistentFactMutation>,
        mutation_count: usize,
        cache: &mut PersistentFactTreeCache,
    ) -> Option<StableId> {
        let mut nodes = Vec::<PendingFactNode>::with_capacity(mutation_count);
        let mut spine = Vec::<usize>::new();
        for mutation in mutations {
            let mut node = match mutation {
                PersistentFactMutation::Upsert { key, value } => {
                    PersistentFactNode::new(key, value, None, None)
                }
                PersistentFactMutation::UpsertWithCommitment {
                    key,
                    value,
                    value_commitment,
                } => PersistentFactNode::new_with_commitment(
                    key,
                    &value,
                    value_commitment,
                    None,
                    None,
                ),
                PersistentFactMutation::Remove { .. } => return None,
            };
            let mut left = None;
            while let Some(last) = spine.last().copied() {
                let last_node = nodes.get(last)?;
                if (last_node.node.priority, last_node.node.key) <= (node.priority, node.key) {
                    break;
                }
                left = spine.pop();
            }
            let index = nodes.len();
            if let Some(parent) = spine.last().copied() {
                let parent_node = nodes.get_mut(parent)?;
                parent_node.right = Some(index);
            }
            node.left = None;
            node.right = None;
            nodes.push(PendingFactNode {
                node,
                left,
                right: None,
            });
            spine.push(index);
        }
        let root_index = spine.first().copied()?;

        let mut ids = vec![None; nodes.len()];
        let mut pending = vec![(root_index, false)];
        while let Some((index, visited)) = pending.pop() {
            let pending_node = nodes.get(index)?;
            if visited {
                let left = child_id(pending_node.left, &ids)?;
                let right = child_id(pending_node.right, &ids)?;
                let id = node_id(
                    pending_node.node.key,
                    pending_node.node.value_hash,
                    left,
                    right,
                );
                let slot = ids.get_mut(index)?;
                *slot = Some(id);
            } else {
                pending.push((index, true));
                if let Some(right) = pending_node.right {
                    pending.push((right, false));
                }
                if let Some(left) = pending_node.left {
                    pending.push((left, false));
                }
            }
        }

        let root_id = ids.get(root_index).copied().flatten()?;
        for pending_node in nodes {
            let mut node = pending_node.node;
            node.left = child_id(pending_node.left, &ids)?;
            node.right = child_id(pending_node.right, &ids)?;
            cache.store_new(node);
        }
        Some(root_id)
    }

    /// Build a deterministic root from sorted upserts without retaining a
    /// second copy of every encoded fact payload beside the page cache.
    pub(crate) fn build_sorted_owned(
        mutations: Vec<PersistentFactMutation>,
        cache: &mut PersistentFactTreeCache,
    ) -> Option<StableId> {
        let mutation_count = mutations.len();
        Self::build_sorted(mutations, mutation_count, cache)
    }

    /// Read one exact fact value from an immutable historical root.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the caller must
    /// load a page from durable storage before retrying the operation.
    pub fn get(
        root: Option<StableId>,
        key: PersistentFactKey,
        cache: &PersistentFactTreeCache,
    ) -> Result<Option<Vec<u8>>, PersistentFactTreeError> {
        let mut cursor = root;
        while let Some(id) = cursor {
            let node = cache.page(id)?;
            match key.cmp(&node.key) {
                Ordering::Less => cursor = node.left,
                Ordering::Greater => cursor = node.right,
                Ordering::Equal => return Ok(Some(node.value_payload().to_vec())),
            }
        }
        Ok(None)
    }

    /// Read at most `limit` key-ordered values strictly after `after`.
    ///
    /// The result cost is bounded by the search path plus returned values,
    /// rather than by the number of entries preceding the cursor.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the caller must
    /// load a page from durable storage before retrying the operation.
    pub fn range_after(
        root: Option<StableId>,
        after: Option<PersistentFactKey>,
        limit: usize,
        cache: &PersistentFactTreeCache,
    ) -> Result<Vec<PersistentFactNode>, PersistentFactTreeError> {
        let mut walker = PersistentFactTreeRangeWalker::new(root, after);
        let mut values = Vec::with_capacity(limit);
        while values.len() < limit {
            let Some(node) = walker.next(cache)? else {
                break;
            };
            values.push(node);
        }
        Ok(values)
    }

    /// Insert or replace one fact and return its new immutable root.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the caller must
    /// load a page from durable storage before retrying the operation.
    pub fn insert(
        root: Option<StableId>,
        key: PersistentFactKey,
        value: &[u8],
        cache: &mut PersistentFactTreeCache,
    ) -> Result<StableId, PersistentFactTreeError> {
        Self::insert_inner(root, key, value, None, cache)
    }

    /// Insert a compact payload while preserving its canonical semantic commitment.
    ///
    /// The storage payload is wrapped in a checksummed physical envelope; neither
    /// the envelope nor the compact encoding contributes to the page ID.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when an ancestor has to
    /// be loaded before retrying the operation.
    pub fn insert_with_commitment(
        root: Option<StableId>,
        key: PersistentFactKey,
        value: &[u8],
        value_commitment: StableId,
        cache: &mut PersistentFactTreeCache,
    ) -> Result<StableId, PersistentFactTreeError> {
        Self::insert_inner(root, key, value, Some(value_commitment), cache)
    }

    fn insert_inner(
        root: Option<StableId>,
        key: PersistentFactKey,
        value: &[u8],
        value_commitment: Option<StableId>,
        cache: &mut PersistentFactTreeCache,
    ) -> Result<StableId, PersistentFactTreeError> {
        let mut ancestors = Vec::new();
        let mut cursor = root;
        let mut found = None;
        while let Some(id) = cursor {
            let node = cache.page(id)?.clone();
            match key.cmp(&node.key) {
                Ordering::Less => {
                    cursor = node.left;
                    ancestors.push((node, true));
                }
                Ordering::Greater => {
                    cursor = node.right;
                    ancestors.push((node, false));
                }
                Ordering::Equal => {
                    found = Some(node);
                    break;
                }
            }
        }
        let new_node = |left, right| {
            value_commitment.map_or_else(
                || PersistentFactNode::new(key, value.to_vec(), left, right),
                |value_hash| {
                    PersistentFactNode::new_with_commitment(key, value, value_hash, left, right)
                },
            )
        };
        let mut current = if let Some(node) = found {
            cache.store_new(new_node(node.left, node.right))
        } else {
            cache.store_new(new_node(None, None))
        };
        while let Some((mut parent, descended_left)) = ancestors.pop() {
            if descended_left {
                parent.left = Some(current);
            } else {
                parent.right = Some(current);
            }
            let child = cache.page(current)?.clone();
            if ranks_before(&child, &parent) && descended_left {
                let mut lower = parent;
                lower.left = child.right;
                let lower_id = cache.store_new(lower);
                let mut upper = child;
                upper.right = Some(lower_id);
                current = cache.store_new(upper);
            } else if ranks_before(&child, &parent) && !descended_left {
                let mut lower = parent;
                lower.right = child.left;
                let lower_id = cache.store_new(lower);
                let mut upper = child;
                upper.left = Some(lower_id);
                current = cache.store_new(upper);
            } else {
                current = cache.store_new(parent);
            }
        }
        Ok(current)
    }

    /// Remove one fact and return its new immutable root.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when the caller must
    /// load a page from durable storage before retrying the operation.
    pub fn remove(
        root: Option<StableId>,
        key: PersistentFactKey,
        cache: &mut PersistentFactTreeCache,
    ) -> Result<Option<StableId>, PersistentFactTreeError> {
        let mut ancestors = Vec::new();
        let mut cursor = root;
        let target = loop {
            let Some(id) = cursor else {
                return Ok(root);
            };
            let node = cache.page(id)?.clone();
            match key.cmp(&node.key) {
                Ordering::Less => {
                    cursor = node.left;
                    ancestors.push((node, true));
                }
                Ordering::Greater => {
                    cursor = node.right;
                    ancestors.push((node, false));
                }
                Ordering::Equal => break node,
            }
        };
        let mut left = target.left;
        let mut right = target.right;
        let mut merge_path = Vec::new();
        while let (Some(left_id), Some(right_id)) = (left, right) {
            let left_node = cache.page(left_id)?.clone();
            let right_node = cache.page(right_id)?.clone();
            if ranks_before(&left_node, &right_node) {
                left = left_node.right;
                merge_path.push((left_node, true));
            } else {
                right = right_node.left;
                merge_path.push((right_node, false));
            }
        }
        let mut current = left.or(right);
        while let Some((mut node, attaches_right)) = merge_path.pop() {
            if attaches_right {
                node.right = current;
            } else {
                node.left = current;
            }
            current = Some(cache.store_new(node));
        }
        while let Some((mut parent, descended_left)) = ancestors.pop() {
            if descended_left {
                parent.left = current;
            } else {
                parent.right = current;
            }
            current = Some(cache.store_new(parent));
        }
        Ok(current)
    }
}

fn priority_for(key: PersistentFactKey) -> StableId {
    StableId::derive(
        "persistent-fact-priority-v1",
        &[&[key.fact_kind], &key.fact_id.0],
    )
}

fn node_id(
    key: PersistentFactKey,
    value_hash: StableId,
    left: Option<StableId>,
    right: Option<StableId>,
) -> StableId {
    let fact_kind = [key.fact_kind];
    let left = optional_id_bytes(left);
    let right = optional_id_bytes(right);
    StableId::derive(
        "persistent-fact-node-v1",
        &[&fact_kind, &key.fact_id.0, &value_hash.0, &left, &right],
    )
}

fn child_id(child: Option<usize>, ids: &[Option<StableId>]) -> Option<Option<StableId>> {
    child.map_or(Some(None), |index| {
        ids.get(index).copied().flatten().map(Some)
    })
}

fn ranks_before(left: &PersistentFactNode, right: &PersistentFactNode) -> bool {
    (left.priority, left.key) < (right.priority, right.key)
}

fn optional_id_bytes(id: Option<StableId>) -> [u8; 33] {
    let mut bytes = [0_u8; 33];
    if let Some(id) = id {
        bytes[0] = 1;
        bytes[1..].copy_from_slice(&id.0);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn key(value: u8) -> PersistentFactKey {
        PersistentFactKey {
            fact_kind: 1,
            fact_id: StableId::derive("persistent-fact-tree-test-key", &[&[value]]),
        }
    }

    fn insert_with_cache(
        root: Option<StableId>,
        key: PersistentFactKey,
        value: &[u8],
        cache: &mut PersistentFactTreeCache,
    ) -> Option<StableId> {
        PersistentFactTree::insert(root, key, value, cache).ok()
    }

    fn remove_with_cache(
        root: Option<StableId>,
        key: PersistentFactKey,
        cache: &mut PersistentFactTreeCache,
    ) -> Option<Option<StableId>> {
        PersistentFactTree::remove(root, key, cache).ok()
    }

    fn collect(
        root: Option<StableId>,
        cache: &PersistentFactTreeCache,
    ) -> Option<Vec<(PersistentFactKey, Vec<u8>)>> {
        let mut walker = PersistentFactTreeWalker::new(root);
        let mut facts = Vec::new();
        while let Some((_, node)) = walker.next(cache).ok()? {
            facts.push((node.key, node.value));
        }
        facts.sort_by_key(|(key, _)| *key);
        Some(facts)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn mutation_sequences_match_ordered_reference_and_preserve_old_roots(
            operations in prop::collection::vec(
                (0_u8..4, 0_u8..32, any::<bool>(), prop::collection::vec(any::<u8>(), 0..32)),
                0..200,
            )
        ) {
            let mut cache = PersistentFactTreeCache::default();
            let mut root = None;
            let mut reference = BTreeMap::new();
            let mut snapshots = Vec::new();

            for (index, (fact_kind, identity, upsert, value)) in operations.into_iter().enumerate() {
                let fact_key = PersistentFactKey {
                    fact_kind,
                    fact_id: StableId::derive(
                        "persistent-fact-tree-property-key",
                        &[&[identity]],
                    ),
                };
                root = if upsert {
                    reference.insert(fact_key, value.clone());
                    Some(
                        PersistentFactTree::insert(root, fact_key, &value, &mut cache)
                            .map_err(|error| {
                                TestCaseError::fail(format!("upsert failed: {error:?}"))
                            })?,
                    )
                } else {
                    reference.remove(&fact_key);
                    PersistentFactTree::remove(root, fact_key, &mut cache)
                        .map_err(|error| TestCaseError::fail(format!("remove failed: {error:?}")))?
                };

            let expected = reference
                .iter()
                .map(|(key, stored_value)| (*key, stored_value.clone()))
                .collect::<Vec<_>>();
                prop_assert_eq!(collect(root, &cache), Some(expected));

                if index % 16 == 0 {
                    snapshots.push((root, reference.clone()));
                }
            }

            for (snapshot_root, snapshot_facts) in snapshots {
                let expected = snapshot_facts.into_iter().collect::<Vec<_>>();
                prop_assert_eq!(collect(snapshot_root, &cache), Some(expected));
            }
        }
    }

    #[test]
    fn compact_payload_keeps_semantic_root_and_detects_storage_corruption() {
        let fact_key = key(99);
        let canonical_value = br#"{"name":"sample"}"#.to_vec();
        let compact_value = b"SMB1 compact fact bytes".to_vec();
        let value_commitment = StableId::derive("persistent-fact-value-v1", &[&canonical_value]);

        let mut canonical_cache = PersistentFactTreeCache::default();
        let canonical_root =
            PersistentFactTree::insert(None, fact_key, &canonical_value, &mut canonical_cache).ok();
        assert!(canonical_root.is_some());
        let Some(canonical_root) = canonical_root else {
            return;
        };

        let mut compact_cache = PersistentFactTreeCache::default();
        let compact_root = PersistentFactTree::insert_with_commitment(
            None,
            fact_key,
            compact_value.as_slice(),
            value_commitment,
            &mut compact_cache,
        )
        .ok();
        assert!(compact_root.is_some());
        let Some(compact_root) = compact_root else {
            return;
        };
        assert_eq!(canonical_root, compact_root);
        assert_eq!(
            PersistentFactTree::get(Some(compact_root), fact_key, &compact_cache),
            Ok(Some(compact_value))
        );

        let compact_page = compact_cache
            .take_dirty()
            .into_iter()
            .find(|(id, _)| *id == compact_root)
            .map(|(_, page)| page);
        assert!(compact_page.is_some());
        let Some(compact_page) = compact_page else {
            return;
        };
        let mut loaded_cache = PersistentFactTreeCache::default();
        let loaded = loaded_cache.insert_loaded(compact_root, compact_page.clone());
        assert!(loaded.is_ok());

        let mut corrupted_page = compact_page;
        let payload_offset = STORAGE_ENVELOPE_HEADER_BYTES;
        let Some(byte) = corrupted_page.value.get_mut(payload_offset) else {
            return;
        };
        *byte ^= 1;
        let mut corrupted_cache = PersistentFactTreeCache::default();
        let corrupt_result = corrupted_cache.insert_loaded(compact_root, corrupted_page);
        assert_eq!(
            corrupt_result,
            Err(PersistentFactTreeError::CorruptPage(compact_root))
        );
    }

    #[test]
    fn generations_share_pages_and_old_roots_remain_immutable() {
        let mut cache = PersistentFactTreeCache::default();
        let first_result = insert_with_cache(None, key(1), b"one", &mut cache);
        assert!(first_result.is_some());
        let Some(first) = first_result else { return };
        let second_result = insert_with_cache(Some(first), key(2), b"two", &mut cache);
        assert!(second_result.is_some());
        let Some(second) = second_result else { return };
        let old_facts_result = collect(Some(first), &cache);
        assert!(old_facts_result.is_some());
        let Some(old_facts) = old_facts_result else {
            return;
        };
        let new_facts_result = collect(Some(second), &cache);
        assert!(new_facts_result.is_some());
        let Some(new_facts) = new_facts_result else {
            return;
        };
        assert_eq!(old_facts, vec![(key(1), b"one".to_vec())]);
        assert_eq!(new_facts.len(), 2);
        assert!(cache.take_dirty().len() < 4);
    }

    #[test]
    fn cloned_cache_mutations_do_not_change_source_snapshot() {
        let mut source = PersistentFactTreeCache::default();
        let original_result = insert_with_cache(None, key(1), b"one", &mut source);
        assert!(original_result.is_some());
        let Some(original_root) = original_result else {
            return;
        };

        let mut candidate = source.clone();
        let candidate_root_result =
            insert_with_cache(Some(original_root), key(2), b"two", &mut candidate);
        assert!(candidate_root_result.is_some());
        let Some(candidate_root) = candidate_root_result else {
            return;
        };

        assert_eq!(
            collect(Some(original_root), &source),
            Some(vec![(key(1), b"one".to_vec())])
        );
        let mut expected_candidate = vec![(key(1), b"one".to_vec()), (key(2), b"two".to_vec())];
        expected_candidate.sort_by_key(|(fact_key, _)| *fact_key);
        assert_eq!(
            collect(Some(candidate_root), &candidate),
            Some(expected_candidate)
        );
    }

    #[test]
    fn batch_drain_omits_superseded_unreachable_pages() {
        let mut cache = PersistentFactTreeCache::default();
        let mut root = None;
        for value in 0_u8..64 {
            let next = insert_with_cache(root, key(value), &[value], &mut cache);
            assert!(next.is_some());
            root = next;
        }
        let Some(root) = root else { return };
        let allocated = cache.dirty.len();
        let reachable = cache.take_reachable_dirty(Some(root));
        assert!(allocated > reachable.len());
        assert_eq!(reachable.len(), 64);
        assert_eq!(
            collect(Some(root), &cache).map(|facts| facts.len()),
            Some(64)
        );
    }

    #[test]
    fn large_apply_prunes_intermediate_pages_at_bounded_intervals() {
        let mutations = (0_u64..8_192)
            .map(|value| {
                let key = PersistentFactKey {
                    fact_kind: 1,
                    fact_id: StableId::derive(
                        "persistent-fact-tree-batch-test-key",
                        &[&value.to_be_bytes()],
                    ),
                };
                let canonical_value = value.to_be_bytes().to_vec();
                if value % 2 == 0 {
                    PersistentFactMutation::Upsert {
                        key,
                        value: canonical_value,
                    }
                } else {
                    let value_commitment =
                        StableId::derive("persistent-fact-value-v1", &[&canonical_value]);
                    PersistentFactMutation::UpsertWithCommitment {
                        key,
                        value: value.to_le_bytes().to_vec(),
                        value_commitment,
                    }
                }
            })
            .collect::<Vec<_>>();
        let mut cache = PersistentFactTreeCache::default();
        let root = PersistentFactTree::apply(None, &mutations, &mut cache).ok();
        assert!(root.is_some());
        let Some(Some(root)) = root else { return };

        assert_eq!(cache.pages.len(), mutations.len());
        assert_eq!(cache.dirty.len(), mutations.len());
        assert_eq!(
            collect(Some(root), &cache).map(|facts| facts.len()),
            Some(8_192)
        );
    }

    #[test]
    fn resumable_apply_matches_batch_with_lazy_page_loading()
    -> Result<(), Box<dyn std::error::Error>> {
        let base_mutations = (0_u8..96)
            .map(|value| PersistentFactMutation::Upsert {
                key: key(value),
                value: vec![value],
            })
            .collect::<Vec<_>>();
        let mut persisted_cache = PersistentFactTreeCache::default();
        let base_root = PersistentFactTree::apply(None, &base_mutations, &mut persisted_cache)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?
            .ok_or_else(|| std::io::Error::other("initial tree has no root"))?;
        let pages = persisted_cache
            .take_dirty()
            .into_iter()
            .collect::<BTreeMap<_, _>>();

        let mutations = vec![
            PersistentFactMutation::Remove { key: key(7) },
            PersistentFactMutation::Upsert {
                key: key(23),
                value: b"replacement".to_vec(),
            },
            PersistentFactMutation::Upsert {
                key: key(101),
                value: b"new fact".to_vec(),
            },
            PersistentFactMutation::Remove { key: key(81) },
        ];
        let mut expected_cache = PersistentFactTreeCache::default();
        for (id, node) in &pages {
            expected_cache
                .insert_loaded(*id, node.clone())
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        }
        let expected_root =
            PersistentFactTree::apply(Some(base_root), &mutations, &mut expected_cache)
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

        let mut cache = PersistentFactTreeCache::default();
        let mut update = PersistentFactTreeApply::new(Some(base_root), &mutations);
        let mut missing_page_count = 0_usize;
        while !update.is_complete() {
            let root_before = update.root();
            match update.advance(&mut cache) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(
                        std::io::Error::other("unfinished update reported completion").into(),
                    );
                }
                Err(PersistentFactTreeError::MissingPage(id)) => {
                    missing_page_count = missing_page_count.saturating_add(1);
                    if update.root() != root_before {
                        return Err(std::io::Error::other(
                            "missing page advanced the working root",
                        )
                        .into());
                    }
                    let page = pages
                        .get(&id)
                        .ok_or_else(|| std::io::Error::other("requested page is not persisted"))?;
                    cache
                        .insert_loaded(id, page.clone())
                        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
                }
                Err(error) => {
                    return Err(std::io::Error::other(format!("{error:?}")).into());
                }
            }
        }

        if missing_page_count <= 1 {
            return Err(std::io::Error::other("test did not load multiple lazy pages").into());
        }
        if update.root() != expected_root {
            return Err(std::io::Error::other("resumed and batch roots differ").into());
        }
        for (id, node) in &pages {
            cache
                .insert_loaded(*id, node.clone())
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        }
        if collect(update.root(), &cache) != collect(expected_root, &expected_cache) {
            return Err(std::io::Error::other("resumed and batch facts differ").into());
        }
        Ok(())
    }

    #[test]
    fn sorted_bulk_build_matches_incremental_treap_root() {
        let mut mutations = (0_u64..128)
            .map(|value| {
                let key = PersistentFactKey {
                    fact_kind: if value % 2 == 0 { 0 } else { 1 },
                    fact_id: StableId::derive(
                        "persistent-fact-tree-bulk-equivalence-key",
                        &[&value.to_be_bytes()],
                    ),
                };
                PersistentFactMutation::Upsert {
                    key,
                    value: value.to_be_bytes().to_vec(),
                }
            })
            .collect::<Vec<_>>();
        mutations.sort_by_key(PersistentFactMutation::key);

        let mut bulk_cache = PersistentFactTreeCache::default();
        let bulk_root = PersistentFactTree::apply(None, &mutations, &mut bulk_cache).ok();
        assert!(bulk_root.is_some());
        let Some(Some(bulk_root)) = bulk_root else {
            return;
        };

        let mut incremental_cache = PersistentFactTreeCache::default();
        let mut incremental_root = None;
        for mutation in &mutations {
            incremental_root = match mutation {
                PersistentFactMutation::Upsert { key, value } => PersistentFactTree::insert(
                    incremental_root,
                    *key,
                    value,
                    &mut incremental_cache,
                )
                .ok(),
                PersistentFactMutation::UpsertWithCommitment {
                    key,
                    value,
                    value_commitment,
                } => PersistentFactTree::insert_with_commitment(
                    incremental_root,
                    *key,
                    value,
                    *value_commitment,
                    &mut incremental_cache,
                )
                .ok(),
                PersistentFactMutation::Remove { .. } => return,
            };
        }
        assert_eq!(incremental_root, Some(bulk_root));
        assert_eq!(
            collect(Some(bulk_root), &bulk_cache).map(|facts| facts.len()),
            collect(incremental_root, &incremental_cache).map(|facts| facts.len())
        );
    }

    #[test]
    fn replacement_removal_and_deterministic_roots_match_ordered_facts() {
        let mut first_cache = PersistentFactTreeCache::default();
        let mut first_root = None;
        for value in 0_u8..40 {
            let root = insert_with_cache(first_root, key(value), &[value], &mut first_cache);
            assert!(root.is_some());
            let Some(root) = root else { return };
            first_root = Some(root);
        }
        let mut second_cache = PersistentFactTreeCache::default();
        let mut second_root = None;
        for value in (0_u8..40).rev() {
            let root = insert_with_cache(second_root, key(value), &[value], &mut second_cache);
            assert!(root.is_some());
            let Some(root) = root else { return };
            second_root = Some(root);
        }
        assert_eq!(first_root, second_root);

        let replaced = insert_with_cache(first_root, key(20), b"replacement", &mut first_cache);
        assert!(replaced.is_some());
        let Some(replaced) = replaced else { return };
        let removed = remove_with_cache(Some(replaced), key(10), &mut first_cache);
        assert!(removed.is_some());
        let Some(removed) = removed else { return };
        let facts_result = collect(removed, &first_cache);
        assert!(facts_result.is_some());
        let Some(facts) = facts_result else { return };
        assert_eq!(facts.len(), 39);
        assert!(!facts.iter().any(|(fact_key, _)| *fact_key == key(10)));
        assert!(
            facts
                .iter()
                .any(|(fact_key, value)| { *fact_key == key(20) && value == b"replacement" })
        );
        let original_facts = collect(first_root, &first_cache);
        assert!(original_facts.is_some());
        assert_eq!(original_facts.map(|entries| entries.len()), Some(40));
    }

    #[test]
    fn walker_preserves_missing_page_for_lazy_storage_loading() {
        let node = PersistentFactNode::new(key(1), b"value".to_vec(), None, None);
        let id = node.id();
        let mut walker = PersistentFactTreeWalker::new(Some(id));
        let cache = PersistentFactTreeCache::default();
        assert_eq!(
            walker.next(&cache),
            Err(PersistentFactTreeError::MissingPage(id))
        );
        let mut loaded = PersistentFactTreeCache::default();
        let loaded_result = loaded.insert_loaded(id, node);
        assert!(loaded_result.is_ok());
        if loaded_result.is_err() {
            return;
        }
        assert_eq!(walker.next(&loaded).map(|next| next.is_some()), Ok(true));
    }

    #[test]
    fn ordered_range_walker_resumes_missing_pages_without_restarting() {
        let mut source_cache = PersistentFactTreeCache::default();
        let mut root = None;
        for value in 0_u8..40 {
            let next = insert_with_cache(root, key(value), &[value], &mut source_cache);
            assert!(next.is_some());
            let Some(next) = next else { return };
            root = Some(next);
        }
        let pages = source_cache
            .take_dirty()
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let mut cache = PersistentFactTreeCache::default();
        let mut walker = PersistentFactTreeRangeWalker::new(root, Some(key(15)));
        let mut found = Vec::new();
        loop {
            match walker.next(&cache) {
                Ok(Some(node)) => found.push(node.key),
                Ok(None) => break,
                Err(PersistentFactTreeError::MissingPage(id)) => {
                    let page = pages.get(&id).cloned();
                    assert!(page.is_some());
                    let Some(page) = page else { return };
                    let inserted = cache.insert_loaded(id, page);
                    assert!(inserted.is_ok());
                }
                Err(PersistentFactTreeError::CorruptPage(_)) => return,
            }
        }
        let mut expected = (0_u8..40)
            .map(key)
            .filter(|candidate| *candidate > key(15))
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(found, expected);
        #[cfg(feature = "benchmark-instrumentation")]
        assert!(cache.page_lookup_count() <= pages.len().saturating_mul(3));
    }

    #[test]
    fn point_lookup_resumes_at_the_missing_page() {
        let mut source_cache = PersistentFactTreeCache::default();
        let mut root = None;
        for value in 0_u8..40 {
            let next = insert_with_cache(root, key(value), &[value], &mut source_cache);
            assert!(next.is_some());
            let Some(next) = next else { return };
            root = Some(next);
        }
        let pages = source_cache
            .take_dirty()
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let mut cache = PersistentFactTreeCache::default();
        let mut lookup = PersistentFactTreeLookup::new(root, key(20));
        let value = loop {
            match lookup.resolve(&cache) {
                Ok(Some(value)) => break value,
                Ok(None) | Err(PersistentFactTreeError::CorruptPage(_)) => return,
                Err(PersistentFactTreeError::MissingPage(id)) => {
                    let page = pages.get(&id).cloned();
                    assert!(page.is_some());
                    let Some(page) = page else { return };
                    let inserted = cache.insert_loaded(id, page);
                    assert!(inserted.is_ok());
                }
            }
        };
        assert_eq!(value, vec![20]);
        #[cfg(feature = "benchmark-instrumentation")]
        assert!(cache.page_lookup_count() <= pages.len().saturating_mul(2));
    }
}
