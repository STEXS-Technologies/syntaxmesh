#![allow(clippy::panic_in_result_fn)]

use super::LexicalPlanRequest;
use crate::{
    GenerationIdentifierIndex, LexicalCandidateRequest, LexicalChannel, LexicalFamily, QueryError,
};
use std::collections::{BTreeSet, VecDeque};
use syntaxmesh_core::{FileId, Node, NodeId};
use syntaxmesh_store::GraphStore;

#[test]
fn positive_role_ids_are_budgeted_and_malformed_roles_fail_closed() -> Result<(), QueryError> {
    let (_, delta) = super::super::tests::path_fixture()?;
    let mut node = delta
        .upsert_nodes
        .first()
        .cloned()
        .ok_or(QueryError::InvalidLimit)?;
    node.extension_payload = syntaxmesh_language_sdk::SourceRole::TestIntent.payload();
    for limit in [64, 65] {
        let mut data = super::super::lexical::PlannerData::default();
        let mut count = 0;
        let mut bytes = 0;
        let result = data.insert(&node, &BTreeSet::new(), &mut count, &mut bytes, 1, limit);
        if limit == 64 {
            assert!(result.is_err());
        } else {
            result?;
            assert_eq!(bytes, 65);
            assert!(data.test_intent.contains(&node.id));
        }
    }
    if let Some(payload) = &mut node.extension_payload {
        payload.schema_version = 2;
    }
    let mut data = super::super::lexical::PlannerData::default();
    let mut count = 0;
    let mut bytes = 0;
    assert!(
        data.insert(&node, &BTreeSet::new(), &mut count, &mut bytes, 1, 2000)
            .is_err()
    );
    Ok(())
}

#[test]
fn explicit_role_order_is_stable_complete_and_neutral_by_default() -> Result<(), QueryError> {
    let (mut store, mut delta) = super::super::tests::path_fixture()?;
    let template = delta
        .upsert_nodes
        .first()
        .cloned()
        .ok_or(QueryError::InvalidLimit)?;
    delta.expected_base = Some(delta.next_generation);
    delta.next_generation = syntaxmesh_core::GenerationId::derive(&[b"role-plan"]);
    delta.remove_nodes = vec![template.id];
    delta.upsert_nodes = (0_u64..4)
        .map(|ordinal| Node {
            id: NodeId::derive(&[b"role-plan-node", &ordinal.to_le_bytes()]),
            name: "load".to_owned(),
            extension_payload: if ordinal == 1 {
                syntaxmesh_language_sdk::SourceRole::TestIntent.payload()
            } else {
                None
            },
            ..template.clone()
        })
        .collect();
    let target = delta
        .upsert_nodes
        .get(1)
        .ok_or(QueryError::InvalidLimit)?
        .id;
    store.apply_delta(delta.clone())?;
    let generation = delta.next_generation;
    let index = GenerationIdentifierIndex::build_for_planner(&store, generation, 4, 100, 2000)?;
    let selected = delta
        .upsert_nodes
        .iter()
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    let original = BTreeSet::new();
    let request = LexicalPlanRequest {
        query: "load",
        posting_budget: 100,
    };
    let neutral = index.lexical_packing_plan(&store, generation, &request, &selected, &original)?;
    for preference in [
        super::SourceRolePreference::Neutral,
        super::SourceRolePreference::TestIntentFirst,
        super::SourceRolePreference::TestIntentLast,
    ] {
        let actual = index.lexical_packing_plan_with_roles(
            &store,
            generation,
            &request,
            &super::SourceRolePackingRequest {
                selected: &selected,
                original: &original,
                preference,
            },
        )?;
        let mut expected = neutral.clone();
        match preference {
            super::SourceRolePreference::Neutral => {}
            super::SourceRolePreference::TestIntentFirst => {
                expected.sort_by_key(|id| *id != target)
            }
            super::SourceRolePreference::TestIntentLast => expected.sort_by_key(|id| *id == target),
        }
        assert_eq!(actual, expected);
        assert_eq!(actual.iter().copied().collect::<BTreeSet<_>>(), selected);
        // Every original-seed subset, including empty and complete, must retain
        // stable role order within both discovery partitions.
        for mask in 0_u32..16 {
            let seeds = delta
                .upsert_nodes
                .iter()
                .zip([1_u32, 2, 4, 8])
                .filter(|(_, bit)| mask & bit != 0)
                .map(|(node, _)| node.id)
                .collect::<BTreeSet<_>>();
            let roles = super::SourceRolePackingRequest {
                selected: &selected,
                original: &seeds,
                preference,
            };
            let baseline =
                index.lexical_packing_plan_with_roles(&store, generation, &request, &roles)?;
            let balanced = index.lexical_packing_plan_with_discovery(
                &store,
                generation,
                &request,
                &roles,
                super::DiscoveryPackingPreference::default(),
            )?;
            assert_eq!(balanced, baseline);
            let lexical_first = index.lexical_packing_plan_with_discovery(
                &store,
                generation,
                &request,
                &roles,
                super::DiscoveryPackingPreference::LexicalFirst,
            )?;
            let stable_partition = baseline
                .iter()
                .filter(|id| seeds.contains(id))
                .chain(baseline.iter().filter(|id| !seeds.contains(id)))
                .copied()
                .collect::<Vec<_>>();
            assert_eq!(lexical_first, stable_partition);
            assert_eq!(
                lexical_first.iter().copied().collect::<BTreeSet<_>>(),
                selected
            );
        }
    }
    Ok(())
}

