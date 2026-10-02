use super::*;

#[test]
fn query_pages_reuse_store_pinning_and_reject_invalid_limits() -> Result<(), QueryError> {
    let mut store = InMemoryGraphStore::new();
    let first = GenerationId::derive(&[b"query-history-page-first"]);
    let second = GenerationId::derive(&[b"query-history-page-second"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"query-history-page-provenance"]),
        producer_namespace: "test.history-pages".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"query-history-page-node"]),
        kind: NodeKind::Function,
        name: "retained".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let delta = GraphDelta {
        repository: RepositoryId::derive(&[b"query-history-page-repository"]),
        worktree: WorktreeId::derive(&[b"query-history-page-worktree"]),
        run_id: IndexRunId::derive(&[b"query-history-page-run-first"]),
        expected_base: None,
        next_generation: first,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![node.clone()],
        remove_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_edges: Vec::new(),
    };
    store.apply_delta(delta.clone())?;
    let original = store.fact_history_page(first, None, 1000)?;
    let mut next = delta;
    next.expected_base = Some(first);
    next.next_generation = second;
    next.run_id = IndexRunId::derive(&[b"query-history-page-run-second"]);
    next.upsert_provenance.clear();
    next.upsert_nodes.clear();
    next.remove_nodes.push(node.id);
    store.apply_delta(next)?;
    let query = Query::new(&store, first);
    if query.fact_history_page(None, 1000)? != original {
        return Err(QueryError::Context(
            "publication changed pinned history page".to_owned(),
        ));
    }
    let mut after = None;
    let mut items = Vec::new();
    loop {
        let page = query.fact_history_page(after, 1)?;
        let expected = store.fact_history_page(first, after, 1)?;
        if page != expected || page.items.len() > 1 {
            return Err(QueryError::Context(
                "query changed store history page".to_owned(),
            ));
        }
        items.extend(page.items);
        after = page.next_cursor;
        if after.is_none() {
            break;
        }
        if items.len() > original.items.len() {
            return Err(QueryError::Context(
                "history cursor did not progress".to_owned(),
            ));
        }
    }
    if items != original.items {
        return Err(QueryError::Context("history pages lost items".to_owned()));
    }
    for limit in [0, 1001, usize::MAX] {
        if !matches!(
            query.fact_history_page(None, limit),
            Err(QueryError::InvalidLimit)
        ) {
            return Err(QueryError::Context(
                "invalid history page limit accepted".to_owned(),
            ));
        }
    }
    let cursor = store
        .fact_history_page(second, None, 1)?
        .next_cursor
        .ok_or(QueryError::InvalidLimit)?;
    if query.fact_history_page(Some(cursor), 1).is_ok()
        || Query::new(&store, GenerationId::derive(&[b"missing-history-page"]))
            .fact_history_page(None, 1)
            .is_ok()
    {
        return Err(QueryError::Context(
            "foreign or missing history generation accepted".to_owned(),
        ));
    }
    Ok(())
}
