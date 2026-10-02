use std::collections::BTreeSet;

use syntaxmesh_api_model::{ContextItem, ContextItemKind, OmittedContextSummary};

use super::{Candidate, Counts};

fn candidate(kind: ContextItemKind, position: usize) -> Candidate {
    Candidate {
        granularity: super::super::SourceGranularity::Local,
        kind,
        relevance: 0,
        distance: 0,
        key: position.to_string(),
        item: ContextItem {
            rank: 0,
            evidence_class: None,
            kind,
            text: "fixture".to_owned(),
            node_ids: Vec::new(),
            edge_ids: Vec::new(),
            source_path: None,
            line_start: None,
            line_end: None,
        },
    }
}

fn scanning_oracle(candidates: &[Candidate], included: &BTreeSet<String>) -> OmittedContextSummary {
    let mut summary = OmittedContextSummary::default();
    for candidate in candidates
        .iter()
        .filter(|candidate| !included.contains(&candidate.key))
    {
        let bucket = match candidate.kind {
            ContextItemKind::SourceEvidence => &mut summary.source_evidence,
            ContextItemKind::Signature => &mut summary.signatures,
            ContextItemKind::GraphPath => &mut summary.graph_paths,
            ContextItemKind::Summary => &mut summary.summaries,
        };
        *bucket = bucket.saturating_add(1);
    }
    summary
}

#[test]
fn incremental_trials_match_scanning_oracle_for_acceptance_and_rejection() {
    let kinds = [
        ContextItemKind::SourceEvidence,
        ContextItemKind::Signature,
        ContextItemKind::GraphPath,
        ContextItemKind::Summary,
    ];
    let candidates = kinds
        .into_iter()
        .cycle()
        .take(257)
        .enumerate()
        .map(|(position, kind)| candidate(kind, position))
        .collect::<Vec<_>>();
    for accept_every in [1, 2, 3, 17, 1000] {
        let mut counts = Counts::from_candidates(&candidates);
        let mut included = BTreeSet::new();
        assert_eq!(counts.summary(), scanning_oracle(&candidates, &included));
        for (position, candidate) in candidates.iter().enumerate() {
            let trial = counts.without(candidate.kind);
            included.insert(candidate.key.clone());
            assert_eq!(trial.summary(), scanning_oracle(&candidates, &included));
            if position.is_multiple_of(accept_every) {
                counts = trial;
            } else {
                included.remove(&candidate.key);
            }
            assert_eq!(counts.summary(), scanning_oracle(&candidates, &included));
        }
    }
}

#[test]
#[cfg(target_pointer_width = "64")]
fn raw_counts_preserve_saturation_until_below_dto_limit() {
    let maximum = u32::MAX as usize;
    let counts = Counts {
        graph_paths: maximum.saturating_add(1),
        ..Counts::default()
    };
    assert_eq!(counts.summary().graph_paths, u32::MAX);
    let trial = counts.without(ContextItemKind::GraphPath);
    assert_eq!(trial.summary().graph_paths, u32::MAX);
    assert_eq!(
        trial
            .without(ContextItemKind::GraphPath)
            .summary()
            .graph_paths,
        u32::MAX.saturating_sub(1)
    );
    assert_eq!(counts.summary().graph_paths, u32::MAX);
}
