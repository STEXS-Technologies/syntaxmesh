//! Reuse the fail-on-unexpected-port-operation pattern from context graph tests.
use crate::{GenerationIdentifierIndex, QueryError};
use std::cell::{Cell, RefCell};
use syntaxmesh_core::{
    AcceptanceTime, Edge, FileId, FileVersion, GenerationHistoryEntry, GenerationId,
    GenerationManifest, GenerationStatus, GraphDelta, Node, NodeId, Provenance, RepositoryId,
    WorktreeId,
};
use syntaxmesh_store::{GraphStore, StoreError};

struct SelectedOnlyStore<'store> {
    canonical: &'store dyn GraphStore,
    manifest_reads: Cell<usize>,
    batches: RefCell<Vec<Vec<NodeId>>>,
}

#[test]
fn query_service_delegates_indexed_plans_without_hydration() -> Result<(), QueryError> {
    let (canonical, delta) = super::path_fixture()?;
    let generation = delta.next_generation;
    let query = crate::Query::new(&canonical, generation);
    let index = query.build_planner_identifier_index(1, 2, 105)?;
    let restricted = SelectedOnlyStore {
        canonical: &canonical,
        manifest_reads: Cell::new(0),
        batches: RefCell::new(Vec::new()),
    };
    let service = crate::Query::new(&restricted, generation);
    let request = crate::LexicalPlanRequest {
        query: "load",
        posting_budget: 8,
    };
    let expected = index.lexical_seed_plan(&canonical, generation, &request)?;
    let seeds = service.lexical_seed_plan(&index, &request)?;
    super::check(seeds == expected, "Query changed indexed seed order")?;
    let selected = seeds.iter().copied().collect();
    super::check(
        service.lexical_packing_plan(&index, &request, &selected, &selected)?
            == index
                .lexical_packing_plan(&canonical, generation, &request, &selected, &selected)?,
        "Query changed indexed packing order",
    )?;
    super::check(
        restricted.batches.borrow().is_empty(),
        "Query hydrated plan payloads",
    )?;
    super::check(
        restricted.manifest_reads.get() == 4,
        "Query changed manifest validation",
    )?;
    let exhausted = crate::LexicalPlanRequest {
        query: "load",
        posting_budget: 7,
    };
    super::check(
        service.lexical_seed_plan(&index, &exhausted).is_err(),
        "Query bypassed aggregate budget",
    )?;
    super::check(
        query.build_planner_identifier_index(1, 2, 104).is_err(),
        "Query bypassed build budget",
    )?;
    Ok(())
}

#[test]
fn planner_queries_hydrate_only_selected_ids_without_enumerating_canonical_nodes()
-> Result<(), QueryError> {
    let (canonical, delta) = super::path_fixture()?;
    let generation = delta.next_generation;
    let index = GenerationIdentifierIndex::build_for_planner(&canonical, generation, 1, 2, 105)?;
    let selected_only = SelectedOnlyStore {
        canonical: &canonical,
        manifest_reads: Cell::new(0),
        batches: RefCell::new(Vec::new()),
    };
    for channel in [crate::LexicalChannel::Exact, crate::LexicalChannel::Stemmed] {
        for query in ["load", "absent"] {
            let request = crate::LexicalCandidateRequest {
                query,
                channel,
                family: Some(crate::LexicalFamily::Code),
                eligible: None,
                posting_budget: 1,
                limit: 1,
            };
            let expected = index.lexical_candidates(&canonical, generation, &request)?;
            super::check(
                index.lexical_candidates(&selected_only, generation, &request)? == expected,
                "restricted planner port changed ranking",
            )?;
            super::check(
                selected_only.batches.borrow().last()
                    == Some(
                        &expected
                            .iter()
                            .map(|entry| entry.node.id)
                            .collect::<Vec<_>>(),
                    ),
                "planner hydrated unselected IDs",
            )?;
        }
    }
    super::check(
        selected_only.manifest_reads.get() == 4,
        "unexpected planner manifest reads",
    )?;
    super::check(
        selected_only.batches.borrow().len() == 4,
        "unexpected planner hydration batches",
    )?;
    Ok(())
}

