use super::{
    Candidate, ContextSourceProvider, ContextTokenCounter, candidate_search_limit,
    deduplicate_candidates, finalize_count, identifier_terms, lexical_relevance, line_span,
};
use syntaxmesh_api_model::{
    CONTEXT_PACK_SCHEMA_VERSION, ContextItem, ContextItemKind, ContextPack, ContextRequest,
    OmittedContextSummary,
};
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, Node,
    NodeId, NodeKind, Provenance, ProvenanceId, RelationKind, RepositoryId, SourceLocation,
    SourceSpan, WorktreeId,
};
use syntaxmesh_store::{GraphStore, InMemoryGraphStore};

struct FixtureSource(Vec<u8>);

impl ContextSourceProvider for FixtureSource {
    fn read_source(&self, file: &FileVersion) -> Result<Option<Vec<u8>>, String> {
        if file.normalized_path == "src/migrations.rs" {
            Ok(Some(self.0.clone()))
        } else {
            Ok(None)
        }
    }
}
struct ByteCounter;

impl super::ContextTokenCounter for ByteCounter {
    fn tokenizer_id(&self) -> &str {
        "test-byte-counter-v1"
    }

    fn count_tokens(&self, serialized_pack: &str) -> Result<u64, String> {
        u64::try_from(serialized_pack.len()).map_err(|error| error.to_string())
    }
}

fn pack() -> ContextPack {
    ContextPack {
        schema_version: CONTEXT_PACK_SCHEMA_VERSION,
        query: "example".to_owned(),
        repository: RepositoryId::derive(&[b"context-test-repository"]),
        worktree: WorktreeId::derive(&[b"context-test-worktree"]),
        generation: GenerationId::derive(&[b"context-test-generation"]),
        tokenizer: "test-byte-counter-v1".to_owned(),
        token_budget: 4096,
        token_count: 0,
        items: Vec::new(),
        warnings: Vec::new(),
        omitted: OmittedContextSummary::default(),
    }
}

