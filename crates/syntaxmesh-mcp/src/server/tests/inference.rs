use super::super::{
    ContractHistoryArgs, ExplainInferenceClaimArgs, InferenceCheckpointArgs, InferenceCursorArgs,
    InferencePageArgs, SyntaxMeshMcp,
};
use super::{Fixture, content_json};
use rmcp::handler::server::wrapper::Parameters;
use syntaxmesh_api_model::{
    INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpointResponse, InferenceExplanation,
    InferencePageResponse,
};
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, GenerationId, GraphDelta, IndexRunId, Node, NodeId, NodeKind,
    Provenance, ProvenanceId, RelationKind, RepositoryId, StableId, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference::{
    Contract, ContractLimits, ContractOrigin, Limits, RelationExpectation, Rule,
};
use syntaxmesh_inference_host::InferenceReadBindings;
use syntaxmesh_inference_sqlite::{ContractHistoryRequest, InferenceMaterializer, RebuildRequest};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::GraphStore;
use syntaxmesh_store_turso::TursoGraphStore;

#[test]
fn mcp_inference_tools_serve_retained_checkpoint_and_claim_page()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let root = std::fs::canonicalize(fixture.0.path())?;
    let canonical_database = root.join("graph.db");
    let inference_database = root.join("inference.db");
    TursoGraphStore::migrate(&canonical_database)?;
    InferenceMaterializer::migrate(&inference_database)?;

    let repository = RepositoryId::derive(&[b"mcp-retained-inference"]);
    let worktree = WorktreeId::derive(&[b"mcp-retained-inference"]);
    let first = GenerationId::derive(&[b"mcp-retained-inference-first"]);
    let generation = GenerationId::derive(&[b"mcp-retained-inference-declared"]);
    let [entry, writer, target, concept] =
        ["entry", "writer", "target", "concept"].map(|name| NodeId::derive(&[name.as_bytes()]));
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"mcp-retained-inference-provenance"]),
        producer_namespace: "test.inference".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let external = |relation: &str| RelationKind::External {
        namespace: "example".to_owned(),
        relation: relation.to_owned(),
    };
    let edge = |name: &[u8], source, destination, relation| Edge {
        id: EdgeId::derive(&[name]),
        source,
        target: destination,
        relation,
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut store = TursoGraphStore::open(&canonical_database)?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"mcp-retained-inference-first-run"]),
        expected_base: None,
        next_generation: first,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance.clone()],
        upsert_nodes: [entry, writer, target, concept]
            .into_iter()
            .map(|id| Node {
                id,
                kind: NodeKind::Function,
                name: id.0.to_hex(),
                owner_file: None,
                source: None,
                provenance: provenance.id,
                extension_payload: None,
            })
            .collect(),
        upsert_edges: vec![edge(b"calls", entry, writer, RelationKind::Calls)],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"mcp-retained-inference-declaration-run"]),
        expected_base: Some(first),
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: vec![
            edge(b"writes", writer, target, external("writes")),
            edge(b"instance", target, concept, external("instance_of")),
        ],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let first_rule = Rule {
        id: "example:write_path".to_owned(),
        version: "1".to_owned(),
        ontology_versions: std::collections::BTreeMap::new(),
        configuration_fingerprint: StableId::derive("config", &[b"mcp-retained-inference"]),
        path_relation: RelationKind::Calls,
        join_relation: external("writes"),
        declaration_relation: external("instance_of"),
        concept,
        output_relation: external("participates"),
    };
    let second_rule = Rule {
        id: "example:write_path_alt".to_owned(),
        configuration_fingerprint: StableId::derive("config", &[b"mcp-retained-inference-alt"]),
        ..first_rule.clone()
    };
    let rules = [first_rule, second_rule];
    let contracts = [Contract {
        id: "example:writer_uses_target".to_owned(),
        version: "1".to_owned(),
        origin: ContractOrigin::Declared,
        source: writer,
        target,
        relation: external("writes"),
        expectation: RelationExpectation::Required,
    }];
    let contract_limits = ContractLimits {
        contracts: 4,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 4,
    };
    let limits = Limits {
        input_bytes: 16_384,
        facts: 20,
        edge_visits: 100,
        depth: 2,
        claims: 10,
    };
    let rebuild = RebuildRequest {
        generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 2,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let mut engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    let origin = engine.initialize_inference_stream(&inference_database, &rebuild, 4096)?;
    drop(engine);

    let history_store = TursoGraphStore::open(&canonical_database)?;
    let mut materializer = InferenceMaterializer::open(&inference_database)?;
    for generation_to_retain in [first, generation] {
        materializer.retain_contract_history(
            &history_store,
            &ContractHistoryRequest {
                generation: generation_to_retain,
                contracts: &contracts,
                limits: &contract_limits,
                history_scan_budget: 100,
                output_bytes: 16_384,
            },
        )?;
    }

    let later_generation = GenerationId::derive(&[b"mcp-retained-inference-later"]);
    let mut later_store = TursoGraphStore::open(&canonical_database)?;
    later_store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"mcp-retained-inference-later-run"]),
        expected_base: Some(generation),
        next_generation: later_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: vec![Node {
            id: NodeId::derive(&[b"mcp-retained-inference-later-unrelated"]),
            kind: NodeKind::Function,
            name: "later unrelated node".to_owned(),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        }],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    drop(later_store);

    let page_limits = syntaxmesh_api_model::InferencePageLimits {
        claims: 1,
        payload_bytes: 16_384,
        premises: 100,
        tree_pages: 4096,
    };
    let mut bindings = InferenceReadBindings::new(repository, worktree, page_limits)?;
    bindings.insert("rules".to_owned(), inference_database.clone(), origin)?;
    let server =
        SyntaxMeshMcp::open(&canonical_database, &root, "cl100k_base")?.with_inference(bindings)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let checkpoint_result = runtime.block_on(server.inference_checkpoint(Parameters(
        InferenceCheckpointArgs {
            schema_version: INFERENCE_READ_SCHEMA_VERSION,
            binding: "rules".to_owned(),
            generation: generation.0.to_hex(),
        },
    )))?;
    let checkpoint: InferenceCheckpointResponse =
        serde_json::from_value(content_json(&checkpoint_result)?)?;
    let retained = checkpoint
        .checkpoint
        .ok_or("MCP omitted the retained checkpoint")?;
    if checkpoint.schema_version != INFERENCE_READ_SCHEMA_VERSION
        || checkpoint.binding != "rules"
        || checkpoint.generation != generation
        || retained.active_claims != 2
    {
        return Err("MCP checkpoint differs from retained generation".into());
    }

    let history_first =
        runtime.block_on(server.contract_history(Parameters(ContractHistoryArgs {
            schema_version: INFERENCE_READ_SCHEMA_VERSION,
            binding: "rules".to_owned(),
            contract_id: "example:writer_uses_target".to_owned(),
            contract_version: "1".to_owned(),
            after_generation: None,
            after_sequence: None,
            limit: Some(1),
        })))?;
    let first_history: serde_json::Value = content_json(&history_first)?;
    let first_item = first_history
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .ok_or("MCP retained contract history page is empty")?;
    let first_evaluation = first_item
        .get("evaluation")
        .ok_or("MCP retained contract evaluation is missing")?;
    let history_cursor = first_history
        .get("next_cursor")
        .filter(|cursor| !cursor.is_null())
        .ok_or("MCP retained contract history cursor is missing")?;
    let first_generation_hex = first.0.to_hex();
    if first_history
        .get("binding")
        .and_then(serde_json::Value::as_str)
        != Some("rules")
        || first_history
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        || first_evaluation
            .get("generation")
            .and_then(serde_json::Value::as_str)
            != Some(first_generation_hex.as_str())
        || first_evaluation
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            != Some("insufficient_evidence")
    {
        return Err(format!(
            "MCP contract history did not return the retained first generation: {first_history}"
        )
        .into());
    }
    let database_before_history_read = std::fs::read(&inference_database)?;
    let history_next = runtime.block_on(
        server.contract_history(Parameters(ContractHistoryArgs {
            schema_version: INFERENCE_READ_SCHEMA_VERSION,
            binding: "rules".to_owned(),
            contract_id: "example:writer_uses_target".to_owned(),
            contract_version: "1".to_owned(),
            after_generation: Some(
                history_cursor
                    .get("generation")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("contract history cursor generation missing")?
                    .to_owned(),
            ),
            after_sequence: Some(
                history_cursor
                    .get("generation_sequence")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or("contract history cursor sequence missing")?,
            ),
            limit: Some(1),
        })),
    )?;
    let next_history: serde_json::Value = content_json(&history_next)?;
    let next_item = next_history
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .ok_or("MCP retained contract history continuation is empty")?;
    let next_evaluation = next_item
        .get("evaluation")
        .ok_or("MCP retained continuation evaluation is missing")?;
    let next_generation_hex = generation.0.to_hex();
    if next_evaluation
        .get("generation")
        .and_then(serde_json::Value::as_str)
        != Some(next_generation_hex.as_str())
        || next_evaluation
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            != Some("supported")
        || !next_history
            .get("next_cursor")
            .is_some_and(serde_json::Value::is_null)
        || std::fs::read(&inference_database)? != database_before_history_read
    {
        return Err("MCP contract history continuation changed or mutated retained results".into());
    }

    let first_page_result =
        runtime.block_on(server.inference_claims_page(Parameters(InferencePageArgs {
            schema_version: INFERENCE_READ_SCHEMA_VERSION,
            binding: "rules".to_owned(),
            generation: generation.0.to_hex(),
            claims: page_limits.claims,
            payload_bytes: page_limits.payload_bytes,
            premises: page_limits.premises,
            tree_pages: page_limits.tree_pages,
            cursor: None,
        })))?;
    let first_page: InferencePageResponse =
        serde_json::from_value(content_json(&first_page_result)?)?;
    if first_page.schema_version != INFERENCE_READ_SCHEMA_VERSION
        || first_page.binding != "rules"
        || first_page.checkpoint != retained
        || first_page.claims.len() != 1
    {
        return Err("MCP first claim page did not preserve its continuation".into());
    }
    let first_claim = first_page
        .claims
        .first()
        .ok_or("MCP first page omitted its claim")?;
    let matching_rule = rules
        .iter()
        .find(|rule| rule.id == first_claim.rule_id)
        .ok_or("MCP claim refers to an unconfigured rule")?;
    let explanation_result = runtime.block_on(server.explain_inference_claim(Parameters(
        ExplainInferenceClaimArgs {
            claim_json: serde_json::to_string(first_claim)?,
            rule_json: serde_json::to_string(matching_rule)?,
        },
    )))?;
    let explanation: InferenceExplanation =
        serde_json::from_value(content_json(&explanation_result)?)?;
    if explanation.claim.id != first_claim.id
        || explanation.selection.generation != generation
        || explanation.selection.generation == later_generation
        || explanation.premises.len() != first_claim.premises.len()
        || explanation
            .premises
            .iter()
            .zip(&first_claim.premises)
            .any(|(evidence, reference)| {
                evidence.reference != *reference
                    || evidence.provenance.id != evidence.edge.provenance
            })
    {
        return Err(
            "MCP explanation did not verify the historical claim and exact evidence".into(),
        );
    }
    let wrong_rule = rules
        .iter()
        .find(|rule| rule.id != matching_rule.id)
        .ok_or("missing mismatched rule")?;
    let wrong_rule = runtime.block_on(server.explain_inference_claim(Parameters(
        ExplainInferenceClaimArgs {
            claim_json: serde_json::to_string(first_claim)?,
            rule_json: serde_json::to_string(wrong_rule)?,
        },
    )));
    if wrong_rule.is_ok() {
        return Err("MCP explanation accepted a mismatched rule definition".into());
    }
    let oversized = runtime.block_on(server.explain_inference_claim(Parameters(
        ExplainInferenceClaimArgs {
            claim_json: "x".repeat(256 * 1024),
            rule_json: String::new(),
        },
    )));
    if oversized.is_ok() {
        return Err("MCP explanation accepted input beyond its byte budget".into());
    }
    let first_id = first_page
        .claims
        .first()
        .ok_or("MCP first claim page is empty")?
        .id;
    let cursor = first_page.next.ok_or("MCP first page omitted its cursor")?;
    let second_page_result =
        runtime.block_on(server.inference_claims_page(Parameters(InferencePageArgs {
            schema_version: INFERENCE_READ_SCHEMA_VERSION,
            binding: "rules".to_owned(),
            generation: generation.0.to_hex(),
            claims: page_limits.claims,
            payload_bytes: page_limits.payload_bytes,
            premises: page_limits.premises,
            tree_pages: page_limits.tree_pages,
            cursor: Some(InferenceCursorArgs {
                binding: cursor.binding,
                stream: cursor.stream.to_hex(),
                repository: cursor.selection.repository.0.to_hex(),
                worktree: cursor.selection.worktree.0.to_hex(),
                generation: cursor.selection.generation.0.to_hex(),
                root: cursor.root.map(|root_id| root_id.to_hex()),
                after: cursor.after.to_hex(),
            }),
        })))?;
    let second_page: InferencePageResponse =
        serde_json::from_value(content_json(&second_page_result)?)?;
    let second_id = second_page
        .claims
        .first()
        .ok_or("MCP second claim page is empty")?
        .id;
    if second_page.schema_version != INFERENCE_READ_SCHEMA_VERSION
        || second_page.binding != "rules"
        || second_page.checkpoint != retained
        || second_page.claims.len() != 1
        || second_page.next.is_some()
        || second_id == first_id
    {
        return Err("MCP cursor did not return the next retained claim".into());
    }
    drop(runtime);
    drop(server);
    Ok(())
}
