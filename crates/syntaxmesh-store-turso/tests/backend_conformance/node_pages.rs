use super::*;
use syntaxmesh_core::StableId;
use syntaxmesh_store::MAX_HISTORICAL_NODE_PAGE_SIZE;

fn verify_pages<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
) -> Result<(), StoreError> {
    super::file_pages::verify(store, generation)?;
    let mut expected = store.historical_snapshot(generation)?.nodes;
    expected.sort_by_key(|node| node.id);
    if store
        .visit_historical_nodes(
            GenerationId::derive(&[b"unknown-scan-generation"]),
            1,
            &mut |_node| Ok(()),
        )
        .is_ok()
    {
        return Err(StoreError::Integrity(
            "unknown historical scan generation accepted".to_owned(),
        ));
    }
    let mut visited = Vec::new();
    let count = store.visit_historical_nodes(generation, expected.len().max(1), &mut |node| {
        visited.push(node);
        Ok(())
    })?;
    if visited != expected || count != expected.len() {
        return Err(StoreError::Integrity(
            "historical visitor differs from snapshot".to_owned(),
        ));
    }
    if !matches!(
        store.visit_historical_nodes(generation, 0, &mut |_node| Ok(())),
        Err(StoreError::InvalidPageLimit)
    ) {
        return Err(StoreError::Integrity(
            "zero historical scan budget accepted".to_owned(),
        ));
    }
    if expected.len() > 1
        && !matches!(
            store.visit_historical_nodes(
                generation,
                expected.len().saturating_sub(1),
                &mut |_node| Ok(())
            ),
            Err(StoreError::InvalidPageLimit)
        )
    {
        return Err(StoreError::Integrity(
            "exhausted historical scan budget accepted".to_owned(),
        ));
    }
    let mut callbacks = 0_usize;
    let stopped = store.visit_historical_nodes(generation, expected.len().max(1), &mut |_node| {
        callbacks = callbacks.saturating_add(1);
        Err(StoreError::Integrity("visitor stopped".to_owned()))
    });
    if !expected.is_empty()
        && (callbacks != 1
            || !matches!(stopped,
        Err(StoreError::Integrity(ref message)) if message == "visitor stopped"))
    {
        return Err(StoreError::Integrity(
            "historical visitor did not stop immediately".to_owned(),
        ));
    }
    let mut batch_ids = expected
        .iter()
        .take(254)
        .map(|node| node.id)
        .collect::<Vec<_>>();
    batch_ids.reverse();
    if let Some(id) = batch_ids.first().copied() {
        batch_ids.push(id);
    }
    batch_ids.push(NodeId::derive(&[b"missing-batch-node"]));
    if store.historical_nodes_by_ids(generation, &batch_ids)?
        != expected.iter().take(254).cloned().collect::<Vec<_>>()
        || !store.historical_nodes_by_ids(generation, &[])?.is_empty()
        || store
            .historical_nodes_by_ids(generation, &vec![NodeId::derive(&[b"too-many"]); 257])
            .is_ok()
        || store
            .historical_nodes_by_ids(GenerationId::derive(&[b"unknown-batch"]), &[])
            .is_ok()
    {
        return Err(StoreError::Integrity(
            "historical node batch differs from pinned versions or accepts invalid limits"
                .to_owned(),
        ));
    }
    verify_term_statistics(store, generation, &expected)?;
    let mut after = None;
    let mut collected = Vec::new();
    let mut page_number = 0_usize;
    loop {
        let limit = [1, 3, 2]
            .get(page_number % 3)
            .copied()
            .ok_or(StoreError::InvalidPageLimit)?;
        let page = store.historical_nodes_page(generation, after, limit)?;
        let eligible = expected
            .iter()
            .filter(|node| after.is_none_or(|id| node.id > id))
            .cloned()
            .collect::<Vec<_>>();
        if page.items != eligible.iter().take(limit).cloned().collect::<Vec<_>>()
            || page.has_more != (eligible.len() > limit)
        {
            return Err(StoreError::Integrity(
                "historical node page differs from snapshot".to_owned(),
            ));
        }
        after = page.items.last().map(|node| node.id);
        collected.extend(page.items);
        if !page.has_more {
            break;
        }
        page_number = page_number.saturating_add(1);
        if page_number > expected.len() {
            return Err(StoreError::InvalidPageLimit);
        }
    }
    if collected != expected {
        return Err(StoreError::Integrity("node pages lost versions".to_owned()));
    }
    for limit in [0, MAX_HISTORICAL_NODE_PAGE_SIZE.saturating_add(1)] {
        if !matches!(
            store.historical_nodes_page(generation, None, limit),
            Err(StoreError::InvalidPageLimit)
        ) {
            return Err(StoreError::Integrity(
                "invalid node page limit accepted".to_owned(),
            ));
        }
    }
    for seek in [
        NodeId(StableId([0; 32])),
        NodeId(StableId([0x80; 32])),
        NodeId(StableId([u8::MAX; 32])),
    ] {
        let page =
            store.historical_nodes_page(generation, Some(seek), MAX_HISTORICAL_NODE_PAGE_SIZE)?;
        let eligible = expected
            .iter()
            .filter(|node| node.id > seek)
            .cloned()
            .collect::<Vec<_>>();
        if page.items != eligible || page.has_more {
            return Err(StoreError::Integrity("sparse node seek differs".to_owned()));
        }
    }
    if store
        .historical_nodes_page(GenerationId::derive(&[b"unknown-node-page"]), None, 1)
        .is_ok()
    {
        return Err(StoreError::Integrity(
            "unknown node-page generation accepted".to_owned(),
        ));
    }
    Ok(())
}