macro_rules! unsupported {
    ($name:ident($($argument:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&self, $($argument: $ty),*) -> Result<$result, StoreError> {
            $(let _ = $argument;)*
            Err(StoreError::Integrity(stringify!($name).to_owned()))
        }
    };
}

#[test]
fn indexed_plan_composition_requires_manifest_reads_but_no_payload_hydration()
-> Result<(), QueryError> {
    let (canonical, delta) = super::path_fixture()?;
    let generation = delta.next_generation;
    let index = GenerationIdentifierIndex::build_for_planner(&canonical, generation, 1, 2, 105)?;
    let selected_only = SelectedOnlyStore {
        canonical: &canonical,
        manifest_reads: Cell::new(0),
        batches: RefCell::new(Vec::new()),
    };
    let request = crate::LexicalPlanRequest {
        query: "load",
        posting_budget: 8,
    };
    let seeds = index.lexical_seed_plan(&selected_only, generation, &request)?;
    let selected = delta
        .upsert_nodes
        .iter()
        .map(|node| node.id)
        .collect::<std::collections::BTreeSet<_>>();
    super::check(
        seeds
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            == selected,
        "indexed seed composition changed IDs",
    )?;
    let plan =
        index.lexical_packing_plan(&selected_only, generation, &request, &selected, &selected)?;
    super::check(
        plan == seeds,
        "indexed packing composition changed singleton order",
    )?;
    super::check(
        selected_only.batches.borrow().is_empty(),
        "indexed composition hydrated canonical payloads",
    )?;
    super::check(
        selected_only.manifest_reads.get() == 4,
        "unexpected composition manifest reads",
    )?;
    Ok(())
}

impl GraphStore for SelectedOnlyStore<'_> {
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

    fn visit_historical_nodes(
        &self,
        generation: GenerationId,
        max_nodes: usize,
        visitor: &mut dyn FnMut(Node) -> Result<(), StoreError>,
    ) -> Result<usize, StoreError> {
        self.canonical
            .visit_historical_nodes(generation, max_nodes, visitor)
    }

    fn manifest(&self, generation: GenerationId) -> Result<GenerationManifest, StoreError> {
        self.manifest_reads
            .set(self.manifest_reads.get().saturating_add(1));
        self.canonical.manifest(generation)
    }

    fn historical_nodes_by_ids(
        &self,
        generation: GenerationId,
        ids: &[NodeId],
    ) -> Result<Vec<Node>, StoreError> {
        self.batches.borrow_mut().push(ids.to_vec());
        self.canonical.historical_nodes_by_ids(generation, ids)
    }

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
}

#[test]
fn index_build_uses_visitor_without_page_or_snapshot_reads() -> Result<(), QueryError> {
    let (canonical, delta) = super::path_fixture()?;
    let restricted = SelectedOnlyStore {
        canonical: &canonical,
        manifest_reads: Cell::new(0),
        batches: RefCell::new(Vec::new()),
    };
    GenerationIdentifierIndex::build(&restricted, delta.next_generation, 1, 1, 36)?;
    GenerationIdentifierIndex::build_for_planner(&restricted, delta.next_generation, 1, 2, 105)?;
    super::check(
        restricted.manifest_reads.get() == 2 && restricted.batches.borrow().is_empty(),
        "visitor builds used unexpected manifest or hydration calls",
    )?;
    let error = GenerationIdentifierIndex::build(&restricted, delta.next_generation, 1, 1, 35);
    super::check(
        matches!(error, Err(QueryError::Context(ref message)) if message == "identifier index payload budget exhausted"),
        "visitor replaced the original payload-budget error",
    )?;
    Ok(())
}

#[test]
fn path_candidates_require_only_manifest_and_selected_id_hydration() -> Result<(), QueryError> {
    let (canonical, delta) = super::path_fixture()?;
    let generation = delta.next_generation;
    let index =
        GenerationIdentifierIndex::build_with_paths(&canonical, generation, 1, 3, 109, 1, 47)?;
    let selected_only = SelectedOnlyStore {
        canonical: &canonical,
        manifest_reads: Cell::new(0),
        batches: RefCell::new(Vec::new()),
    };
    for query in ["scripts load", "load", "absent"] {
        let expected = index.candidates(&canonical, generation, query, 3, 1)?;
        let actual = index.candidates(&selected_only, generation, query, 3, 1)?;
        super::check(
            actual == expected,
            "restricted-port query changed candidates",
        )?;
        let expected_ids = expected
            .iter()
            .map(|entry| entry.node.id)
            .collect::<Vec<_>>();
        super::check(
            selected_only.batches.borrow().last() == Some(&expected_ids),
            "hydration requested unselected IDs",
        )?;
    }
    super::check(
        selected_only.manifest_reads.get() == 3 && selected_only.batches.borrow().len() == 3,
        "unexpected manifest or hydration call count",
    )?;
    super::check(
        index
            .candidates(&selected_only, generation, "scripts load py", 2, 1)
            .is_err(),
        "posting budget did not reject exhausted work",
    )?;
    super::check(
        selected_only.batches.borrow().len() == 3,
        "exhausted posting budget hydrated candidates",
    )?;
    Ok(())
}