#[test]
fn reversible_trial_matches_clone_oracle_across_budgets() -> Result<(), crate::QueryError> {
    for budget in (500..5000).step_by(137) {
        let mut actual = pack();
        actual.token_budget = budget;
        finalize_count(&mut actual, &ByteCounter)?;
        let mut expected = actual.clone();
        for (position, text) in [
            "small",
            "large source evidence\n".repeat(200).as_str(),
            "λ architecture",
            "later small",
            "another complete item",
        ]
        .into_iter()
        .enumerate()
        {
            let item = ContextItem {
                rank: 0,
                evidence_class: None,
                kind: ContextItemKind::SourceEvidence,
                text: text.to_owned(),
                node_ids: Vec::new(),
                edge_ids: Vec::new(),
                source_path: Some("docs/design.md".to_owned()),
                line_start: Some(1),
                line_end: Some(2),
            };
            let omitted = OmittedContextSummary {
                source_evidence: u32::try_from(5_usize.saturating_sub(position))
                    .unwrap_or(u32::MAX),
                ..OmittedContextSummary::default()
            };
            let mut trial = expected.clone();
            let mut oracle_item = item.clone();
            oracle_item.rank =
                u32::try_from(trial.items.len().saturating_add(1)).unwrap_or(u32::MAX);
            trial.items.push(oracle_item);
            trial.omitted = omitted;
            finalize_count(&mut trial, &ByteCounter)?;
            let fits = trial.token_count <= budget;
            if fits {
                expected = trial;
            }
            let admitted = super::packing::try_item(&mut actual, item, omitted, &ByteCounter)?;
            if admitted != fits || actual != expected {
                return Err(crate::QueryError::Context(
                    "reversible packing diverged from clone oracle".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn failed_tokenizer_restores_the_complete_accepted_pack() -> Result<(), crate::QueryError> {
    struct FailingCounter(std::cell::Cell<usize>);
    impl ContextTokenCounter for FailingCounter {
        fn tokenizer_id(&self) -> &str {
            "failure-fixture"
        }
        fn count_tokens(&self, _payload: &str) -> Result<u64, String> {
            let calls = self.0.get();
            self.0.set(calls.saturating_add(1));
            if calls == 0 {
                Ok(1)
            } else {
                Err("fixture failure".to_owned())
            }
        }
    }
    let mut actual = pack();
    finalize_count(&mut actual, &ByteCounter)?;
    let before = actual.clone();
    let item = ContextItem {
        rank: 0,
        evidence_class: None,
        kind: ContextItemKind::Summary,
        text: "trial".to_owned(),
        node_ids: Vec::new(),
        edge_ids: Vec::new(),
        source_path: None,
        line_start: None,
        line_end: None,
    };
    let result = super::packing::try_item(
        &mut actual,
        item,
        OmittedContextSummary {
            summaries: 3,
            ..OmittedContextSummary::default()
        },
        &FailingCounter(std::cell::Cell::new(0)),
    );
    if result.is_ok() || actual != before {
        return Err(crate::QueryError::Context(
            "tokenizer failure did not restore accepted pack".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn line_span_returns_complete_one_based_lines() {
    assert_eq!(
        line_span("first\nsecond\nthird", 7, 12),
        Some(("second".to_owned(), 2, 2))
    );
    assert_eq!(
        line_span("one\ntwo", 1, 6),
        Some(("one\ntwo".to_owned(), 1, 2))
    );
}

#[test]
fn lexical_lookup_overfetch_is_bounded_and_final_candidates_stay_request_limited() {
    assert_eq!(candidate_search_limit(1), 4);
    assert_eq!(candidate_search_limit(64), 256);
    assert_eq!(candidate_search_limit(256), 1024);
    assert_eq!(candidate_search_limit(usize::MAX), 1024);
}

#[test]
fn line_span_rejects_invalid_bounds_and_utf8_boundaries() {
    assert_eq!(line_span("é", 1, 2), None);
    assert_eq!(line_span("abc", 2, 4), None);
    assert_eq!(line_span("abc", 3, 2), None);
}

#[test]
fn serialized_token_count_reaches_a_fixed_point() -> Result<(), String> {
    let counter = ByteCounter;
    let mut context = pack();
    finalize_count(&mut context, &counter).map_err(|error| error.to_string())?;
    let serialized = serde_json::to_string(&context).map_err(|error| error.to_string())?;
    if context.token_count != counter.count_tokens(&serialized)? {
        return Err("token count does not match the final serialized pack".to_owned());
    }
    Ok(())
}

#[test]
fn candidate_deduplication_is_independent_of_sort_adjacency() {
    let candidate = |key: &str, relevance| Candidate {
        kind: ContextItemKind::Signature,
        granularity: super::SourceGranularity::Local,
        relevance,
        distance: 0,
        key: key.to_owned(),
        item: ContextItem {
            rank: 0,
            evidence_class: None,
            kind: ContextItemKind::Signature,
            text: key.to_owned(),
            node_ids: Vec::new(),
            edge_ids: Vec::new(),
            source_path: None,
            line_start: None,
            line_end: None,
        },
    };
    let mut ordered = vec![
        candidate("same", 3),
        candidate("other", 2),
        candidate("same", 1),
    ];
    deduplicate_candidates(&mut ordered);
    assert_eq!(ordered.len(), 2);
    assert_eq!(ordered.first().map(|selected| selected.relevance), Some(3));
}

#[test]
fn definition_evidence_tiebreak_preserves_relevance_distance_and_key_order() {
    let item = ContextItem {
        rank: 0,
        evidence_class: None,
        kind: ContextItemKind::SourceEvidence,
        text: "complete evidence".to_owned(),
        node_ids: Vec::new(),
        edge_ids: Vec::new(),
        source_path: None,
        line_start: None,
        line_end: None,
    };
    let definition = Candidate {
        kind: ContextItemKind::SourceEvidence,
        granularity: super::SourceGranularity::Local,
        relevance: 0,
        distance: 1,
        key: "z-definition".to_owned(),
        item,
    };
    let mut reference = definition.clone();
    reference.granularity = super::SourceGranularity::Occurrence;
    reference.key = "a-reference".to_owned();
    assert!(super::candidate_order(&definition, &reference).is_lt());
    let mut container = definition.clone();
    container.granularity = super::SourceGranularity::Container;
    container.key = "a-container".to_owned();
    assert!(super::candidate_order(&reference, &container).is_lt());
    container.distance = 0;
    assert!(super::candidate_order(&container, &definition).is_lt());
    reference.distance = 0;
    assert!(super::candidate_order(&reference, &definition).is_lt());
    reference.distance = 2;
    reference.relevance = 1;
    assert!(super::candidate_order(&reference, &definition).is_lt());
    let mut earlier_definition = definition.clone();
    earlier_definition.key = "a-definition".to_owned();
    assert!(super::candidate_order(&earlier_definition, &definition).is_lt());
}

#[test]
fn source_granularity_uses_canonical_node_kinds() {
    use super::SourceGranularity;
    for kind in [
        NodeKind::File,
        NodeKind::Module,
        NodeKind::Script,
        NodeKind::Document,
    ] {
        assert_eq!(
            SourceGranularity::for_node(&kind),
            SourceGranularity::Container
        );
    }
    for kind in [
        NodeKind::Function,
        NodeKind::Section,
        NodeKind::DocumentChunk,
    ] {
        assert_eq!(SourceGranularity::for_node(&kind), SourceGranularity::Local);
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
        NodeKind::Import {
            specifier: "./module".to_owned(),
            kind: syntaxmesh_core::ImportKind::Named,
            imported_name: Some("symbol".to_owned()),
            local_name: Some("symbol".to_owned()),
            type_only: false,
        },
        NodeKind::Export {
            source_specifier: None,
            kind: syntaxmesh_core::ExportKind::Local,
            exported_name: Some("symbol".to_owned()),
            local_name: Some("symbol".to_owned()),
            type_only: false,
        },
        NodeKind::ModuleResolutionDiagnostic {
            occurrence: NodeId::derive(&[b"granularity-import"]),
            status: syntaxmesh_core::ModuleResolutionDiagnosticStatus::Unresolved,
            candidate_paths: Vec::new(),
        },
    ] {
        assert_eq!(
            SourceGranularity::for_node(&kind),
            SourceGranularity::Occurrence
        );
    }
}

#[test]
fn lexical_relevance_splits_identifier_separators_and_prefers_complete_function_symbols() {
    let terms = ["publish", "prepared", "lineage"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let function = Node {
        id: NodeId::derive(&[b"context-ranking-function"]),
        kind: NodeKind::Function,
        name: "SyntaxMeshEngine::publish_prepared_with_lineage".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"context-ranking-function-provenance"]),
        extension_payload: None,
    };
    let reference = Node {
        id: NodeId::derive(&[b"context-ranking-reference"]),
        kind: NodeKind::Reference {
            relation: RelationKind::Calls,
        },
        name: "publish_prepared_with_lineage".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"context-ranking-reference-provenance"]),
        extension_payload: None,
    };

    assert_eq!(lexical_relevance(&function, &terms), 4);
    assert_eq!(lexical_relevance(&reference, &terms), 3);

    let camel_case = Node {
        id: NodeId::derive(&[b"context-ranking-camel-case"]),
        kind: NodeKind::Function,
        name: "generateBlockDoc".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"context-ranking-camel-case-provenance"]),
        extension_payload: None,
    };
    let camel_terms = ["generate", "block", "documentation", "page"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(lexical_relevance(&camel_case, &camel_terms), 2);
    let exact_symbol = Node {
        id: NodeId::derive(&[b"context-ranking-exact-symbol"]),
        kind: NodeKind::Function,
        name: "audit".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"context-ranking-exact-symbol-provenance"]),
        extension_payload: None,
    };
    let audit_terms = ["audit", "packet", "revision"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(lexical_relevance(&exact_symbol, &audit_terms), 5);
    let exact_query = ["audit".to_owned()].into_iter().collect();
    assert_eq!(lexical_relevance(&exact_symbol, &exact_query), 3);
    let generic_page = Node {
        name: "Page".to_owned(),
        ..exact_symbol
    };
    assert_eq!(lexical_relevance(&generic_page, &camel_terms), 6);
    assert!(
        lexical_relevance(&camel_case, &camel_terms)
            < lexical_relevance(&generic_page, &camel_terms)
    );
    assert_eq!(
        identifier_terms("HTTPClient::generateBlockDoc"),
        ["block", "client", "doc", "generate", "http"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
}

#[test]
fn context_compilation_keeps_complete_evidence_and_reports_ambiguity_and_omissions()
-> Result<(), crate::QueryError> {
    let text = "pub fn sqlite_migration_runner() { apply_migration(); }\npub fn sqlite_migration_ledger() {}\npub fn unrelated_network_client() {}\n";
    let bytes = text.as_bytes().to_vec();
    let file_id = FileId::derive(&[b"context-retrieval-fixture-file"]);
    let content_hash = *blake3::hash(&bytes).as_bytes();
    let file = FileVersion {
        file_id,
        normalized_path: "src/migrations.rs".to_owned(),
        content_hash,
        size_bytes: u64::try_from(bytes.len()).map_err(|error| {
            crate::QueryError::Context(format!("convert fixture size: {error}"))
        })?,
    };
    let span_for = |source: &str| -> Result<SourceLocation, crate::QueryError> {
        let start = text.find(source).ok_or_else(|| {
            crate::QueryError::Context("expected function source is missing".to_owned())
        })?;
        let end = start.checked_add(source.len()).ok_or_else(|| {
            crate::QueryError::Context("fixture source span overflowed".to_owned())
        })?;
        Ok(SourceLocation {
            file_id,
            content_hash,
            span: SourceSpan {
                start_byte: u64::try_from(start).map_err(|error| {
                    crate::QueryError::Context(format!("convert span start: {error}"))
                })?,
                end_byte: u64::try_from(end).map_err(|error| {
                    crate::QueryError::Context(format!("convert span end: {error}"))
                })?,
            },
        })
    };
    let runner_source = "pub fn sqlite_migration_runner() { apply_migration(); }";
    let ledger_source = "pub fn sqlite_migration_ledger() {}";
    let runner_location = span_for(runner_source)?;
    let ledger_location = span_for(ledger_source)?;
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"context-retrieval-fixture-provenance"]),
        producer_namespace: "syntaxmesh.context.fixture".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: Some(runner_location.clone()),
    };
    let runner = Node {
        id: NodeId::derive(&[b"context-retrieval-runner"]),
        kind: NodeKind::Function,
        name: "sqlite_migration_runner".to_owned(),
        owner_file: Some(file_id),
        source: Some(runner_location),
        provenance: provenance.id,
        extension_payload: None,
    };
    let ledger = Node {
        id: NodeId::derive(&[b"context-retrieval-ledger"]),
        kind: NodeKind::Function,
        name: "sqlite_migration_ledger".to_owned(),
        owner_file: Some(file_id),
        source: Some(ledger_location),
        provenance: provenance.id,
        extension_payload: None,
    };
    let generation = GenerationId::derive(&[b"context-retrieval-fixture-generation"]);
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository: RepositoryId::derive(&[b"context-retrieval-fixture-repository"]),
        worktree: WorktreeId::derive(&[b"context-retrieval-fixture-worktree"]),
        run_id: IndexRunId::derive(&[b"context-retrieval-fixture-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: vec![file],
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![runner.clone(), ledger.clone()],
        upsert_edges: vec![Edge {
            id: EdgeId::derive(&[b"context-retrieval-call-edge"]),
            source: runner.id,
            target: ledger.id,
            relation: RelationKind::Calls,
            provenance: runner.provenance,
            extension_payload: None,
        }],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let query = crate::Query::new(&store, generation);
    let source = FixtureSource(bytes);
    let request = ContextRequest {
        query: "sqlite migration".to_owned(),
        seed_nodes: Vec::new(),
        token_budget: 4096,
        max_hops: 1,
        max_candidates: 8,
    };
    let complete = query.context(&request, &source, &ByteCounter)?;
    verify_seeded_context(&query, &source, &request, runner.id, ledger.id)?;
    let selection = query.context_selection(&request)?;
    if selection.generation != generation
        || selection.nodes.len() != 2
        || selection.nodes.windows(2).any(|pair| {
            pair.first()
                .zip(pair.get(1))
                .is_some_and(|(left, right)| left.0 >= right.0)
        })
        || selection != query.historical_context_selection(&request)?
        || selection.warnings != complete.warnings
    {
        return Err(crate::QueryError::Context(
            "source-free selection report differs from compiler selection".to_owned(),
        ));
    }
    if complete.generation != generation
        || complete.token_count > complete.token_budget
        || complete.items.len() != 7
        || complete.warnings.first().is_none_or(|warning| {
            warning.code != "ambiguous_lexical_seeds" || warning.node_ids.len() != 2
        })
    {
        return Err(crate::QueryError::Context(
            "context fixture did not preserve its tied, generation-pinned candidates".to_owned(),
        ));
    }
    let source_items = complete
        .items
        .iter()
        .filter(|item| item.kind == ContextItemKind::SourceEvidence)
        .collect::<Vec<_>>();
    if source_items.len() != 2
        || !source_items.iter().any(|item| {
            item.text.contains(runner_source)
                && item.source_path.as_deref() == Some("src/migrations.rs")
                && item.line_start == Some(1)
                && item.line_end == Some(1)
        })
        || !source_items.iter().any(|item| {
            item.text.contains(ledger_source)
                && item.source_path.as_deref() == Some("src/migrations.rs")
                && item.line_start == Some(2)
                && item.line_end == Some(2)
        })
    {
        return Err(crate::QueryError::Context(
            "context fixture source evidence is incomplete or has incorrect locations".to_owned(),
        ));
    }
    if query.context(&request, &source, &ByteCounter)? != complete {
        return Err(crate::QueryError::Context(
            "repeated context compilation was not deterministic".to_owned(),
        ));
    }
    let limited_request = ContextRequest {
        max_candidates: 1,
        ..request.clone()
    };
    for (term_query, expected) in [
        ("aa ab ac ad ae af ag ah ai aj ak al am an ao ap", false),
        ("aa ab ac ad ae af ag ah ai aj ak al am an ao ap aq", true),
        ("aa ab ac ad ae af ag ah ai aj ak al am an ao ap aa", false),
    ] {
        let term_limited = ContextRequest {
            query: term_query.to_owned(),
            ..request.clone()
        };
        let term_pack = query.context(&term_limited, &source, &ByteCounter)?;
        let warning_count = term_pack
            .warnings
            .iter()
            .filter(|warning| warning.code == "lexical_query_terms_truncated")
            .count();
        if warning_count != usize::from(expected) {
            return Err(crate::QueryError::Context(
                "query term warning did not respect the distinct-term boundary".to_owned(),
            ));
        }
    }
    let limited = query.context(&limited_request, &source, &ByteCounter)?;
    if !limited
        .warnings
        .iter()
        .any(|warning| warning.code == "lexical_candidates_truncated")
        || limited
            .warnings
            .iter()
            .any(|warning| warning.code == "lexical_lookup_limit_reached")
    {
        return Err(crate::QueryError::Context(
            "candidate truncation warning did not distinguish a complete lookup".to_owned(),
        ));
    }

    let mut omitted_pack = None;
    for budget in (1..complete.token_count).rev() {
        let constrained = ContextRequest {
            token_budget: budget,
            ..request.clone()
        };
        match query.context(&constrained, &source, &ByteCounter) {
            Ok(pack) if pack.omitted.total() > 0 => {
                omitted_pack = Some(pack);
                break;
            }
            Ok(_) | Err(crate::QueryError::ContextBudgetTooSmall { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    let omitted_pack = omitted_pack.ok_or_else(|| {
        crate::QueryError::Context(
            "no feasible constrained budget reported omitted complete evidence".to_owned(),
        )
    })?;
    if omitted_pack.token_count > omitted_pack.token_budget || omitted_pack.omitted.total() == 0 {
        return Err(crate::QueryError::Context(
            "omitted evidence was not transparently reported within budget".to_owned(),
        ));
    }
    let saturated_generation = GenerationId::derive(&[b"context-saturated-lookup"]);
    store.apply_delta(GraphDelta {
        repository: RepositoryId::derive(&[b"context-retrieval-fixture-repository"]),
        worktree: WorktreeId::derive(&[b"context-retrieval-fixture-worktree"]),
        run_id: IndexRunId::derive(&[b"context-saturated-run"]),
        expected_base: Some(generation),
        next_generation: saturated_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: (0_u64..4)
            .map(|ordinal| Node {
                id: NodeId::derive(&[b"context-lookup-copy", &ordinal.to_le_bytes()]),
                name: format!("sqlite_migration_copy_{ordinal}"),
                ..runner.clone()
            })
            .collect(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let saturated = crate::Query::new(&store, saturated_generation).context(
        &limited_request,
        &source,
        &ByteCounter,
    )?;
    for code in [
        "lexical_lookup_limit_reached",
        "lexical_candidates_truncated",
    ] {
        if saturated
            .warnings
            .iter()
            .filter(|warning| warning.code == code)
            .count()
            != 1
        {
            return Err(crate::QueryError::Context(format!(
                "saturated lookup omitted or duplicated {code}"
            )));
        }
    }
    Ok(())
}

fn verify_seeded_context(
    query: &crate::Query<'_, InMemoryGraphStore>,
    source: &FixtureSource,
    request: &ContextRequest,
    runner: NodeId,
    ledger: NodeId,
) -> Result<(), crate::QueryError> {
    verify_ranked_seeded_context(query, source, request, runner, ledger)?;
    let empty = ContextRequest {
        seed_nodes: Vec::new(),
        max_hops: 0,
        max_candidates: 1,
        ..request.clone()
    };
    let empty_selection = query.ranked_plan_context_selection(&empty)?;
    let empty_pack = query.ranked_plan_context(&empty, source, &ByteCounter)?;
    if !empty_selection.nodes.is_empty()
        || empty_selection != query.historical_ranked_plan_context_selection(&empty)?
        || !empty_pack.items.is_empty()
        || empty_pack != query.historical_ranked_plan_context(&empty, source, &ByteCounter)?
        || empty_pack.token_count > empty.token_budget
    {
        return Err(crate::QueryError::Context(
            "empty plan performed fallback or changed historical semantics".to_owned(),
        ));
    }
    let too_small = ContextRequest {
        token_budget: 1,
        ..empty
    };
    if !matches!(
        query.ranked_plan_context(&too_small, source, &ByteCounter),
        Err(crate::QueryError::ContextBudgetTooSmall { .. })
    ) {
        return Err(crate::QueryError::Context(
            "empty plan bypassed serialized budget".to_owned(),
        ));
    }
    let mut seeded = ContextRequest {
        seed_nodes: vec![runner, runner],
        max_hops: 0,
        max_candidates: 1,
        ..request.clone()
    };
    let selection = query.seeded_context_selection(&seeded)?;
    if selection.nodes != vec![(runner, 0, u32::MAX)]
        || !selection.warnings.is_empty()
        || selection != query.historical_seeded_context_selection(&seeded)?
    {
        return Err(crate::QueryError::Context(
            "seeded selection changed membership or history semantics".to_owned(),
        ));
    }
    let compiled = query.seeded_context(&seeded, source, &ByteCounter)?;
    if compiled.query != request.query
        || compiled.generation != query.generation()
        || compiled
            .items
            .iter()
            .any(|item| item.node_ids.contains(&ledger))
        || compiled != query.historical_seeded_context(&seeded, source, &ByteCounter)?
        || compiled != query.seeded_context(&seeded, source, &ByteCounter)?
    {
        return Err(crate::QueryError::Context(
            "seeded compilation lost query identity or determinism".to_owned(),
        ));
    }
    let serialized = serde_json::to_string(&compiled)
        .map_err(|error| crate::QueryError::Context(error.to_string()))?;
    if compiled.token_count
        != ByteCounter
            .count_tokens(&serialized)
            .map_err(crate::QueryError::Context)?
        || compiled.token_count > compiled.token_budget
    {
        return Err(crate::QueryError::Context(
            "seeded compilation violated exact token budget".to_owned(),
        ));
    }

    seeded.max_candidates = 8;
    if query.context_selection(&seeded)?.nodes.len() != 2
        || query.seeded_context_selection(&seeded)?.nodes.len() != 1
    {
        return Err(crate::QueryError::Context(
            "explicit selection performed lexical lookup".to_owned(),
        ));
    }
    seeded.max_hops = 1;
    let expanded = query.seeded_context_selection(&seeded)?;
    if !expanded
        .nodes
        .iter()
        .any(|(id, depth, _)| *id == ledger && *depth == 1)
        || expanded != query.historical_seeded_context_selection(&seeded)?
    {
        return Err(crate::QueryError::Context(
            "seeded graph expansion diverged".to_owned(),
        ));
    }

    for seeds in [Vec::new(), vec![runner, ledger], vec![runner; 33]] {
        let invalid = ContextRequest {
            seed_nodes: seeds,
            max_candidates: 1,
            ..seeded.clone()
        };
        if !matches!(
            query.seeded_context_selection(&invalid),
            Err(crate::QueryError::InvalidContextRequest)
        ) || !matches!(
            query.historical_seeded_context_selection(&invalid),
            Err(crate::QueryError::InvalidContextRequest)
        ) || !matches!(
            query.seeded_context(&invalid, source, &ByteCounter),
            Err(crate::QueryError::InvalidContextRequest)
        ) || !matches!(
            query.historical_seeded_context(&invalid, source, &ByteCounter),
            Err(crate::QueryError::InvalidContextRequest)
        ) {
            return Err(crate::QueryError::Context(
                "invalid seed budget was accepted".to_owned(),
            ));
        }
    }
    let unknown = NodeId::derive(&[b"unknown-context-seed"]);
    seeded.seed_nodes = vec![unknown];
    if !matches!(query.seeded_context_selection(&seeded), Err(crate::QueryError::UnknownSeed(id)) if id == unknown)
        || !matches!(query.historical_seeded_context_selection(&seeded), Err(crate::QueryError::UnknownSeed(id)) if id == unknown)
    {
        return Err(crate::QueryError::Context(
            "unknown explicit seed was accepted".to_owned(),
        ));
    }
    Ok(())
}

fn verify_ranked_seeded_context(
    query: &crate::Query<'_, InMemoryGraphStore>,
    source: &FixtureSource,
    request: &ContextRequest,
    runner: NodeId,
    ledger: NodeId,
) -> Result<(), crate::QueryError> {
    let mut plan = ContextRequest {
        seed_nodes: vec![runner, runner, ledger],
        max_hops: 0,
        max_candidates: 2,
        token_budget: 100_000,
        ..request.clone()
    };
    let selection = query.ranked_seeded_context_selection(&plan)?;
    if selection != query.historical_ranked_seeded_context_selection(&plan)?
        || !selection.nodes.contains(&(runner, 0, u32::MAX))
        || !selection.nodes.contains(&(ledger, 0, u32::MAX - 1))
        || selection.nodes.len() != 2
    {
        return Err(crate::QueryError::Context(
            "ranked duplicate priority or history differs".to_owned(),
        ));
    }
    let forward = query.ranked_seeded_context(&plan, source, &ByteCounter)?;
    let mut packing_plan = plan.clone();
    packing_plan.max_candidates = 40;
    packing_plan.seed_nodes = vec![runner; 39];
    packing_plan.seed_nodes.push(ledger);
    if query.ranked_plan_context_selection(&packing_plan)? != selection
        || query.historical_ranked_plan_context_selection(&packing_plan)? != selection
        || query.ranked_plan_context(&packing_plan, source, &ByteCounter)? != forward
        || query.historical_ranked_plan_context(&packing_plan, source, &ByteCounter)? != forward
        || !matches!(
            query.ranked_seeded_context_selection(&packing_plan),
            Err(crate::QueryError::InvalidContextRequest)
        )
    {
        return Err(crate::QueryError::Context(
            "packing plan changes identities, priorities, evidence or seed limits".to_owned(),
        ));
    }
    for invalid in [
        ContextRequest {
            max_hops: 1,
            ..packing_plan.clone()
        },
        ContextRequest {
            max_candidates: 39,
            ..packing_plan.clone()
        },
        ContextRequest {
            seed_nodes: vec![runner; 257],
            max_candidates: 256,
            ..packing_plan.clone()
        },
    ] {
        if !matches!(
            query.ranked_plan_context_selection(&invalid),
            Err(crate::QueryError::InvalidContextRequest)
        ) || !matches!(
            query.historical_ranked_plan_context_selection(&invalid),
            Err(crate::QueryError::InvalidContextRequest)
        ) || !matches!(
            query.ranked_plan_context(&invalid, source, &ByteCounter),
            Err(crate::QueryError::InvalidContextRequest)
        ) || !matches!(
            query.historical_ranked_plan_context(&invalid, source, &ByteCounter),
            Err(crate::QueryError::InvalidContextRequest)
        ) {
            return Err(crate::QueryError::Context(
                "invalid bounded packing plan accepted".to_owned(),
            ));
        }
    }
    if forward != query.historical_ranked_seeded_context(&plan, source, &ByteCounter)?
        || forward.query != plan.query
    {
        return Err(crate::QueryError::Context(
            "ranked pack loses question or history".to_owned(),
        ));
    }
    let unordered = query.seeded_context(&plan, source, &ByteCounter)?;
    let mut constrained_priority_verified = false;
    for budget in (256..forward.token_count).step_by(32) {
        let mut limited = plan.clone();
        limited.token_budget = budget;
        let first = query.ranked_seeded_context(&limited, source, &ByteCounter);
        limited.seed_nodes = vec![ledger, runner];
        let second = query.ranked_seeded_context(&limited, source, &ByteCounter);
        let (first, second) = match (first, second) {
            (Ok(first), Ok(second)) => (first, second),
            (Err(crate::QueryError::ContextBudgetTooSmall { .. }), _)
            | (_, Err(crate::QueryError::ContextBudgetTooSmall { .. })) => continue,
            (Err(error), _) | (_, Err(error)) => return Err(error),
        };
        let source_ids = |pack: &syntaxmesh_api_model::ContextPack| {
            pack.items
                .iter()
                .filter(|item| item.kind == ContextItemKind::SourceEvidence)
                .flat_map(|item| item.node_ids.iter().copied())
                .collect::<BTreeSet<_>>()
        };
        if source_ids(&first) == BTreeSet::from([runner])
            && source_ids(&second) == BTreeSet::from([ledger])
        {
            if first.token_count > budget
                || second.token_count > budget
                || first.omitted.source_evidence == 0
                || second.omitted.source_evidence == 0
            {
                return Err(crate::QueryError::Context(
                    "ranked constrained packing violates bounds or omissions".to_owned(),
                ));
            }
            constrained_priority_verified = true;
            break;
        }
    }
    if !constrained_priority_verified {
        return Err(crate::QueryError::Context(
            "no constrained budget demonstrates ranked source admission".to_owned(),
        ));
    }
    plan.seed_nodes = vec![ledger, runner, ledger];
    let reverse = query.ranked_seeded_context(&plan, source, &ByteCounter)?;
    if query.seeded_context(&plan, source, &ByteCounter)? != unordered
        || reverse
            .items
            .first()
            .is_none_or(|item| !item.node_ids.contains(&ledger))
        || forward
            .items
            .first()
            .is_none_or(|item| !item.node_ids.contains(&runner))
    {
        return Err(crate::QueryError::Context(
            "ranked source priority or ordinary ordering changed".to_owned(),
        ));
    }
    let encoded = serde_json::to_string(&reverse)
        .map_err(|error| crate::QueryError::Context(error.to_string()))?;
    if reverse.token_count
        != ByteCounter
            .count_tokens(&encoded)
            .map_err(crate::QueryError::Context)?
    {
        return Err(crate::QueryError::Context(
            "ranked serialized count differs".to_owned(),
        ));
    }
    plan.seed_nodes = vec![runner];
    plan.max_hops = 1;
    if !query
        .ranked_seeded_context_selection(&plan)?
        .nodes
        .contains(&(ledger, 1, 0))
    {
        return Err(crate::QueryError::Context(
            "ranked priority propagated to neighbor".to_owned(),
        ));
    }
    plan.max_candidates = 1;
    for seeds in [Vec::new(), vec![runner; 33], vec![runner, ledger]] {
        plan.seed_nodes = seeds;
        if !matches!(
            query.ranked_seeded_context_selection(&plan),
            Err(crate::QueryError::InvalidContextRequest)
        ) || !matches!(
            query.historical_ranked_seeded_context(&plan, source, &ByteCounter),
            Err(crate::QueryError::InvalidContextRequest)
        ) {
            return Err(crate::QueryError::Context(
                "invalid ranked request accepted".to_owned(),
            ));
        }
    }
    let unknown = NodeId::derive(&[b"unknown-ranked-context-seed"]);
    plan.seed_nodes = vec![unknown];
    if !matches!(query.ranked_seeded_context_selection(&plan), Err(crate::QueryError::UnknownSeed(id)) if id == unknown)
        || !matches!(query.historical_ranked_seeded_context(&plan, source, &ByteCounter), Err(crate::QueryError::UnknownSeed(id)) if id == unknown)
    {
        return Err(crate::QueryError::Context(
            "unknown ranked seed accepted".to_owned(),
        ));
    }
    Ok(())
}
use std::collections::BTreeSet;
