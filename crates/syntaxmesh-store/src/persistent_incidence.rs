//! Prototype persistent endpoint-to-edge incidence index.
//!
//! This composes the existing content-addressed ordered fact tree rather than
//! introducing another page format: an outer map locates an endpoint's inner
//! edge-ID set. The outer key uses distinct source/target fact-kind tags, so
//! direction remains explicit while the canonical graph fact tree is intact.

use std::collections::BTreeMap;

use syntaxmesh_core::{EdgeId, NodeId, StableId};

use crate::{
    PersistentFactKey, PersistentFactMutation, PersistentFactTree, PersistentFactTreeCache,
    PersistentFactTreeError, PersistentFactTreeLookup, PersistentFactTreeRangeWalker,
    PersistentFactTreeWalker,
};

const SOURCE_ENDPOINT_KIND: u8 = 4;
const TARGET_ENDPOINT_KIND: u8 = 5;
const INCIDENT_EDGE_KIND: u8 = 6;

/// One edge-incidence change applied while deriving a generation's index root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncidenceMutation {
    pub edge: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub present: bool,
}

/// Persistent endpoint incidence maps built from the existing fact-page tree.
pub struct PersistentIncidenceIndex;

/// Resumable lookup of one endpoint's direction-specific incidence root.
#[derive(Debug)]
pub struct PersistentIncidenceEndpointLookup {
    lookup: PersistentFactTreeLookup,
}

impl PersistentIncidenceEndpointLookup {
    /// Continue resolving the endpoint root without replaying prior search
    /// steps after a missing durable page.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] if the next search page
    /// has not been loaded into the supplied cache, or `CorruptPage` if the
    /// stored nested root is invalid.
    pub fn resolve(
        &mut self,
        cache: &PersistentFactTreeCache,
    ) -> Result<Option<StableId>, PersistentFactTreeError> {
        self.lookup
            .resolve(cache)?
            .map(|bytes| decode_root(&bytes))
            .transpose()
    }
}

/// Resumable ordered cursor over one endpoint's edge IDs.
#[derive(Debug)]
pub struct PersistentIncidencePageWalker {
    walker: PersistentFactTreeRangeWalker,
}

impl PersistentIncidencePageWalker {
    /// Start an ordered incidence page after an optional edge ID.
    #[must_use]
    pub fn new(root: Option<StableId>, after: Option<EdgeId>) -> Self {
        let after_key = after.map(|edge| PersistentFactKey {
            fact_kind: INCIDENT_EDGE_KIND,
            fact_id: edge.0,
        });
        Self {
            walker: PersistentFactTreeRangeWalker::new(root, after_key),
        }
    }

    /// Return the next edge ID; a missing page preserves the exact cursor for
    /// resume after the caller loads that page.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] if the next page is not
    /// loaded, or `CorruptPage` if a loaded page fails validation.
    pub fn next(
        &mut self,
        cache: &PersistentFactTreeCache,
    ) -> Result<Option<EdgeId>, PersistentFactTreeError> {
        self.walker
            .next(cache)
            .map(|node| node.map(|page| EdgeId(page.key.fact_id)))
    }
}

/// Resumable endpoint-index mutation cursor; a missing durable page does not
/// replay already-applied incidences.
pub struct PersistentIncidenceApply {
    mutations: Vec<(u8, StableId, StableId, bool)>,
    next_mutation: usize,
    root: Option<StableId>,
    nested_roots: BTreeMap<(u8, StableId), Option<StableId>>,
    pending_outer_update: Option<(u8, StableId)>,
}

impl PersistentIncidenceApply {
    #[must_use]
    pub fn new(root: Option<StableId>, changes: &[IncidenceMutation]) -> Self {
        let mut normalized = BTreeMap::new();
        for change in changes {
            normalized.insert(
                (SOURCE_ENDPOINT_KIND, change.source.0, change.edge.0),
                change.present,
            );
            normalized.insert(
                (TARGET_ENDPOINT_KIND, change.target.0, change.edge.0),
                change.present,
            );
        }
        Self {
            mutations: normalized
                .into_iter()
                .map(|((direction, endpoint, edge), present)| (direction, endpoint, edge, present))
                .collect(),
            next_mutation: 0,
            root,
            nested_roots: BTreeMap::new(),
            pending_outer_update: None,
        }
    }

