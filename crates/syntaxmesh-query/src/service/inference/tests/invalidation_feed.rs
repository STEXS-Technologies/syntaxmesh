use super::*;
use syntaxmesh_core::{
    AcceptanceTime, Edge, FileId, FileVersion, GenerationHistoryEntry, GenerationId,
    GenerationManifest, GenerationStatus, GraphDelta, Node, NodeId, Provenance, RepositoryId,
    WorktreeId,
};
use syntaxmesh_inference::{ClaimSelection, InvalidationLimits, PremiseDependencyIndex};
use syntaxmesh_store::{
    FactVersionChangeCursor, FactVersionChangePage, GenerationChangeCursor, GenerationChangePage,
    StoreError,
};

struct FeedOnly<'store> {
    store: &'store InMemoryGraphStore,
    fault: Option<u8>,
}

macro_rules! unsupported {
    ($name:ident($($argument:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&self, $($argument: $ty),*) -> Result<$result, StoreError> {
            $(let _ = $argument;)*
            Err(StoreError::Integrity(stringify!($name).to_owned()))
        }
    };
}

impl GraphStore for FeedOnly<'_> {
    unsupported!(current_generation(repository: RepositoryId, worktree: WorktreeId) -> Option<GenerationManifest>);
    unsupported!(generation_history() -> Vec<GenerationHistoryEntry>);
    unsupported!(acceptance_time(generation: GenerationId) -> Option<AcceptanceTime>);
    unsupported!(node(generation: GenerationId, id: NodeId) -> Option<Node>);
    unsupported!(nodes(generation: GenerationId) -> Vec<Node>);
    unsupported!(files(generation: GenerationId) -> Vec<FileVersion>);
    unsupported!(edges(generation: GenerationId) -> Vec<Edge>);
    unsupported!(provenance(generation: GenerationId) -> Vec<Provenance>);
    unsupported!(outgoing(generation: GenerationId, id: NodeId) -> Vec<Edge>);
    unsupported!(nodes_for_file(generation: GenerationId, file: FileId) -> Vec<NodeId>);

    fn apply_delta(&mut self, _delta: GraphDelta) -> Result<GenerationManifest, StoreError> {
        Err(StoreError::Integrity("unexpected write".to_owned()))
    }
    fn apply_delta_with_acceptance_time(
        &mut self,
        _delta: GraphDelta,
        _accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        Err(StoreError::Integrity("unexpected write".to_owned()))
    }
    fn set_generation_status(
        &mut self,
        _generation: GenerationId,
        _status: GenerationStatus,
    ) -> Result<GenerationManifest, StoreError> {
        Err(StoreError::Integrity("unexpected write".to_owned()))
    }
    fn manifest(&self, generation: GenerationId) -> Result<GenerationManifest, StoreError> {
        self.store.manifest(generation)
    }
    fn generation_sequence(&self, generation: GenerationId) -> Result<u64, StoreError> {
        self.store.generation_sequence(generation)
    }
    fn fact_versions_changed_at(
        &self,
        fact: FactRef,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_store::FactHistoryEntry>, StoreError> {
        let mut versions = self.store.fact_versions_changed_at(fact, generation)?;
        for entry in &mut versions {
            if entry.version.valid_from != generation {
                continue;
            }
            match self.fault {
                Some(11) => entry.valid_from_sequence = entry.valid_from_sequence.saturating_add(1),
                Some(12) if entry.valid_until_sequence.is_some() => {
                    entry.valid_until_sequence = entry
                        .valid_until_sequence
                        .map(|sequence| sequence.saturating_add(1));
                }
                Some(_) | None => {}
            }
        }
        Ok(versions)
    }

    fn changes_between_page(
        &self,
        from: GenerationId,
        to: GenerationId,
        after: Option<GenerationChangeCursor>,
        limit: usize,
    ) -> Result<GenerationChangePage, StoreError> {
        let mut page = self.store.changes_between_page(from, to, after, limit)?;
        if after.is_none() {
            let first = page
                .items
                .first_mut()
                .ok_or_else(|| StoreError::Integrity("empty fixture".to_owned()))?;
            match self.fault {
                Some(0) => first.sequence = 0,
                Some(1) => first.delta.expected_base = None,
                Some(2) => first.delta.repository = RepositoryId::derive(&[b"foreign"]),
                Some(3) => first.manifest.worktree = WorktreeId::derive(&[b"foreign"]),
                Some(4) => {
                    page.next_cursor = Some(GenerationChangeCursor {
                        from_generation: GenerationId::derive(&[b"foreign"]),
                        to_generation: to,
                        after_sequence: first.sequence,
                        after_generation: first.manifest.generation,
                    })
                }
                Some(5) => page.next_cursor = None,
                Some(14) => first.manifest.parent = None,
                Some(15) => first.manifest.graph_root = [255; 32],
                Some(_) | None => {}
            }
        }
        Ok(page)
    }
    fn fact_version_changes_at_page(
        &self,
        generation: GenerationId,
        after: Option<FactVersionChangeCursor>,
        limit: usize,
    ) -> Result<FactVersionChangePage, StoreError> {
        let mut page = self
            .store
            .fact_version_changes_at_page(generation, after, limit)?;
        if let Some(first) = page.items.first_mut() {
            match self.fault {
                Some(6) => first.fact = FactRef::Node(NodeId::derive(&[b"foreign"])),
                Some(7) => first.valid_from_sequence = 0,
                Some(8) => first.valid_until_sequence = None,
                Some(9) => {
                    let duplicate = first.clone();
                    page.items.push(duplicate);
                }
                Some(10) => {
                    page.next_cursor = Some(FactVersionChangeCursor {
                        generation: GenerationId::derive(&[b"foreign"]),
                        after_fact: first.fact,
                        after_valid_from_sequence: first.valid_from_sequence,
                    })
                }
                Some(13) if first.version.valid_until == Some(generation) => {
                    first.version.valid_from = GenerationId::derive(&[b"substituted-opening"]);
                }
                Some(_) | None => {}
            }
        }
        Ok(page)
    }
}

#[test]
fn invalidation_uses_only_pinned_feeds_and_rejects_substituted_pages() -> Result<(), Box<dyn Error>>
{
    let mut store = InMemoryGraphStore::new();
    let (rule, retained, current) = fixture(&mut store)?;
    let claims = Query::new(&store, retained).infer(&rule, &limits(), 100)?;
    let manifest = store.manifest(retained)?;
    fixture::verify_restoration(&mut store, &rule, retained, current)?;
    let final_generation = store
        .current_generation(manifest.repository, manifest.worktree)?
        .ok_or("missing final generation")?
        .generation;
    let index = PremiseDependencyIndex::build_scoped(
        ClaimSelection {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: retained,
        },
        &claims,
        10,
        100,
    )?;
    let bounds = InvalidationLimits {
        generations: 10,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let valid = FeedOnly {
        store: &store,
        fault: None,
    };
    let claim = claims.first().ok_or("missing fixture claim")?;
    Query::new(&valid, retained).verify_inference_claim(claim, &rule, 10, 2)?;
    for fault in [11, 12] {
        let malformed = FeedOnly {
            store: &store,
            fault: Some(fault),
        };
        if Query::new(&malformed, retained)
            .inference_premises(claim, 10)
            .is_ok()
        {
            return Err(format!("accepted mismatched premise sequence case {fault}").into());
        }
    }
    let foreign_index = PremiseDependencyIndex::build_scoped(
        ClaimSelection {
            repository: RepositoryId::derive(&[b"foreign-index"]),
            worktree: manifest.worktree,
            generation: retained,
        },
        &[],
        1,
        1,
    )?;
    let later_index = PremiseDependencyIndex::build_scoped(
        ClaimSelection {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: final_generation,
        },
        &[],
        1,
        1,
    )?;
    if Query::new(&valid, final_generation)
        .inference_invalidations(&foreign_index, retained, &bounds)
        .is_ok()
        || Query::new(&valid, retained)
            .inference_invalidations(&later_index, final_generation, &bounds)
            .is_ok()
    {
        return Err("invalidation accepted foreign index or reversed generation range".into());
    }
    if Query::new(&valid, final_generation).inference_invalidations(&index, retained, &bounds)?
        != claims
            .iter()
            .map(|retained_claim| retained_claim.id)
            .collect()
    {
        return Err("feed-only invalidation disagreed with retained claims".into());
    }
    for fault in (0..11).chain([13, 14, 15]) {
        let malformed = FeedOnly {
            store: &store,
            fault: Some(fault),
        };
        if Query::new(&malformed, final_generation)
            .inference_invalidations(&index, retained, &bounds)
            .is_ok()
            || Query::new(&malformed, final_generation)
                .inference_invalidation_explanations(&index, retained, &bounds, 100)
                .is_ok()
            || Query::new(&malformed, final_generation)
                .inference_premise_closures(retained, &bounds)
                .is_ok()
            || Query::new(&malformed, final_generation)
                .inference_premise_transitions(retained, bounds.generations, bounds.versions)
                .is_ok()
        {
            return Err(format!("accepted malformed feed case {fault}").into());
        }
    }
    Ok(())
}
