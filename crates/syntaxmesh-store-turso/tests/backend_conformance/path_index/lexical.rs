use super::{GenerationId, GraphStore, RankedCandidate, StoreError};
use std::collections::BTreeSet;
use syntaxmesh_core::NodeId;
use syntaxmesh_query::{
    GenerationIdentifierIndex, LexicalCandidateRequest, LexicalChannel, LexicalFamily,
    LexicalPlanRequest,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct LexicalRows {
    rankings: Vec<Vec<RankedCandidate>>,
    seeds: Vec<Vec<NodeId>>,
    plans: Vec<Vec<NodeId>>,
}

impl LexicalRows {
    pub(super) fn is_empty(&self) -> bool {
        self.rankings.iter().all(Vec::is_empty)
    }
}

pub(super) fn index(
    store: &dyn GraphStore,
    generation: GenerationId,
) -> Result<GenerationIdentifierIndex, StoreError> {
    GenerationIdentifierIndex::build_for_planner(store, generation, 2, 100, 10_000)
        .map_err(|error| StoreError::Backend(error.to_string()))
}

pub(super) fn rows(
    index: &GenerationIdentifierIndex,
    store: &dyn GraphStore,
    generation: GenerationId,
) -> Result<LexicalRows, StoreError> {
    let eligible = BTreeSet::from([NodeId::derive(&[b"context-conformance-helper"])]);
    let mut result = Vec::new();
    for channel in [LexicalChannel::Exact, LexicalChannel::Stemmed] {
        for family in [
            None,
            Some(LexicalFamily::Code),
            Some(LexicalFamily::Documentation),
            Some(LexicalFamily::Reference),
            Some(LexicalFamily::Other),
        ] {
            for restriction in [None, Some(&eligible)] {
                for query in [
                    "caller helper",
                    "callers helpers",
                    "revised",
                    "revisions",
                    "absent",
                ] {
                    result.push(
                        index
                            .lexical_candidates(
                                store,
                                generation,
                                &LexicalCandidateRequest {
                                    query,
                                    channel,
                                    family,
                                    eligible: restriction,
                                    posting_budget: 100,
                                    limit: 2,
                                },
                            )
                            .map_err(|error| StoreError::Backend(error.to_string()))?,
                    );
                }
            }
        }
    }
    let selected = BTreeSet::from([
        NodeId::derive(&[b"context-conformance-caller"]),
        NodeId::derive(&[b"context-conformance-helper"]),
    ]);
    let mut seeds = Vec::new();
    let mut plans = Vec::new();
    for query in [
        "caller helper",
        "callers helpers",
        "revised",
        "revisions",
        "absent",
    ] {
        let request = LexicalPlanRequest {
            query,
            posting_budget: 100,
        };
        let original = index
            .lexical_seed_plan(store, generation, &request)
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let seed_set = original.iter().copied().collect::<BTreeSet<_>>();
        let plan = index
            .lexical_packing_plan(store, generation, &request, &selected, &seed_set)
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        if plan.len() != selected.len() || plan.iter().copied().collect::<BTreeSet<_>>() != selected
        {
            return Err(StoreError::Integrity(
                "indexed temporal composer lost selected membership".to_owned(),
            ));
        }
        seeds.push(original);
        plans.push(plan);
    }
    Ok(LexicalRows {
        rankings: result,
        seeds,
        plans,
    })
}

pub(super) fn verify(
    store: &dyn GraphStore,
    generation: GenerationId,
    expected: &LexicalRows,
) -> Result<(), StoreError> {
    if &rows(&index(store, generation)?, store, generation)? != expected {
        return Err(StoreError::Integrity(
            "lexical planner changed retained ranking or canonical payloads".to_owned(),
        ));
    }
    Ok(())
}