    /// Apply at most one incidence. A missing page leaves this operation
    /// uncommitted, so callers can load it and resume safely.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] when a durable page
    /// must be loaded, or `CorruptPage` when a cached page is invalid.
    pub fn advance(
        &mut self,
        cache: &mut PersistentFactTreeCache,
    ) -> Result<bool, PersistentFactTreeError> {
        if let Some((direction, endpoint)) = self.pending_outer_update {
            let endpoint_key = PersistentFactKey {
                fact_kind: direction,
                fact_id: endpoint,
            };
            let inner_root = self
                .nested_roots
                .get(&(direction, endpoint))
                .copied()
                .flatten();
            self.root = match inner_root {
                Some(inner_root) => Some(PersistentFactTree::insert(
                    self.root,
                    endpoint_key,
                    &inner_root.0,
                    cache,
                )?),
                None => PersistentFactTree::remove(self.root, endpoint_key, cache)?,
            };
            self.pending_outer_update = None;
        }
        let Some((direction, endpoint, edge, present)) =
            self.mutations.get(self.next_mutation).copied()
        else {
            return Ok(false);
        };
        let endpoint_key = PersistentFactKey {
            fact_kind: direction,
            fact_id: endpoint,
        };
        let existing_inner_root =
            if let Some(inner_root) = self.nested_roots.get(&(direction, endpoint)) {
                *inner_root
            } else {
                PersistentFactTree::get(self.root, endpoint_key, cache)?
                    .map(|bytes| decode_root(&bytes))
                    .transpose()?
            };
        let edge_key = PersistentFactKey {
            fact_kind: INCIDENT_EDGE_KIND,
            fact_id: edge,
        };
        let next_inner_root = if present {
            Some(PersistentFactTree::insert(
                existing_inner_root,
                edge_key,
                &[],
                cache,
            )?)
        } else {
            PersistentFactTree::remove(existing_inner_root, edge_key, cache)?
        };

        self.nested_roots
            .insert((direction, endpoint), next_inner_root);
        self.next_mutation = self.next_mutation.saturating_add(1);
        if self.mutations.get(self.next_mutation).is_none_or(
            |(next_direction, next_endpoint, ..)| {
                *next_direction != direction || *next_endpoint != endpoint
            },
        ) {
            self.pending_outer_update = Some((direction, endpoint));
        }
        Ok(true)
    }

    #[must_use]
    pub const fn root(&self) -> Option<StableId> {
        self.root
    }

    pub fn dirty_roots(&self) -> impl Iterator<Item = StableId> + '_ {
        self.root
            .into_iter()
            .chain(self.nested_roots.values().flatten().copied())
    }
}

impl PersistentIncidenceIndex {
    /// Apply edge incidence changes to an immutable index root.
    ///
    /// Each endpoint owns an inner persistent set of edge IDs. Updating one
    /// incidence path-copies that set and the outer endpoint-map path; other
    /// endpoint sets and untouched pages remain shared.
    ///
    /// # Errors
    /// Returns a tree error when a referenced page must be loaded or is corrupt.
    pub fn apply(
        root: Option<StableId>,
        changes: &[IncidenceMutation],
        cache: &mut PersistentFactTreeCache,
    ) -> Result<(Option<StableId>, Vec<StableId>), PersistentFactTreeError> {
        if root.is_none() {
            return Self::build_empty_root(changes, cache);
        }
        let mut apply = PersistentIncidenceApply::new(root, changes);
        while apply.advance(cache)? {}
        Ok((
            apply.root(),
            apply
                .dirty_roots()
                .skip(usize::from(apply.root().is_some()))
                .collect(),
        ))
    }

    fn build_empty_root(
        changes: &[IncidenceMutation],
        cache: &mut PersistentFactTreeCache,
    ) -> Result<(Option<StableId>, Vec<StableId>), PersistentFactTreeError> {
        let mutations = PersistentIncidenceApply::new(None, changes).mutations;
        let mut outer_mutations = Vec::new();
        let mut nested_roots = Vec::new();
        let mut start = 0;
        while let Some((direction, endpoint, ..)) = mutations.get(start).copied() {
            let mut end = start.saturating_add(1);
            while mutations
                .get(end)
                .is_some_and(|(next_direction, next_endpoint, ..)| {
                    *next_direction == direction && *next_endpoint == endpoint
                })
            {
                end = end.saturating_add(1);
            }
            let endpoint_mutations = mutations.get(start..end).ok_or_else(|| {
                PersistentFactTreeError::CorruptPage(StableId::derive(
                    "persistent-incidence-build-range-v1",
                    &[&start.to_be_bytes(), &end.to_be_bytes()],
                ))
            })?;
            let nested_mutations = endpoint_mutations
                .iter()
                .filter(|(_, _, _, present)| *present)
                .map(|(_, _, edge, _)| PersistentFactMutation::Upsert {
                    key: PersistentFactKey {
                        fact_kind: INCIDENT_EDGE_KIND,
                        fact_id: *edge,
                    },
                    value: Vec::new(),
                })
                .collect::<Vec<_>>();
            if !nested_mutations.is_empty()
                && let Some(nested_root) =
                    PersistentFactTree::apply(None, &nested_mutations, cache)?
            {
                outer_mutations.push(PersistentFactMutation::Upsert {
                    key: PersistentFactKey {
                        fact_kind: direction,
                        fact_id: endpoint,
                    },
                    value: nested_root.0.to_vec(),
                });
                nested_roots.push(nested_root);
            }
            start = end;
        }
        let root = PersistentFactTree::apply(None, &outer_mutations, cache)?;
        Ok((root, nested_roots))
    }