#[test]
fn family_partition_retains_matches_beyond_global_top_256() -> Result<(), QueryError> {
    let (mut store, mut delta) = super::super::tests::path_fixture()?;
    let template = delta
        .upsert_nodes
        .first()
        .cloned()
        .ok_or(QueryError::InvalidLimit)?;
    delta.expected_base = Some(delta.next_generation);
    delta.next_generation = syntaxmesh_core::GenerationId::derive(&[b"crowded-family"]);
    delta.remove_nodes = vec![template.id];
    delta.upsert_nodes = (0_u64..300)
        .map(|ordinal| Node {
            id: NodeId::derive(&[b"crowded-code", &ordinal.to_le_bytes()]),
            name: "load alpha".to_owned(),
            kind: syntaxmesh_core::NodeKind::Function,
            ..template.clone()
        })
        .collect();
    let document = Node {
        id: NodeId::derive(&[b"uncrowded-document"]),
        name: "load".to_owned(),
        kind: syntaxmesh_core::NodeKind::Section,
        ..template
    };
    delta.upsert_nodes.push(document.clone());
    store.apply_delta(delta.clone())?;
    let generation = delta.next_generation;
    let index =
        GenerationIdentifierIndex::build_for_planner(&store, generation, 301, 2000, 100_000)?;
    let request = LexicalPlanRequest {
        query: "load alpha",
        posting_budget: 10_000,
    };
    let actual = index.lexical_seed_plan(&store, generation, &request)?;
    assert_eq!(
        actual,
        complete_oracle(&delta.upsert_nodes, request.query, None, 32)
    );
    assert!(actual.contains(&document.id));
    let global = index.lexical_candidates(
        &store,
        generation,
        &LexicalCandidateRequest {
            query: request.query,
            channel: LexicalChannel::Exact,
            family: None,
            eligible: None,
            posting_budget: 10_000,
            limit: 256,
        },
    )?;
    assert!(
        !global
            .iter()
            .any(|candidate| candidate.node.id == document.id)
    );
    Ok(())
}

