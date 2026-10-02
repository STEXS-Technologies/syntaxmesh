use super::*;
use syntaxmesh_core::{
    EvidenceClass, GraphDelta, IndexRunId, Node, NodeId, NodeKind, Provenance, ProvenanceId,
    RepositoryId, WorktreeId,
};
use syntaxmesh_store::InMemoryGraphStore;

#[test]
fn multi_page_counts_remain_pinned_after_label_replacement() -> Result<(), QueryError> {
    let repository = RepositoryId::derive(&[b"stats-repository"]);
    let worktree = WorktreeId::derive(&[b"stats-worktree"]);
    let first = GenerationId::derive(&[b"stats-first"]);
    let second = GenerationId::derive(&[b"stats-second"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"stats-producer"]),
        producer_namespace: "test/stats".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let nodes = (0_u64..1002)
        .map(|index| Node {
            id: NodeId::derive(&[&index.to_le_bytes()]),
            kind: NodeKind::Function,
            name: "executeWithRetry_retry".to_owned(),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        })
        .collect::<Vec<_>>();
    let mut replacement = nodes.first().cloned().ok_or(QueryError::InvalidLimit)?;
    replacement.name = "unrelated".to_owned();
    let mut store = InMemoryGraphStore::new();
    let initial = GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"stats-initial"]),
        expected_base: None,
        next_generation: first,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: nodes,
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    store.apply_delta(initial.clone())?;
    let before = generation_term_statistics(&store, first, "retry execute", 1002)?;
    let ranked_before =
        crate::reference_ranked_candidates(&store, first, "retry execute", 1002, 10)?;
    let identifier_index =
        crate::Query::new(&store, first).build_identifier_index(1002, 3006, 96_208)?;
    if crate::Query::new(&store, first).identifier_candidates(
        &identifier_index,
        "retry execute",
        2004,
        10,
    )? != ranked_before
    {
        return Err(QueryError::Context(
            "query identifier channel differs from reference".to_owned(),
        ));
    }
    let mut other_store = InMemoryGraphStore::new();
    let mut other_delta = initial.clone();
    other_delta.repository = RepositoryId::derive(&[b"different-index-repository"]);
    other_store.apply_delta(other_delta)?;
    if identifier_index
        .candidates(&other_store, first, "absent", 1, 10)
        .is_ok()
    {
        return Err(QueryError::Context(
            "identifier index accepted another repository with the same generation ID".to_owned(),
        ));
    }
    if identifier_index.candidates(&store, first, "retry execute", 2004, 10)? != ranked_before
        || identifier_index
            .candidates(&store, first, "retry execute", 2003, 10)
            .is_ok()
        || crate::GenerationIdentifierIndex::build(&store, first, 1001, 3006, 96_208).is_ok()
        || crate::GenerationIdentifierIndex::build(&store, first, 1002, 3005, 96_208).is_ok()
        || crate::GenerationIdentifierIndex::build(&store, first, 1002, 3006, 96_207).is_ok()
        || !identifier_index
            .candidates(&store, first, "absent", 1, 10)?
            .is_empty()
    {
        return Err(QueryError::Context(
            "identifier postings disagree with the reference or accepted partial work".to_owned(),
        ));
    }
    let mut expected_ids = store
        .historical_snapshot(first)?
        .nodes
        .into_iter()
        .map(|node| node.id)
        .collect::<Vec<_>>();
    expected_ids.sort();
    expected_ids.truncate(10);
    if ranked_before
        .iter()
        .map(|candidate| candidate.node.id)
        .collect::<Vec<_>>()
        != expected_ids
        || ranked_before.iter().any(|candidate| candidate.score == 0)
        || crate::reference_ranked_candidates(&store, first, "retry", 1000, 10).is_ok()
    {
        return Err(QueryError::Serialization(
            "ranked candidates lost deterministic ties or accepted partial statistics".to_owned(),
        ));
    }
    let partial = generation_term_statistics(&store, first, "retry", 1000)?;
    store.apply_delta(GraphDelta {
        run_id: IndexRunId::derive(&[b"stats-replacement"]),
        expected_base: Some(first),
        next_generation: second,
        upsert_provenance: Vec::new(),
        upsert_nodes: vec![replacement],
        ..initial
    })?;
    let retained = generation_term_statistics(&store, first, "retry execute", 1002)?;
    if crate::Query::new(&store, second)
        .identifier_candidates(&identifier_index, "retry execute", 2004, 10)
        .is_ok()
    {
        return Err(QueryError::Context(
            "query accepted a foreign generation index".to_owned(),
        ));
    }
    if identifier_index.candidates(&store, first, "retry execute", 2004, 10)? != ranked_before
        || identifier_index
            .candidates(&store, second, "retry execute", 2004, 10)
            .is_ok()
    {
        return Err(QueryError::Context(
            "identifier index lost generation isolation".to_owned(),
        ));
    }
    if crate::reference_ranked_candidates(&store, first, "retry execute", 1002, 10)?
        != ranked_before
    {
        return Err(QueryError::Serialization(
            "retained candidate ranking changed after publication".to_owned(),
        ));
    }
    let current = generation_term_statistics(&store, second, "retry execute unrelated", 1002)?;
    if !before.complete
        || before.scanned_nodes != 1002
        || before.frequencies.get("retry") != Some(&1002)
        || before.frequencies.get("execute") != Some(&1002)
        || retained != before
        || partial.complete
        || partial.scanned_nodes != 1000
        || partial.frequencies.get("retry") != Some(&1000)
        || !current.complete
        || current.frequencies.get("retry") != Some(&1001)
        || current.frequencies.get("unrelated") != Some(&1)
    {
        return Err(QueryError::Serialization(
            "generation-scoped term counts changed or counted occurrences rather than documents"
                .to_owned(),
        ));
    }
    if generation_term_statistics(&store, first, "a b c d e f g h i j k l m n o p q", 1).is_ok()
        || generation_term_statistics(&store, GenerationId::derive(&[b"unknown"]), "retry", 1)
            .is_ok()
    {
        return Err(QueryError::Serialization(
            "invalid statistics request accepted".to_owned(),
        ));
    }
    Ok(())
}