    /// Resolve one endpoint's source or target incidence-set root.
    ///
    /// # Errors
    /// Returns a tree error when the outer path or nested root payload is invalid.
    pub fn endpoint_root(
        root: Option<StableId>,
        endpoint: NodeId,
        outgoing: bool,
        cache: &PersistentFactTreeCache,
    ) -> Result<Option<StableId>, PersistentFactTreeError> {
        let mut lookup = Self::endpoint_lookup(root, endpoint, outgoing);
        lookup.resolve(cache)
    }

    /// Start a resumable lookup of an endpoint's direction-specific root.
    #[must_use]
    pub const fn endpoint_lookup(
        root: Option<StableId>,
        endpoint: NodeId,
        outgoing: bool,
    ) -> PersistentIncidenceEndpointLookup {
        let key = PersistentFactKey {
            fact_kind: if outgoing {
                SOURCE_ENDPOINT_KIND
            } else {
                TARGET_ENDPOINT_KIND
            },
            fact_id: endpoint.0,
        };
        PersistentIncidenceEndpointLookup {
            lookup: PersistentFactTreeLookup::new(root, key),
        }
    }

    /// Materialize an endpoint's edge IDs for small reference/test callers.
    ///
    /// Durable query adapters will use an ordered bounded walker, not this
    /// convenience method, when they add the persisted index read path.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] for an unloaded page or
    /// `CorruptPage` for an invalid page.
    pub fn edge_ids(
        root: Option<StableId>,
        cache: &PersistentFactTreeCache,
    ) -> Result<Vec<EdgeId>, PersistentFactTreeError> {
        let mut walker = PersistentFactTreeWalker::new(root);
        let mut edges = Vec::new();
        while let Some((_, page)) = walker.next(cache)? {
            edges.push(EdgeId(page.key.fact_id));
        }
        edges.sort_unstable();
        Ok(edges)
    }

    /// Read one ordered, bounded edge-ID page after an optional edge cursor.
    ///
    /// # Errors
    /// Returns [`PersistentFactTreeError::MissingPage`] for an unloaded page or
    /// `CorruptPage` for an invalid page.
    pub fn edge_ids_page(
        root: Option<StableId>,
        after: Option<EdgeId>,
        limit: usize,
        cache: &PersistentFactTreeCache,
    ) -> Result<Vec<EdgeId>, PersistentFactTreeError> {
        let mut walker = PersistentIncidencePageWalker::new(root, after);
        let mut edge_ids = Vec::with_capacity(limit);
        while edge_ids.len() < limit {
            let Some(edge_id) = walker.next(cache)? else {
                break;
            };
            edge_ids.push(edge_id);
        }
        Ok(edge_ids)
    }
}