#[test]
fn complete_plan_capacity_accepts_256_known_ids_and_rejects_257_and_33_originals()
-> Result<(), QueryError> {
    let (mut store, mut delta) = super::super::tests::path_fixture()?;
    let template = delta
        .upsert_nodes
        .first()
        .cloned()
        .ok_or(QueryError::InvalidLimit)?;
    delta.expected_base = Some(delta.next_generation);
    delta.next_generation = syntaxmesh_core::GenerationId::derive(&[b"plan-capacity"]);
    delta.remove_nodes = vec![template.id];
    delta.upsert_nodes = (0_u64..257)
        .map(|ordinal| Node {
            id: NodeId::derive(&[b"plan-capacity-node", &ordinal.to_le_bytes()]),
            name: String::new(),
            ..template.clone()
        })
        .collect();
    store.apply_delta(delta.clone())?;
    let generation = delta.next_generation;
    let index = GenerationIdentifierIndex::build_for_planner(&store, generation, 257, 1, 9000)?;
    let all = delta
        .upsert_nodes
        .iter()
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    let selected = all.iter().take(256).copied().collect::<BTreeSet<_>>();
    let original = selected.iter().take(32).copied().collect::<BTreeSet<_>>();
    let request = LexicalPlanRequest {
        query: "absent",
        posting_budget: 1,
    };
    let plan = index.lexical_packing_plan(&store, generation, &request, &selected, &original)?;
    assert_eq!(plan.len(), 256);
    assert_eq!(plan.iter().copied().collect::<BTreeSet<_>>(), selected);
    assert_eq!(
        plan,
        index.lexical_packing_plan(&store, generation, &request, &selected, &original)?
    );
    assert!(
        index
            .lexical_packing_plan(&store, generation, &request, &all, &original)
            .is_err()
    );
    let excessive_seeds = selected.iter().take(33).copied().collect::<BTreeSet<_>>();
    assert!(
        index
            .lexical_packing_plan(&store, generation, &request, &selected, &excessive_seeds)
            .is_err()
    );
    Ok(())
}

fn primary(nodes: &[Node], id: &NodeId) -> bool {
    nodes
        .iter()
        .find(|node| node.id == *id)
        .is_some_and(|node| {
            matches!(
                LexicalFamily::of(&node.kind),
                LexicalFamily::Code | LexicalFamily::Documentation
            )
        })
}

fn complete_oracle(
    nodes: &[Node],
    query: &str,
    eligible: Option<&BTreeSet<NodeId>>,
    limit: usize,
) -> Vec<NodeId> {
    let mut queues = Vec::new();
    for family in [
        LexicalFamily::Code,
        LexicalFamily::Documentation,
        LexicalFamily::Reference,
        LexicalFamily::Other,
    ] {
        for channel in [LexicalChannel::Exact, LexicalChannel::Stemmed] {
            let request = LexicalCandidateRequest {
                query,
                channel,
                family: Some(family),
                eligible,
                posting_budget: usize::MAX,
                limit,
            };
            queues.push(
                super::super::lexical::tests::oracle(nodes, &request)
                    .into_iter()
                    .map(|entry| entry.node.id)
                    .collect::<VecDeque<_>>(),
            );
        }
    }
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    while ordered.len() < limit && queues.iter().any(|queue| !queue.is_empty()) {
        for queue in &mut queues {
            while let Some(id) = queue.pop_front() {
                if seen.insert(id) {
                    ordered.push(id);
                    break;
                }
            }
            if ordered.len() == limit {
                break;
            }
        }
    }
    if let Some(ids) = eligible {
        ordered.extend(ids.difference(&seen).copied());
    }
    ordered.sort_by_key(|id| !primary(nodes, id));
    ordered
}

#[test]
fn indexed_seed_and_complete_packing_plans_match_independent_corpus_oracle()
-> Result<(), QueryError> {
    let (store, delta) = super::super::lexical::tests::fixture()?;
    let generation = delta.next_generation;
    let index =
        GenerationIdentifierIndex::build_for_planner(&store, generation, 90, 1000, 100_000)?;
    let selected = delta
        .upsert_nodes
        .iter()
        .take(64)
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    for query in ["alpha beta", "execution execute", "absent", ""] {
        let request = LexicalPlanRequest {
            query,
            posting_budget: 10_000,
        };
        let seeds = index.lexical_seed_plan(&store, generation, &request)?;
        assert_eq!(seeds, complete_oracle(&delta.upsert_nodes, query, None, 32));
        let original = seeds
            .iter()
            .filter(|id| selected.contains(id))
            .copied()
            .collect::<BTreeSet<_>>();
        let complete = complete_oracle(&delta.upsert_nodes, query, Some(&selected), selected.len());
        let mut lexical = complete.iter().filter(|id| original.contains(id));
        let mut discoveries = complete.iter().filter(|id| !original.contains(id));
        let mut expected = Vec::new();
        loop {
            let left = lexical.next();
            let right = discoveries.next();
            if left.is_none() && right.is_none() {
                break;
            }
            expected.extend(left.copied());
            expected.extend(right.copied());
        }
        expected.sort_by_key(|id| !primary(&delta.upsert_nodes, id));
        let actual =
            index.lexical_packing_plan(&store, generation, &request, &selected, &original)?;
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), selected.len());
        assert_eq!(actual.iter().copied().collect::<BTreeSet<_>>(), selected);
    }
    Ok(())
}

