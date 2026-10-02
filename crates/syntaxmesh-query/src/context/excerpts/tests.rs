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
