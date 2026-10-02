use super::charged_payload;
use crate::{GenerationIdentifierIndex, QueryError};
use syntaxmesh_core::{
    EvidenceClass, FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, Node, NodeId,
    NodeKind, Provenance, ProvenanceId, RepositoryId, SourceLocation, SourceSpan, WorktreeId,
};
use syntaxmesh_store::{GraphStore, InMemoryGraphStore};

mod query_work;

#[test]
fn source_content_filters_reserved_metadata_before_candidate_limits() -> Result<(), QueryError> {
    let (_, mut delta) = path_fixture()?;
    let original = delta
        .upsert_nodes
        .first()
        .ok_or(QueryError::InvalidLimit)?
        .clone();
    let mut content = original;
    content.name = "audit".to_owned();
    let mut extension = content.clone();
    extension.id = NodeId::derive(&[b"legitimate-extension"]);
    extension.kind = NodeKind::External {
        namespace: "fixture.catalog".to_owned(),
        kind: "entry".to_owned(),
    };
    delta.upsert_nodes = vec![content.clone(), extension.clone()];
    for ordinal in 0_u64..20 {
        let mut metadata = content.clone();
        metadata.id = NodeId::derive(&[b"processing", &ordinal.to_le_bytes()]);
        metadata.kind = NodeKind::External {
            namespace: syntaxmesh_language_sdk::SOURCE_PROCESSING_NAMESPACE.to_owned(),
            kind: "completed.v1".to_owned(),
        };
        delta.upsert_nodes.push(metadata);
    }
    let generation = delta.next_generation;
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(delta)?;
    let filtered =
        GenerationIdentifierIndex::build_source_content(&store, generation, 22, 100, 10000)?;
    let found = filtered.candidates(&store, generation, "audit", 2, 2)?;
    let planner = GenerationIdentifierIndex::build_source_content_for_planner(
        &store, generation, 22, 100, 10000,
    )?;
    for channel in [crate::LexicalChannel::Exact, crate::LexicalChannel::Stemmed] {
        let planned = planner.lexical_candidates(
            &store,
            generation,
            &crate::LexicalCandidateRequest {
                query: "audit",
                channel,
                family: None,
                eligible: None,
                posting_budget: 2,
                limit: 2,
            },
        )?;
        check(
            planned == found,
            "planner channels retained processing candidates",
        )?;
    }
    check(
        found.len() == 2
            && found.iter().all(|candidate| {
                candidate.node.id == content.id || candidate.node.id == extension.id
            }),
        "metadata consumed content limit or unrelated extension was excluded",
    )?;
    check(
        store.search_nodes(generation, "audit", 100)?.len() == 22,
        "source-content policy changed raw search",
    )?;
    check(
        GenerationIdentifierIndex::build_source_content(&store, generation, 21, 100, 10000)
            .is_err(),
        "excluded metadata bypassed canonical node budget",
    )?;
    Ok(())
}

#[cfg(feature = "benchmark-instrumentation")]
#[test]
fn completed_build_metrics_count_work_without_timing_assumptions() -> Result<(), QueryError> {
    let (store, delta) = path_fixture()?;
    let index = GenerationIdentifierIndex::build(&store, delta.next_generation, 1, 1, 36)?;
    let metrics = index.build_metrics();
    if metrics.node_pages != 0
        || metrics.node_scans != 1
        || metrics.nodes != 1
        || metrics.postings != 1
        || metrics.payload_bytes != 36
        || metrics.elapsed
            < metrics
                .page_read_elapsed
                .saturating_add(metrics.processing_elapsed)
        || GenerationIdentifierIndex::build(&store, delta.next_generation, 1, 1, 35).is_ok()
    {
        return Err(QueryError::Context(
            "completed index metrics differ from charged work".to_owned(),
        ));
    }

    let mut empty = delta;
    empty.upsert_nodes.clear();
    let mut empty_store = InMemoryGraphStore::new();
    empty_store.apply_delta(empty.clone())?;
    let empty_index =
        GenerationIdentifierIndex::build(&empty_store, empty.next_generation, 1, 1, 1)?;
    let empty_metrics = empty_index.build_metrics();
    if empty_metrics.node_pages != 0
        || empty_metrics.node_scans != 1
        || empty_metrics.nodes != 0
        || empty_metrics.postings != 0
        || empty_metrics.payload_bytes != 0
        || empty_metrics.elapsed
            < empty_metrics
                .page_read_elapsed
                .saturating_add(empty_metrics.processing_elapsed)
    {
        return Err(QueryError::Context(
            "empty index metrics differ from charged work".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn payload_accounting_accepts_exact_limit_and_rejects_overflow() {
    assert_eq!(charged_payload(0, 5, 37).ok(), Some(37));
    assert!(charged_payload(0, 5, 36).is_err());
    assert_eq!(charged_payload(37, 0, 69).ok(), Some(69));
    assert!(charged_payload(usize::MAX, 0, usize::MAX).is_err());
    assert!(charged_payload(0, usize::MAX, usize::MAX).is_err());
}

pub(super) fn path_fixture() -> Result<(InMemoryGraphStore, GraphDelta), QueryError> {
    let file = FileVersion {
        file_id: FileId::derive(&[b"path-file"]),
        normalized_path: "scripts/load.py".to_owned(),
        content_hash: [1; 32],
        size_bytes: 1,
    };
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"path-producer"]),
        producer_namespace: "test/path".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"path-node"]),
        kind: NodeKind::Function,
        name: "load".to_owned(),
        owner_file: Some(file.file_id),
        source: Some(SourceLocation {
            file_id: file.file_id,
            content_hash: file.content_hash,
            span: SourceSpan {
                start_byte: 0,
                end_byte: 1,
            },
        }),
        provenance: provenance.id,
        extension_payload: None,
    };
    let delta = GraphDelta {
        repository: RepositoryId::derive(&[b"path-repository"]),
        worktree: WorktreeId::derive(&[b"path-worktree"]),
        run_id: IndexRunId::derive(&[b"path-run"]),
        expected_base: None,
        next_generation: GenerationId::derive(&[b"path-first"]),
        changed_files: vec![file],
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![node],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(delta.clone())?;
    Ok((store, delta))
}