#[test]
fn source_span_seeds_prefer_distinct_files_without_expanding_the_seed_budget()
-> Result<(), QueryError> {
    let (mut store, mut delta) = super::super::tests::path_fixture()?;
    let template = delta
        .upsert_nodes
        .first()
        .cloned()
        .ok_or(QueryError::InvalidLimit)?;
    delta.expected_base = Some(delta.next_generation);
    delta.next_generation = syntaxmesh_core::GenerationId::derive(&[b"file-diverse-seeds"]);
    delta.remove_nodes = vec![template.id];
    let mut ids = (0_u64..80)
        .map(|ordinal| NodeId::derive(&[b"file-diverse-seed", &ordinal.to_le_bytes()]))
        .collect::<Vec<_>>();
    ids.sort_unstable();
    let target = ids.pop().ok_or(QueryError::InvalidLimit)?;
    delta.upsert_nodes = ids
        .iter()
        .chain(std::iter::once(&target))
        .map(|id| Node {
            id: *id,
            kind: syntaxmesh_core::NodeKind::Function,
            name: "checkpoint recovery".to_owned(),
            ..template.clone()
        })
        .collect();
    store.apply_delta(delta.clone())?;
    let generation = delta.next_generation;
    let mut index =
        GenerationIdentifierIndex::build_for_planner(&store, generation, 80, 1000, 100_000)?;
    let request = LexicalPlanRequest {
        query: "checkpoint",
        posting_budget: 1000,
    };
    let ordinary = index.lexical_seed_plan(&store, generation, &request)?;
    if ordinary.contains(&target) || ordinary.len() != 32 {
        return Err(QueryError::Context(
            "fixture did not place the target beyond the ordinary seed cap".to_owned(),
        ));
    }
    let crowded_file = FileId::derive(&[b"crowded-source-file"]);
    let target_file = FileId::derive(&[b"target-source-file"]);
    if let Some(planner) = &mut index.planner {
        for id in &ids {
            planner.source_files.insert(*id, crowded_file);
        }
        planner.source_files.insert(target, target_file);
    } else {
        return Err(QueryError::Context(
            "planner metadata is unavailable".to_owned(),
        ));
    }
    index.source_span_augmented = true;
    let diverse = index.lexical_seed_plan(&store, generation, &request)?;
    if diverse.len() != 32 || !diverse.contains(&target) {
        return Err(QueryError::Context(
            "file diversity did not admit the lower-ranked file leader".to_owned(),
        ));
    }
    let limited_request = LexicalPlanRequest {
        query: request.query,
        posting_budget: 1,
    };
    assert!(
        index
            .lexical_seed_plan(&store, generation, &limited_request)
            .is_err()
    );
    Ok(())
}

#[test]
fn source_content_seed_scores_keep_ranked_candidates_beyond_channel_quota() -> Result<(), QueryError>
{
    let mut data = super::super::lexical::PlannerData::default();
    let mut exact = Vec::new();
    let mut target = None;
    for rank in 0_u64..40 {
        let id = NodeId::derive(&[b"source-score-rank", &rank.to_le_bytes()]);
        data.families.insert(id, LexicalFamily::Code);
        data.source_files.insert(
            id,
            FileId::derive(&[b"source-score-file", &rank.to_le_bytes()]),
        );
        exact.push((id, 100_u64.saturating_sub(rank)));
        if rank == 23 {
            target = Some(id);
        }
    }
    let target = target.ok_or(QueryError::InvalidLimit)?;
    let selected = super::source_content_seed_selection(&exact, &[], &data, 32);
    assert_eq!(selected.len(), 32);
    assert!(selected.contains(&target));
    Ok(())
}

