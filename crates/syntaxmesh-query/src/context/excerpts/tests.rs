#![allow(clippy::panic)]

use syntaxmesh_api_model::{ContextItem, ContextItemKind};
use syntaxmesh_core::{EdgeId, EvidenceClass, NodeId};

use super::super::{Candidate, SourceGranularity};

fn candidate(id: NodeId, relevance: u32) -> Candidate {
    Candidate {
        kind: ContextItemKind::SourceEvidence,
        granularity: SourceGranularity::Local,
        relevance,
        distance: 0,
        key: id.0.to_hex(),
        item: ContextItem {
            rank: 0,
            evidence_class: None,
            kind: ContextItemKind::SourceEvidence,
            text: "file.rs:1-1\nfn helper() {}".to_owned(),
            node_ids: vec![id],
            edge_ids: Vec::new(),
            source_path: Some("file.rs".to_owned()),
            line_start: Some(1),
            line_end: Some(1),
        },
    }
}

#[test]
fn exact_excerpt_retains_all_identities_and_first_priority() {
    let first = NodeId::derive(&[b"first"]);
    let second = NodeId::derive(&[b"second"]);
    let mut candidates = vec![
        candidate(first, 10),
        candidate(second, 1),
        candidate(first, 0),
    ];
    super::coalesce(&mut candidates);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates.first().map(|entry| entry.relevance), Some(10));
    assert_eq!(
        candidates.first().map(|entry| &entry.key),
        Some(&first.0.to_hex())
    );
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(
        candidates.first().map(|entry| &entry.item.node_ids),
        Some(&expected)
    );
}

#[test]
fn distinct_locations_text_and_non_source_items_remain_separate() {
    let base = candidate(NodeId::derive(&[b"base"]), 10);
    let mut changed_path = base.clone();
    changed_path.item.source_path = Some("other.rs".to_owned());
    let mut changed_line = base.clone();
    changed_line.item.line_end = Some(2);
    let mut changed_text = base.clone();
    changed_text.item.text.push(' ');
    let mut changed_class = base.clone();
    changed_class.item.evidence_class = Some(EvidenceClass::SemanticInference);
    let mut changed_edges = base.clone();
    changed_edges.item.edge_ids = vec![EdgeId::derive(&[b"different-edge"])];
    let mut signature = base.clone();
    signature.kind = ContextItemKind::Signature;
    signature.item.kind = ContextItemKind::Signature;
    let mut candidates = vec![
        base,
        changed_path,
        changed_line,
        changed_text,
        changed_class,
        changed_edges,
        signature,
    ];
    super::coalesce(&mut candidates);
    assert_eq!(candidates.len(), 7);
}

#[test]
fn query_windows_are_bounded_term_relevant_and_source_located() {
    let setup_line = "let unrelated_setup_value = 1234567890; ".repeat(4);
    let mut lines = vec!["fn execute() {".to_owned()];
    lines.extend((0..5).map(|_| setup_line.clone()));
    lines.push("check_expected_version();".to_owned());
    lines.extend((0..5).map(|_| setup_line.clone()));
    lines.push("profile.apply_state_reducer();".to_owned());
    lines.extend((0..5).map(|_| setup_line.clone()));
    lines.push("}".to_owned());
    let source = lines.join("\n");
    let mut remaining = 256 * 1024;
    let excerpts = super::query_windows(
        &source,
        40,
        "expected_version StateProfile reducer",
        &mut remaining,
    );
    assert_eq!(excerpts.len(), 4);
    let Some((largest, largest_start, largest_end)) = excerpts.first() else {
        panic!("expected largest relevant excerpt");
    };
    assert!(largest.contains("[…]"));
    assert!(largest.contains("check_expected_version"));
    assert!(largest.contains("profile.apply_state_reducer"));
    assert_eq!((*largest_start, *largest_end), (41, 52));
    let Some((smallest, smallest_start, smallest_end)) = excerpts.last() else {
        panic!("expected one-line relevant excerpt");
    };
    assert!(smallest.contains("state_reducer"));
    assert!(!smallest.contains("unrelated_setup_value"));
    assert_eq!((*smallest_start, *smallest_end), (52, 52));
    assert!(super::query_windows("one line only", 1, "one", &mut remaining).is_empty());
    assert!(super::query_windows(&source, 1, "unmatched query terms", &mut remaining).is_empty());
}