#[test]
fn path_terms_deduplicate_and_charge_exact_budgets() -> Result<(), QueryError> {
    let (store, delta) = path_fixture()?;
    let generation = delta.next_generation;
    let index = GenerationIdentifierIndex::build_with_paths(&store, generation, 1, 3, 109, 1, 47)?;
    let found = index.candidates(&store, generation, "scripts load py", 3, 1)?;
    let query = crate::Query::new(&store, generation);
    let service_index = query.build_path_identifier_index(1, 3, 109, 1, 47)?;
    check(
        query.identifier_candidates(&service_index, "scripts load py", 3, 1)? == found,
        "Query path channel differs from direct index",
    )?;
    check(found.len() == 1, "unexpected candidate count")?;
    let candidate = found.first().ok_or(QueryError::InvalidLimit)?;
    check(
        Some(&candidate.node) == delta.upsert_nodes.first(),
        "canonical node differs",
    )?;
    // One node, three unique terms: duplicate label/path "load" is charged once.
    check(candidate.score == 9, "duplicate terms changed score")?;
    check(
        index
            .candidates(&store, generation, "scripts load py", 2, 1)
            .is_err(),
        "posting limit accepted partial work",
    )?;
    for (nodes, postings, payload, files, inventory) in [
        (0, 3, 109, 1, 47),
        (1, 2, 109, 1, 47),
        (1, 3, 108, 1, 47),
        (1, 3, 109, 0, 47),
        (1, 3, 109, 1, 46),
    ] {
        check(
            GenerationIdentifierIndex::build_with_paths(
                &store, generation, nodes, postings, payload, files, inventory,
            )
            .is_err(),
            "build accepted exhausted budget",
        )?;
    }
    let label_only = GenerationIdentifierIndex::build(&store, generation, 1, 1, 36)?;
    check(
        label_only
            .candidates(&store, generation, "scripts", 1, 1)?
            .is_empty(),
        "label-only channel gained path terms",
    )?;
    Ok(())
}

#[test]
fn path_inventory_is_retained_after_rename() -> Result<(), QueryError> {
    let (mut store, mut delta) = path_fixture()?;
    let first = delta.next_generation;
    let old = GenerationIdentifierIndex::build_with_paths(&store, first, 1, 3, 109, 1, 47)?;
    let expected = old.candidates(&store, first, "scripts", 1, 1)?;
    let second = GenerationId::derive(&[b"path-second"]);
    delta.expected_base = Some(first);
    delta.next_generation = second;
    delta.run_id = IndexRunId::derive(&[b"path-rename"]);
    delta
        .changed_files
        .first_mut()
        .ok_or(QueryError::InvalidLimit)?
        .normalized_path = "tools/load.py".to_owned();
    store.apply_delta(delta)?;
    let rebuilt_old = GenerationIdentifierIndex::build_with_paths(&store, first, 1, 3, 109, 1, 47)?;
    let current = GenerationIdentifierIndex::build_with_paths(&store, second, 1, 3, 109, 1, 47)?;
    check(
        rebuilt_old.candidates(&store, first, "scripts", 1, 1)? == expected,
        "rebuilt historical index lost original path",
    )?;
    check(
        old.candidates(&store, first, "scripts", 1, 1)? == expected,
        "retained index changed",
    )?;
    check(
        current
            .candidates(&store, second, "scripts", 1, 1)?
            .is_empty(),
        "current index retained old path",
    )?;
    check(
        current.candidates(&store, second, "tools", 1, 1)?.len() == 1,
        "new path missing",
    )?;
    check(
        old.candidates(&store, second, "scripts", 1, 1).is_err(),
        "foreign generation accepted",
    )?;
    Ok(())
}