#[test]
fn source_content_seed_scores_fuse_channels_and_defer_nonprimary_families() {
    let mut data = super::super::lexical::PlannerData::default();
    let code_stem_overlap = NodeId::derive(&[b"source-score-overlap"]);
    let stronger_code = NodeId::derive(&[b"source-score-code"]);
    let documentation = NodeId::derive(&[b"source-score-documentation"]);
    let reference = NodeId::derive(&[b"source-score-reference"]);
    for (id, family, name) in [
        (
            code_stem_overlap,
            LexicalFamily::Code,
            b"overlap".as_slice(),
        ),
        (stronger_code, LexicalFamily::Code, b"code".as_slice()),
        (
            documentation,
            LexicalFamily::Documentation,
            b"documentation".as_slice(),
        ),
        (reference, LexicalFamily::Reference, b"reference".as_slice()),
    ] {
        data.families.insert(id, family);
        data.source_files.insert(id, FileId::derive(&[name]));
    }
    let exact = [
        (code_stem_overlap, 100),
        (stronger_code, 150),
        (documentation, 140),
    ];
    let stemmed = [(code_stem_overlap, 100), (reference, 400)];
    assert_eq!(
        super::source_content_seed_selection(&exact, &stemmed, &data, 2),
        [stronger_code, documentation]
    );
    assert_eq!(
        super::source_content_seed_selection(&exact, &stemmed, &data, 3),
        [stronger_code, documentation, code_stem_overlap]
    );
    assert_eq!(
        super::source_content_seed_selection(&exact, &stemmed, &data, 4),
        [stronger_code, documentation, code_stem_overlap, reference]
    );
}

#[test]
fn source_content_seed_diversity_treats_missing_provenance_as_singletons() {
    let mut data = super::super::lexical::PlannerData::default();
    let exact = (0_u64..3)
        .map(|rank| {
            let id = NodeId::derive(&[b"source-unlocated", &rank.to_le_bytes()]);
            data.families.insert(id, LexicalFamily::Code);
            (id, 100 - rank)
        })
        .collect::<Vec<_>>();
    let selected = super::source_content_seed_selection(&exact, &[], &data, 2);
    assert_eq!(selected.len(), 2);
    assert_eq!(selected.first().copied(), exact.first().map(|item| item.0));
    assert_eq!(selected.get(1).copied(), exact.get(1).map(|item| item.0));
}

#[test]
fn aggregate_budget_is_shared_and_foreign_originals_are_rejected() -> Result<(), QueryError> {
    let (store, delta) = super::super::tests::path_fixture()?;
    let generation = delta.next_generation;
    let index = GenerationIdentifierIndex::build_for_planner(&store, generation, 1, 2, 105)?;
    let mut request = LexicalPlanRequest {
        query: "load",
        posting_budget: 8,
    };
    assert_eq!(
        index.lexical_seed_plan(&store, generation, &request)?.len(),
        1
    );
    request.posting_budget = 7;
    assert!(
        index
            .lexical_seed_plan(&store, generation, &request)
            .is_err()
    );
    request.posting_budget = 0;
    assert!(
        index
            .lexical_seed_plan(&store, generation, &request)
            .is_err()
    );
    request.posting_budget = 8;
    let known = delta
        .upsert_nodes
        .iter()
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    let foreign = BTreeSet::from([NodeId::derive(&[b"foreign-plan"])]);
    assert!(
        index
            .lexical_packing_plan(&store, generation, &request, &known, &foreign)
            .is_err()
    );
    assert!(
        index
            .lexical_packing_plan(&store, generation, &request, &foreign, &BTreeSet::new())
            .is_err()
    );
    assert!(
        index
            .lexical_packing_plan(
                &store,
                generation,
                &request,
                &BTreeSet::new(),
                &BTreeSet::new()
            )?
            .is_empty()
    );
    Ok(())
}