fn decode_root(bytes: &[u8]) -> Result<StableId, PersistentFactTreeError> {
    let root = bytes.try_into().map_err(|_error| {
        PersistentFactTreeError::CorruptPage(StableId::derive(
            "invalid-incidence-root-v1",
            &[bytes],
        ))
    })?;
    Ok(StableId(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PersistentFactKey;

    fn id(value: u8) -> StableId {
        StableId([value; 32])
    }

    fn change(edge: u8, source: u8, target: u8, present: bool) -> IncidenceMutation {
        IncidenceMutation {
            edge: EdgeId(id(edge)),
            source: NodeId(id(source)),
            target: NodeId(id(target)),
            present,
        }
    }

    fn insert_all(
        cache: &mut PersistentFactTreeCache,
        root: Option<StableId>,
        changes: &[IncidenceMutation],
    ) -> Option<(StableId, Vec<StableId>)> {
        let (root, nested_roots) = PersistentIncidenceIndex::apply(root, changes, cache).ok()?;
        Some((root?, nested_roots))
    }

    #[test]
    fn bulk_initial_build_matches_resumable_apply_with_fewer_page_lookups() {
        let mut changes = (0_u16..256)
            .map(|edge| {
                let edge_id = StableId::derive("bulk-incidence-edge", &[&edge.to_be_bytes()]);
                let source_id =
                    StableId::derive("bulk-incidence-source", &[&(edge % 17).to_be_bytes()]);
                let target_id =
                    StableId::derive("bulk-incidence-target", &[&(edge % 23).to_be_bytes()]);
                IncidenceMutation {
                    edge: EdgeId(edge_id),
                    source: NodeId(source_id),
                    target: NodeId(target_id),
                    present: true,
                }
            })
            .collect::<Vec<_>>();
        let Some(selected_change) = changes.get(7).copied() else {
            return;
        };
        let mut duplicate = selected_change;
        duplicate.present = false;
        changes.push(duplicate);
        let mut final_state = duplicate;
        final_state.present = true;
        changes.push(final_state);

        let mut bulk_cache = PersistentFactTreeCache::default();
        let bulk_result = PersistentIncidenceIndex::apply(None, &changes, &mut bulk_cache);
        assert!(bulk_result.is_ok());
        let Some((bulk_root, bulk_nested_roots)) = bulk_result.ok() else {
            return;
        };

        let mut incremental_cache = PersistentFactTreeCache::default();
        let mut apply = PersistentIncidenceApply::new(None, &changes);
        let mut apply_failed = false;
        loop {
            match apply.advance(&mut incremental_cache) {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {
                    apply_failed = true;
                    break;
                }
            }
        }
        assert!(!apply_failed, "in-memory apply requested an absent page");
        let incremental_root = apply.root();

        assert_eq!(bulk_root, incremental_root);
        #[cfg(feature = "benchmark-instrumentation")]
        assert!(
            bulk_cache.page_lookup_count() < incremental_cache.page_lookup_count(),
            "bulk build used {} page lookups; incremental apply used {}",
            bulk_cache.page_lookup_count(),
            incremental_cache.page_lookup_count()
        );
        let source_root = PersistentIncidenceIndex::endpoint_root(
            bulk_root,
            selected_change.source,
            true,
            &bulk_cache,
        )
        .ok()
        .flatten();
        assert!(source_root.is_some(), "bulk source root should be present");
        let source_edges = source_root
            .and_then(|root| PersistentIncidenceIndex::edge_ids(Some(root), &bulk_cache).ok());
        assert!(
            source_edges.is_some_and(|edges| edges.contains(&selected_change.edge)),
            "last duplicate mutation should win"
        );
        assert!(!bulk_nested_roots.is_empty());
    }

    #[test]
    fn incidence_sets_share_untouched_endpoints_and_track_both_directions() {
        let mut cache = PersistentFactTreeCache::default();
        let first = insert_all(
            &mut cache,
            None,
            &[change(11, 1, 2, true), change(12, 3, 2, true)],
        );
        assert!(first.is_some());
        let Some((first, _first_nested_roots)) = first else {
            return;
        };
        let first_incoming_two =
            PersistentIncidenceIndex::endpoint_root(Some(first), NodeId(id(2)), false, &cache)
                .ok()
                .flatten();
        let first_outgoing_one =
            PersistentIncidenceIndex::endpoint_root(Some(first), NodeId(id(1)), true, &cache)
                .ok()
                .flatten();
        assert!(first_incoming_two.is_some());
        assert!(first_outgoing_one.is_some());

        let second = insert_all(&mut cache, Some(first), &[change(13, 4, 2, true)]);
        assert!(second.is_some());
        let Some((second, _second_nested_roots)) = second else {
            return;
        };
        let second_incoming_two =
            PersistentIncidenceIndex::endpoint_root(Some(second), NodeId(id(2)), false, &cache)
                .ok()
                .flatten();
        let second_outgoing_one =
            PersistentIncidenceIndex::endpoint_root(Some(second), NodeId(id(1)), true, &cache)
                .ok()
                .flatten();
        assert_ne!(first_incoming_two, second_incoming_two);
        assert_eq!(first_outgoing_one, second_outgoing_one);
        assert_eq!(
            PersistentIncidenceIndex::edge_ids(first_incoming_two, &cache),
            Ok(vec![EdgeId(id(11)), EdgeId(id(12))])
        );
        assert_eq!(
            PersistentIncidenceIndex::edge_ids(second_incoming_two, &cache),
            Ok(vec![EdgeId(id(11)), EdgeId(id(12)), EdgeId(id(13))])
        );
        assert_eq!(
            PersistentIncidenceIndex::edge_ids_page(second_incoming_two, None, 2, &cache),
            Ok(vec![EdgeId(id(11)), EdgeId(id(12))])
        );
        assert_eq!(
            PersistentIncidenceIndex::edge_ids_page(
                second_incoming_two,
                Some(EdgeId(id(12))),
                2,
                &cache
            ),
            Ok(vec![EdgeId(id(13))])
        );

        let third = insert_all(&mut cache, Some(second), &[change(11, 1, 2, false)]);
        assert!(third.is_some());
        let Some((third, _third_nested_roots)) = third else {
            return;
        };
        let third_incoming_two =
            PersistentIncidenceIndex::endpoint_root(Some(third), NodeId(id(2)), false, &cache)
                .ok()
                .flatten();
        assert_eq!(
            PersistentIncidenceIndex::edge_ids(third_incoming_two, &cache),
            Ok(vec![EdgeId(id(12)), EdgeId(id(13))])
        );
        assert_eq!(
            PersistentFactTree::get(
                Some(third),
                PersistentFactKey {
                    fact_kind: SOURCE_ENDPOINT_KIND,
                    fact_id: id(1),
                },
                &cache,
            ),
            Ok(None)
        );
    }

    #[test]
    fn malformed_nested_root_is_rejected() {
        let mut cache = PersistentFactTreeCache::default();
        let key = PersistentFactKey {
            fact_kind: SOURCE_ENDPOINT_KIND,
            fact_id: id(1),
        };
        let outer = PersistentFactTree::insert(None, key, b"short", &mut cache);
        assert!(outer.is_ok());
        let Some(outer) = outer.ok() else {
            return;
        };
        assert!(
            PersistentIncidenceIndex::endpoint_root(Some(outer), NodeId(id(1)), true, &cache)
                .is_err()
        );
    }

    #[test]
    fn adding_one_edge_to_a_high_degree_node_path_copies_pages_not_its_adjacency() {
        let mut cache = PersistentFactTreeCache::default();
        let existing = (2..=129)
            .map(|target| change(target, 1, target, true))
            .collect::<Vec<_>>();
        let first = insert_all(&mut cache, None, &existing);
        assert!(first.is_some());
        let Some((first, nested_roots)) = first else {
            return;
        };
        let first_pages =
            cache.take_reachable_dirty_roots(std::iter::once(first).chain(nested_roots));
        assert!(!first_pages.is_empty());

        let mut second_cache = PersistentFactTreeCache::default();
        let mut apply = PersistentIncidenceApply::new(Some(first), &[change(130, 1, 130, true)]);
        let mut missing_page_count = 0_usize;
        let completed = loop {
            match apply.advance(&mut second_cache) {
                Ok(true) => {}
                Ok(false) => break true,
                Err(PersistentFactTreeError::MissingPage(id)) => {
                    let Some((_, page)) = first_pages.iter().find(|(page_id, _)| *page_id == id)
                    else {
                        break false;
                    };
                    if second_cache.insert_loaded(id, page.clone()).is_err() {
                        break false;
                    }
                    missing_page_count = missing_page_count.saturating_add(1);
                }
                Err(PersistentFactTreeError::CorruptPage(_)) => break false,
            }
        };
        assert!(
            completed,
            "incidence mutation did not resume after lazy loads"
        );
        assert!(
            missing_page_count > 0,
            "test did not exercise lazy page loading"
        );
        let Some(second) = apply.root() else {
            return;
        };
        let changed_pages = second_cache.take_reachable_dirty_roots(apply.dirty_roots());
        assert!(!changed_pages.is_empty());
        assert!(
            changed_pages.len() < 64,
            "one edge insertion rewrote {} pages for a 128-edge endpoint",
            changed_pages.len()
        );

        let mut reopened_cache = PersistentFactTreeCache::default();
        for (page_id, page) in first_pages.iter().chain(changed_pages.iter()) {
            assert!(reopened_cache.insert_loaded(*page_id, page.clone()).is_ok());
        }
        let old_outgoing = PersistentIncidenceIndex::endpoint_root(
            Some(first),
            NodeId(id(1)),
            true,
            &reopened_cache,
        )
        .ok()
        .flatten();
        let new_outgoing = PersistentIncidenceIndex::endpoint_root(
            Some(second),
            NodeId(id(1)),
            true,
            &reopened_cache,
        )
        .ok()
        .flatten();
        assert_eq!(
            PersistentIncidenceIndex::edge_ids_page(old_outgoing, None, 512, &reopened_cache)
                .map(|edges| edges.len()),
            Ok(128)
        );
        assert_eq!(
            PersistentIncidenceIndex::edge_ids_page(new_outgoing, None, 512, &reopened_cache)
                .map(|edges| edges.len()),
            Ok(129)
        );
    }
}