fn verify_term_statistics<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
    expected: &[Node],
) -> Result<(), StoreError> {
    let query = "node page updated caller callee";
    let terms = syntaxmesh_query::identifier_terms(query);
    let postings = expected
        .iter()
        .map(|node| syntaxmesh_query::identifier_terms(&node.name).len())
        .sum::<usize>()
        .max(1);
    let identifier_index = syntaxmesh_query::GenerationIdentifierIndex::build(
        store,
        generation,
        expected.len().max(1),
        postings,
        expected
            .iter()
            .map(|node| node.name.len())
            .sum::<usize>()
            .saturating_add(postings.saturating_mul(32))
            .max(1),
    )
    .map_err(|error| StoreError::Backend(error.to_string()))?;
    let indexed_candidates = identifier_index
        .candidates(store, generation, query, postings, 10)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let reference_candidates = syntaxmesh_query::reference_ranked_candidates(
        store,
        generation,
        query,
        expected.len().max(1),
        10,
    )
    .map_err(|error| StoreError::Backend(error.to_string()))?;
    if indexed_candidates != reference_candidates {
        return Err(StoreError::Integrity(
            "identifier postings differ from retained reference ranking".to_owned(),
        ));
    }
    let full = syntaxmesh_query::generation_term_statistics(
        store,
        generation,
        query,
        expected.len().max(1),
    )
    .map_err(|error| StoreError::Backend(error.to_string()))?;
    let frequencies = terms
        .iter()
        .map(|term| {
            let count = expected
                .iter()
                .filter(|node| syntaxmesh_query::identifier_terms(&node.name).contains(term))
                .count();
            (term.clone(), count)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    if !full.complete
        || full.generation != generation
        || full.scanned_nodes != expected.len()
        || full.frequencies != frequencies
    {
        return Err(StoreError::Integrity(
            "term statistics differ from retained snapshot".to_owned(),
        ));
    }
    let budget = 2;
    let partial = syntaxmesh_query::generation_term_statistics(store, generation, query, budget)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let prefix = terms
        .iter()
        .map(|term| {
            let count = expected
                .iter()
                .take(budget)
                .filter(|node| syntaxmesh_query::identifier_terms(&node.name).contains(term))
                .count();
            (term.clone(), count)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    if partial.complete != (expected.len() <= budget)
        || partial.scanned_nodes != expected.len().min(budget)
        || partial.frequencies != prefix
    {
        return Err(StoreError::Integrity(
            "partial term statistics lost budget semantics".to_owned(),
        ));
    }
    Ok(())
}

fn publish_and_verify<S: GraphStore + ?Sized>(
    store: &mut S,
) -> Result<[GenerationId; 3], StoreError> {
    let mut first = first_delta();
    let old = first.next_generation;
    let file_id = FileId::derive(&[b"conformance-file"]);
    let provenance = ProvenanceId::derive(&[b"conformance-source"]);
    for index in 0..64 {
        let path = format!("file-page-{index}.rs");
        first.changed_files.push(FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path,
            content_hash: [1; 32],
            size_bytes: 1,
        });
        first
            .upsert_nodes
            .push(node(&format!("node-page-{index}"), file_id, provenance));
    }
    store.apply_delta(first)?;
    verify_pages(store, old)?;
    let mut replacement = replacement_delta(old);
    let updated_file = FileVersion {
        file_id: FileId::derive(&[b"file-page-0.rs"]),
        normalized_path: "file-page-0.rs".to_owned(),
        content_hash: [2; 32],
        size_bytes: 3,
    };
    replacement.changed_files.push(updated_file);
    replacement
        .removed_files
        .push(FileId::derive(&[b"file-page-1.rs"]));
    let mut updated = node("node-page-0", file_id, provenance);
    updated.name = "node-page-updated".to_owned();
    replacement.upsert_nodes.push(updated);
    let current = store.apply_delta(replacement)?.generation;
    verify_pages(store, old)?;
    verify_pages(store, current)?;
    let mut deletion = deletion_delta(Some(current));
    deletion.removed_files = store
        .historical_snapshot(current)?
        .files
        .into_iter()
        .map(|file| file.file_id)
        .collect();
    deletion.remove_nodes = store
        .historical_snapshot(current)?
        .nodes
        .into_iter()
        .map(|node| node.id)
        .collect();
    let empty = store.apply_delta(deletion)?.generation;
    verify_pages(store, old)?;
    verify_pages(store, current)?;
    verify_pages(store, empty)?;
    Ok([old, current, empty])
}

#[test]
fn historical_node_pages_seek_and_retain_versions_across_backends_and_restart()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let mut memory = InMemoryGraphStore::new();
    let expected = publish_and_verify(&mut memory)?;
    let file_path = fixture.path().join("graph.snapshot");
    let mut file = FileGraphStore::open(&file_path)?;
    if publish_and_verify(&mut file)? != expected {
        return Err("File generations differ".into());
    }
    drop(file);
    let reopened_file = FileGraphStore::open(&file_path)?;
    for generation in expected {
        verify_pages(&reopened_file, generation)?;
    }
    drop(reopened_file);
    let file_copy = fixture.path().join("copied.snapshot");
    std::fs::copy(&file_path, &file_copy)?;
    let copied_file = FileGraphStore::open(&file_copy)?;
    for generation in expected {
        verify_pages(&copied_file, generation)?;
    }
    let sqlite_path = fixture.path().join("graph.sqlite");
    let mut sqlite = open_migrated(&sqlite_path)?;
    if publish_and_verify(&mut sqlite)? != expected {
        return Err("SQLite generations differ".into());
    }
    drop(sqlite);
    let reopened_sqlite = SqliteGraphStore::open(&sqlite_path)?;
    for generation in expected {
        verify_pages(&reopened_sqlite, generation)?;
    }
    drop(reopened_sqlite);
    let sqlite_copy = fixture.path().join("copied.sqlite");
    super::copy_cold_database_set(&sqlite_path, &sqlite_copy)?;
    let copied_sqlite = SqliteGraphStore::open(&sqlite_copy)?;
    for generation in expected {
        verify_pages(&copied_sqlite, generation)?;
    }
    let turso_path = fixture.path().join("graph.turso");
    let mut turso = open_turso(&turso_path)?;
    if publish_and_verify(&mut turso)? != expected {
        return Err("Turso generations differ".into());
    }
    drop(turso);
    let reopened_turso = TursoGraphStore::open(&turso_path)?;
    for generation in expected {
        verify_pages(&reopened_turso, generation)?;
    }
    drop(reopened_turso);
    let turso_copy = fixture.path().join("copied.turso");
    super::copy_cold_database_set(&turso_path, &turso_copy)?;
    let copied_turso = TursoGraphStore::open(&turso_copy)?;
    for generation in expected {
        verify_pages(&copied_turso, generation)?;
    }
    Ok(())
}
