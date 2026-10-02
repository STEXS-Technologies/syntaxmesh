#![allow(clippy::panic_in_result_fn)]

use super::{LexicalCandidateRequest, LexicalChannel, LexicalFamily};
use crate::{GenerationIdentifierIndex, QueryError, RankedCandidate};
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{GenerationId, Node, NodeId, NodeKind};
use syntaxmesh_store::GraphStore;

pub(in crate::identifier_index) fn oracle(
    nodes: &[Node],
    request: &LexicalCandidateRequest<'_>,
) -> Vec<RankedCandidate> {
    let normalize = |text: &str| {
        let terms = crate::identifier_terms(text);
        if request.channel == LexicalChannel::Exact {
            return terms;
        }
        let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
        terms
            .iter()
            .map(|term| stemmer.stem(term).into_owned())
            .collect::<BTreeSet<_>>()
    };
    let documents = nodes
        .iter()
        .map(|node| (node, normalize(&node.name)))
        .collect::<Vec<_>>();
    let terms = normalize(request.query);
    let weights = terms
        .iter()
        .map(|term| {
            let frequency = documents
                .iter()
                .filter(|(_, words)| words.contains(term))
                .count();
            let mut ratio = nodes
                .len()
                .checked_div(frequency.saturating_add(1))
                .unwrap_or(0)
                .saturating_add(1);
            let mut weight = 1_u64;
            while ratio > 1 {
                ratio /= 2;
                weight = weight.saturating_add(1);
            }
            (term, weight)
        })
        .collect::<BTreeMap<_, _>>();
    let mut ranked = documents
        .into_iter()
        .filter_map(|(node, words)| {
            let matched = weights
                .iter()
                .filter(|(term, _)| words.contains(**term))
                .collect::<Vec<_>>();
            if matched.is_empty()
                || request
                    .family
                    .is_some_and(|family| LexicalFamily::of(&node.kind) != family)
                || request.eligible.is_some_and(|ids| !ids.contains(&node.id))
            {
                return None;
            }
            let coverage = u64::try_from(matched.len()).ok()?;
            Some(RankedCandidate {
                node: node.clone(),
                score: matched
                    .iter()
                    .map(|(_, weight)| **weight)
                    .sum::<u64>()
                    .saturating_mul(coverage),
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.node.id.cmp(&right.node.id))
    });
    ranked.truncate(request.limit);
    ranked
}

pub(in crate::identifier_index) fn fixture() -> Result<
    (
        syntaxmesh_store::InMemoryGraphStore,
        syntaxmesh_core::GraphDelta,
    ),
    QueryError,
> {
    let (mut store, mut delta) = super::super::tests::path_fixture()?;
    let template = delta
        .upsert_nodes
        .first()
        .cloned()
        .ok_or(QueryError::InvalidLimit)?;
    delta.expected_base = Some(delta.next_generation);
    delta.next_generation = GenerationId::derive(&[b"lexical-planner"]);
    delta.remove_nodes = vec![template.id];
    delta.upsert_nodes = (0_u64..90)
        .map(|ordinal| {
            let mut node = template.clone();
            node.id = NodeId::derive(&[b"lexical-planner-node", &ordinal.to_le_bytes()]);
            node.kind = if ordinal < 9 {
                NodeKind::Function
            } else {
                NodeKind::DocumentChunk
            };
            node.name = match ordinal {
                0..=7 => "alpha".to_owned(),
                8 => "beta execute execution executing".to_owned(),
                _ => "beta executing".to_owned(),
            };
            node
        })
        .collect();
    store.apply_delta(delta.clone())?;
    Ok((store, delta))
}

#[test]
fn indexed_channels_match_global_oracle_before_family_and_selected_set_cutoffs()
-> Result<(), QueryError> {
    let (store, delta) = fixture()?;
    let generation = delta.next_generation;
    let index =
        GenerationIdentifierIndex::build_for_planner(&store, generation, 90, 1000, 100_000)?;
    let eligible = delta
        .upsert_nodes
        .iter()
        .take(12)
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    let exhausted = LexicalCandidateRequest {
        query: "beta",
        channel: LexicalChannel::Stemmed,
        family: Some(LexicalFamily::Reference),
        eligible: None,
        posting_budget: 1,
        limit: 1,
    };
    assert!(
        index
            .lexical_candidates(&store, generation, &exhausted)
            .is_err()
    );
    for channel in [LexicalChannel::Exact, LexicalChannel::Stemmed] {
        for family in [
            None,
            Some(LexicalFamily::Code),
            Some(LexicalFamily::Documentation),
            Some(LexicalFamily::Reference),
            Some(LexicalFamily::Other),
        ] {
            for restriction in [None, Some(&eligible)] {
                for query in ["alpha beta", "execution execute executing", "absent", ""] {
                    for limit in [1, 3, 256] {
                        let request = LexicalCandidateRequest {
                            query,
                            channel,
                            family,
                            eligible: restriction,
                            posting_budget: 1000,
                            limit,
                        };
                        assert_eq!(
                            index.lexical_candidates(&store, generation, &request)?,
                            oracle(&delta.upsert_nodes, &request)
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn planner_capability_budget_and_eligible_validation_are_explicit() -> Result<(), QueryError> {
    let (store, delta) = super::super::tests::path_fixture()?;
    let generation = delta.next_generation;
    // "load": two separate channel postings (36 bytes each), plus ID/family (33).
    let index = GenerationIdentifierIndex::build_for_planner(&store, generation, 1, 2, 105)?;
    for (nodes, postings, bytes) in [(0, 2, 105), (1, 1, 105), (1, 2, 104)] {
        assert!(
            GenerationIdentifierIndex::build_for_planner(
                &store, generation, nodes, postings, bytes
            )
            .is_err()
        );
    }
    let mut request = LexicalCandidateRequest {
        query: "load",
        channel: LexicalChannel::Stemmed,
        family: Some(LexicalFamily::Code),
        eligible: None,
        posting_budget: 1,
        limit: 1,
    };
    assert_eq!(
        index
            .lexical_candidates(&store, generation, &request)?
            .len(),
        1
    );
    let plain = GenerationIdentifierIndex::build(&store, generation, 1, 1, 36)?;
    assert!(
        plain
            .lexical_candidates(&store, generation, &request)
            .is_err()
    );
    assert!(
        index
            .lexical_candidates(&store, GenerationId::derive(&[b"foreign"]), &request)
            .is_err()
    );
    for limit in [0, 257] {
        request.limit = limit;
        assert!(
            index
                .lexical_candidates(&store, generation, &request)
                .is_err()
        );
    }
    request.limit = 1;
    request.posting_budget = 0;
    assert!(
        index
            .lexical_candidates(&store, generation, &request)
            .is_err()
    );
    request.posting_budget = 1;
    let foreign = BTreeSet::from([NodeId::derive(&[b"foreign"])]);
    request.eligible = Some(&foreign);
    assert!(
        index
            .lexical_candidates(&store, generation, &request)
            .is_err()
    );
    let empty = BTreeSet::new();
    request.eligible = Some(&empty);
    assert!(
        index
            .lexical_candidates(&store, generation, &request)?
            .is_empty()
    );
    Ok(())
}
