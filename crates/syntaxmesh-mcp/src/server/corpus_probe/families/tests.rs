use super::{classify, refill};
use syntaxmesh_core::{NodeId, NodeKind, RelationKind};

#[test]
fn discovery_balancing_retains_membership_and_partition_order() {
    let ids = (0_u64..8)
        .map(|ordinal| NodeId::derive(&[&ordinal.to_le_bytes()]))
        .collect::<Vec<_>>();
    for mask in 0_u16..256 {
        let original = ids
            .iter()
            .enumerate()
            .filter(|(position, _)| mask & (1 << position) != 0)
            .map(|(_, id)| *id)
            .collect::<std::collections::BTreeSet<_>>();
        let actual = super::balance_discovery(&ids, &original);
        assert_eq!(actual.len(), ids.len());
        assert_eq!(
            actual
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            ids.iter().copied().collect()
        );
        for lexical in [true, false] {
            assert_eq!(
                actual
                    .iter()
                    .filter(|id| original.contains(id) == lexical)
                    .collect::<Vec<_>>(),
                ids.iter()
                    .filter(|id| original.contains(id) == lexical)
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(actual, super::balance_discovery(&ids, &original));
    }
    let original = ids.iter().take(4).copied().collect();
    let expected = ids
        .iter()
        .take(4)
        .zip(ids.iter().skip(4))
        .flat_map(|(left, right)| [*left, *right])
        .collect::<Vec<_>>();
    assert_eq!(super::balance_discovery(&ids, &original), expected);
    assert!(super::balance_discovery(&[], &original).is_empty());
}

#[test]
fn primary_priority_retains_membership_and_both_relative_orders() {
    let ids = (0_u64..8)
        .map(|ordinal| NodeId::derive(&[&ordinal.to_le_bytes()]))
        .collect::<Vec<_>>();
    for mask in 0_u16..256 {
        let primary = ids
            .iter()
            .enumerate()
            .filter(|(position, _)| mask & (1 << position) != 0)
            .map(|(_, id)| *id)
            .collect::<std::collections::BTreeSet<_>>();
        let expected = ids
            .iter()
            .filter(|id| primary.contains(id))
            .chain(ids.iter().filter(|id| !primary.contains(id)))
            .copied()
            .collect::<Vec<_>>();
        let mut actual = ids.clone();
        super::primary_first(&mut actual, &primary);
        assert_eq!(actual, expected);
        super::primary_first(&mut actual, &primary);
        assert_eq!(actual, expected);
    }
}

#[test]
fn family_local_rarity_is_not_global_rarity_after_filtering() {
    let node = |ordinal: u64, name: &str, kind| syntaxmesh_core::Node {
        id: NodeId::derive(&[&ordinal.to_le_bytes()]),
        kind,
        name: name.to_owned(),
        owner_file: None,
        source: None,
        provenance: syntaxmesh_core::ProvenanceId::derive(&[b"family-statistics"]),
        extension_payload: None,
    };
    let mut nodes = (0..8)
        .map(|ordinal| node(ordinal, "alpha", NodeKind::Function))
        .collect::<Vec<_>>();
    let beta = node(8, "beta", NodeKind::Function);
    let beta_id = beta.id;
    nodes.push(beta);
    nodes.extend((9..89).map(|ordinal| node(ordinal, "beta", NodeKind::DocumentChunk)));
    let code = nodes
        .iter()
        .filter(|entry| classify(&entry.kind) == "code")
        .collect::<Vec<_>>();
    let global =
        super::super::token_scores(&nodes, "alpha beta", syntaxmesh_query::identifier_terms);
    let local =
        super::super::token_scores_refs(&code, "alpha beta", syntaxmesh_query::identifier_terms);
    assert!(
        global
            .first()
            .is_some_and(|(_, entry)| entry.name == "alpha")
    );
    assert!(local.first().is_some_and(|(_, entry)| entry.id == beta_id));
    assert!(
        local
            .iter()
            .all(|(_, entry)| entry.kind == NodeKind::Function)
    );
    assert!(
        super::super::token_scores_refs(&[], "alpha beta", syntaxmesh_query::identifier_terms)
            .is_empty()
    );
    let report = super::super::stemmed_rank(&nodes, "alpha beta", beta_id);
    let family = report
        .get("families")
        .and_then(|families| families.get("code"));
    assert_eq!(
        family
            .and_then(|value| value.get("population"))
            .and_then(serde_json::Value::as_u64),
        Some(9)
    );
    assert_eq!(
        family
            .and_then(|value| value.get("exact_target_rank"))
            .and_then(serde_json::Value::as_u64),
        Some(9)
    );
    assert_eq!(
        family
            .and_then(|value| value.get("family_local_statistics"))
            .and_then(|value| value.get("exact_target_rank"))
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[test]
fn families_distinguish_docs_symbols_and_occurrences() {
    for kind in [
        NodeKind::Script,
        NodeKind::Module,
        NodeKind::File,
        NodeKind::Function,
        NodeKind::Struct,
        NodeKind::Enum,
        NodeKind::Trait,
        NodeKind::Test,
        NodeKind::Class,
    ] {
        assert_eq!(classify(&kind), "code");
    }
    for kind in [
        NodeKind::Document,
        NodeKind::Section,
        NodeKind::DocumentChunk,
    ] {
        assert_eq!(classify(&kind), "documentation");
    }
    for kind in [
        NodeKind::Reference {
            relation: RelationKind::Calls,
        },
        NodeKind::UnresolvedReference {
            relation: RelationKind::Calls,
        },
        NodeKind::AmbiguousReference {
            relation: RelationKind::Calls,
        },
    ] {
        assert_eq!(classify(&kind), "reference");
    }
    assert_eq!(
        classify(&NodeKind::External {
            namespace: "syntaxmesh.source".to_owned(),
            kind: "anchor".to_owned()
        }),
        "other"
    );
}

#[test]
fn refill_exhaustively_preserves_prefixes_membership_and_capacity() {
    // Adapt Shardline's lifecycle sequence-invariant testing approach without
    // adding a property-test dependency: enumerate this small alphabet fully.
    let id = |number: u8| NodeId::derive(&[&[number]]);
    let mut sequences = vec![Vec::new()];
    for length in 1_u32..=3 {
        for encoded in 0..3_u32.pow(length) {
            let mut digits = encoded;
            let mut sequence = Vec::new();
            for _ in 0..length {
                sequence.push(match digits % 3 {
                    0 => id(0),
                    1 => id(1),
                    _ => id(2),
                });
                digits /= 3;
            }
            sequences.push(sequence);
        }
    }
    for left in &sequences {
        for right in &sequences {
            let channels = vec![left.clone(), Vec::new(), right.clone()];
            let available = channels
                .iter()
                .flatten()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            let complete = refill(&channels, 32);
            assert_eq!(
                complete
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>(),
                available
            );
            assert_eq!(complete.len(), available.len());
            for limit in 0..=6 {
                let selected = refill(&channels, limit);
                assert_eq!(selected.len(), limit.min(available.len()));
                assert_eq!(Some(selected.as_slice()), complete.get(..selected.len()));
            }
            // A singleton family must retain first occurrence relevance order,
            // even when IDs sort differently and occurrences repeat.
            let singleton = refill(std::slice::from_ref(left), 32);
            let mut seen = std::collections::BTreeSet::new();
            let expected = left
                .iter()
                .copied()
                .filter(|value| seen.insert(*value))
                .collect::<Vec<_>>();
            assert_eq!(singleton, expected);
        }
    }
}

#[test]
fn refill_preserves_rank_order_and_uses_vacant_duplicate_slots() {
    let id = |number: u8| NodeId::derive(&[&[number]]);
    let channels = vec![
        vec![id(3), id(1), id(2)],
        vec![id(3), id(4), id(4), id(5)],
        vec![],
    ];
    assert_eq!(
        refill(&channels, 5),
        vec![id(3), id(4), id(1), id(5), id(2)]
    );
    assert_eq!(refill(&channels, 3), vec![id(3), id(4), id(1)]);
    assert!(refill(&channels, 0).is_empty());
    assert!(refill(&[], 8).is_empty());
    assert_eq!(refill(&channels, 8).len(), 5);
    let many = vec![(0..100).map(id).collect::<Vec<_>>()];
    assert_eq!(refill(&many, usize::MAX).len(), 32);
    assert_eq!(refill(&channels, 5), refill(&channels, 5));
}
