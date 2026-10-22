#![allow(clippy::panic_in_result_fn)]

use super::super::{Candidate, ContextTokenCounter};
use super::*;
use syntaxmesh_api_model::{
    CONTEXT_PACK_SCHEMA_VERSION, ContextItem, ContextItemKind, ContextPack, OmittedContextSummary,
};
use syntaxmesh_core::{GenerationId, NodeId, RepositoryId, WorktreeId};

struct ByteCounter;

impl ContextTokenCounter for ByteCounter {
    fn tokenizer_id(&self) -> &str {
        "test-byte-counter-v1"
    }

    fn count_tokens(&self, serialized_pack: &str) -> Result<u64, String> {
        u64::try_from(serialized_pack.len()).map_err(|error| error.to_string())
    }
}

fn pack(token_budget: u64) -> ContextPack {
    ContextPack {
        schema_version: CONTEXT_PACK_SCHEMA_VERSION,
        query: "expected version reducer".to_owned(),
        repository: RepositoryId::derive(&[b"excerpt-pack-repository"]),
        worktree: WorktreeId::derive(&[b"excerpt-pack-worktree"]),
        generation: GenerationId::derive(&[b"excerpt-pack-generation"]),
        tokenizer: "test-byte-counter-v1".to_owned(),
        token_budget,
        token_count: 0,
        items: Vec::new(),
        warnings: Vec::new(),
        omitted: OmittedContextSummary::default(),
    }
}

fn source_candidate(source: &str) -> Candidate {
    let id = NodeId::derive(&[b"excerpt-pack-node", source.as_bytes()]);
    let line_end = u32::try_from(source.lines().count()).unwrap_or(u32::MAX);
    Candidate {
        kind: ContextItemKind::SourceEvidence,
        granularity: super::super::ranking::SourceGranularity::Local,
        relevance: 1,
        distance: 0,
        key: id.0.to_hex(),
        item: ContextItem {
            rank: 0,
            evidence_class: None,
            kind: ContextItemKind::SourceEvidence,
            text: format!("src/reducer.rs:1-{line_end}\n{source}"),
            node_ids: vec![id],
            edge_ids: Vec::new(),
            source_path: Some("src/reducer.rs".to_owned()),
            line_start: Some(1),
            line_end: Some(line_end),
        },
    }
}