fn check(condition: bool, message: &str) -> Result<(), QueryError> {
    if condition {
        Ok(())
    } else {
        Err(QueryError::Context(message.to_owned()))
    }
}

#[test]
fn path_channel_rejects_missing_files_and_foreign_manifest() -> Result<(), QueryError> {
    let (store, mut delta) = path_fixture()?;
    let generation = delta.next_generation;
    let missing = std::collections::BTreeMap::new();
    check(
        GenerationIdentifierIndex::build_with_inventory(
            &store,
            generation,
            1,
            3,
            109,
            Some(&missing),
            false,
        )
        .is_err(),
        "missing source file accepted",
    )?;
    let index = GenerationIdentifierIndex::build_with_paths(&store, generation, 1, 3, 109, 1, 47)?;
    delta.repository = RepositoryId::derive(&[b"foreign-path-repository"]);
    let mut foreign = InMemoryGraphStore::new();
    foreign.apply_delta(delta)?;
    check(
        index
            .candidates(&foreign, generation, "scripts", 1, 1)
            .is_err(),
        "foreign manifest accepted",
    )?;
    Ok(())
}

#[test]
fn path_postings_match_independent_snapshot_oracle() -> Result<(), QueryError> {
    let (_, mut delta) = path_fixture()?;
    let prototype = delta
        .upsert_nodes
        .first()
        .cloned()
        .ok_or(QueryError::InvalidLimit)?;
    for (number, name, source_backed) in [
        (1_u8, "executeWithRetry", true),
        (2, "loadMetadata", true),
        (3, "load", false),
        (4, "executeWithRetry", false),
        (5, "unrelated", false),
    ] {
        let mut node = prototype.clone();
        node.id = NodeId::derive(&[b"path-oracle", &[number]]);
        node.name = name.to_owned();
        if !source_backed {
            node.source = None;
            node.owner_file = None;
        }
        delta.upsert_nodes.push(node);
    }
    let generation = delta.next_generation;
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(delta)?;
    let index =
        GenerationIdentifierIndex::build_with_paths(&store, generation, 6, 100, 10_000, 1, 47)?;
    let snapshot = store.historical_snapshot(generation)?;
    let documents = snapshot
        .nodes
        .iter()
        .map(|node| {
            let mut terms = crate::identifier_terms(&node.name);
            if let Some(source) = &node.source {
                let file = snapshot
                    .files
                    .iter()
                    .find(|file| file.file_id == source.file_id)
                    .ok_or(QueryError::InvalidLimit)?;
                terms.extend(crate::identifier_terms(&file.normalized_path));
            }
            Ok((node, terms))
        })
        .collect::<Result<Vec<_>, QueryError>>()?;
    let population =
        u64::try_from(documents.len()).map_err(|error| QueryError::Context(error.to_string()))?;
    for query in [
        "load",
        "scripts",
        "execute retry",
        "scripts load py",
        "metadata scripts",
        "unrelated",
        "absent",
    ] {
        let query_terms = crate::identifier_terms(query);
        let mut expected = Vec::new();
        for (node, terms) in &documents {
            let mut sum = 0_u64;
            let mut coverage = 0_u64;
            for term in &query_terms {
                if terms.contains(term) {
                    let frequency = u64::try_from(
                        documents
                            .iter()
                            .filter(|(_, words)| words.contains(term))
                            .count(),
                    )
                    .map_err(|error| QueryError::Context(error.to_string()))?;
                    // Independent complete-document calculation, not posting statistics.
                    let weight = u64::from((population / (frequency + 1) + 1).ilog2()) + 1;
                    sum += weight;
                    coverage += 1;
                }
            }
            if coverage != 0 {
                expected.push(crate::RankedCandidate {
                    node: (*node).clone(),
                    score: sum * coverage,
                });
            }
        }
        expected.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.node.id.cmp(&right.node.id))
        });
        for limit in [1, 3, 6] {
            let mut selected = expected.clone();
            selected.truncate(limit);
            check(
                index.candidates(&store, generation, query, 100, limit)? == selected,
                "path postings differ from independent oracle",
            )?;
        }
    }
    Ok(())
}