#[test]
fn candidate_admits_excerpt_only_after_complete_source_fails_and_counts_once()
-> Result<(), crate::QueryError> {
    let unrelated = "unrelated evidence text ".repeat(6);
    let mut source_lines = vec![unrelated; 20];
    source_lines.push("check_expected_version_and_apply_state_reducer".to_owned());
    let candidate = source_candidate(&source_lines.join("\n"));
    let mut pack = pack(1300);
    let mut scan_bytes = 8 * 1024 * 1024;
    let mut excerpt_bytes = 256 * 1024;
    let omitted = OmittedContextSummary {
        source_evidence: 0,
        ..OmittedContextSummary::default()
    };
    assert!(try_candidate(
        &mut pack,
        &candidate,
        omitted,
        &ByteCounter,
        &mut scan_bytes,
        &mut excerpt_bytes,
    )?);
    assert_eq!(pack.items.len(), 1);
    let Some(item) = pack.items.first() else {
        return Err(crate::QueryError::Context(
            "excerpt was not packed".to_owned(),
        ));
    };
    if item.text.contains("complete evidence")
        || !item.text.contains("[…]\n")
        || !item
            .text
            .contains("check_expected_version_and_apply_state_reducer")
        || item.node_ids != candidate.item.node_ids
        || item.line_start != Some(19)
        || item.line_end != Some(21)
        || pack.token_count > pack.token_budget
    {
        return Err(crate::QueryError::Context(format!(
            "bad excerpt text={:?} lines={:?}-{:?} token_count={} budget={}",
            item.text, item.line_start, item.line_end, pack.token_count, pack.token_budget
        )));
    }
    Ok(())
}
#[test]
fn candidate_keeps_complete_source_when_it_fits() -> Result<(), crate::QueryError> {
    let candidate = source_candidate("complete expected version reducer");
    let mut pack = pack(2048);
    let mut scan_bytes = 8 * 1024 * 1024;
    let mut excerpt_bytes = 256 * 1024;
    assert!(try_candidate(
        &mut pack,
        &candidate,
        OmittedContextSummary::default(),
        &ByteCounter,
        &mut scan_bytes,
        &mut excerpt_bytes,
    )?);
    if pack.items.first().map(|item| item.text.as_str())
        != Some("src/reducer.rs:1-1\ncomplete expected version reducer")
    {
        return Err(crate::QueryError::Context(
            "fitting source was replaced by an excerpt".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn candidate_retries_complete_source_after_compacting_lower_priority_evidence()
-> Result<(), crate::QueryError> {
    let unrelated = "let unrelated_value = 1234567890; ".repeat(4);
    let mut earlier_lines = vec![unrelated; 24];
    earlier_lines.push("expected version reducer;".to_owned());
    let earlier = source_candidate(&earlier_lines.join("\n"));
    let later_line = format!("let other_unrelated_value = {}", "x".repeat(64));
    let later = source_candidate(
        &std::iter::repeat_n(later_line.as_str(), 6)
            .chain(std::iter::once("expected version reducer;"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if later.item.text.len() <= 512 {
        return Err(crate::QueryError::Context(
            "complete-source retry fixture must exercise the long-span path".to_owned(),
        ));
    }
    let mut context = pack(16 * 1024);
    assert!(try_item(
        &mut context,
        earlier.item,
        OmittedContextSummary::default(),
        &ByteCounter,
    )?);
    context.token_budget = 2300;
    assert!(!try_item(
        &mut context.clone(),
        later.item.clone(),
        OmittedContextSummary::default(),
        &ByteCounter,
    )?);
    let mut scan_bytes = 8 * 1024 * 1024;
    let mut excerpt_bytes = 256 * 1024;
    if !try_candidate(
        &mut context,
        &later,
        OmittedContextSummary::default(),
        &ByteCounter,
        &mut scan_bytes,
        &mut excerpt_bytes,
    )? {
        return Err(crate::QueryError::Context(
            "candidate could not be admitted after source compaction".to_owned(),
        ));
    }
    if context.items.len() != 2
        || context
            .items
            .first()
            .is_none_or(|item| !item.text.contains("[…]"))
        || context
            .items
            .get(1)
            .is_none_or(|item| item.node_ids != later.item.node_ids || item.text != later.item.text)
        || context.token_count > context.token_budget
    {
        return Err(crate::QueryError::Context(format!(
            "source compaction changed identity, order, or budget: first={:?}, second={:?}, tokens={}/{}",
            context.items.first().map(|item| &item.text),
            context.items.get(1).map(|item| &item.text),
            context.token_count,
            context.token_budget
        )));
    }
    Ok(())
}

#[test]
fn short_complete_candidate_is_retried_after_compacting_existing_source()
-> Result<(), crate::QueryError> {
    let mut earlier_lines = vec!["unrelated source evidence with repeated words".to_owned(); 24];
    earlier_lines.push("expected version reducer;".to_owned());
    let earlier = source_candidate(&earlier_lines.join("\n"));
    let later = source_candidate("expected_version reducer");
    let mut context = pack(16 * 1024);
    assert!(try_item(
        &mut context,
        earlier.item,
        OmittedContextSummary::default(),
        &ByteCounter,
    )?);

    let mut complete = context.clone();
    complete.token_budget = u64::MAX;
    assert!(try_item(
        &mut complete,
        later.item.clone(),
        OmittedContextSummary::default(),
        &ByteCounter,
    )?);
    let mut token_budget = complete.token_count;
    loop {
        token_budget = token_budget.saturating_sub(1);
        let mut trial = context.clone();
        trial.token_budget = token_budget;
        if !try_item(
            &mut trial,
            later.item.clone(),
            OmittedContextSummary::default(),
            &ByteCounter,
        )? {
            break;
        }
    }
    context.token_budget = token_budget;
    let mut scan_bytes = 8 * 1024 * 1024;
    let mut excerpt_bytes = 256 * 1024;

    if !try_candidate(
        &mut context,
        &later,
        OmittedContextSummary::default(),
        &ByteCounter,
        &mut scan_bytes,
        &mut excerpt_bytes,
    )? {
        return Err(crate::QueryError::Context(
            "short complete candidate was not retried after compaction".to_owned(),
        ));
    }
    let Some(compacted) = context.items.first() else {
        return Err(crate::QueryError::Context(
            "compacted source evidence is missing".to_owned(),
        ));
    };
    let Some(admitted) = context.items.get(1) else {
        return Err(crate::QueryError::Context(
            "complete short candidate is missing".to_owned(),
        ));
    };
    if !compacted.text.contains("[…]")
        || admitted.text != later.item.text
        || admitted.node_ids != later.item.node_ids
        || context.token_count > context.token_budget
    {
        return Err(crate::QueryError::Context(format!(
            "bad compacted/admitted items: compacted={:?}, admitted={:?}, tokens={}/{}",
            compacted.text, admitted.text, context.token_count, context.token_budget
        )));
    }
    Ok(())
}
