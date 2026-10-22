use super::super::{
    InferenceCheckpointArgs, InferenceCursorArgs, InferencePageArgs, SyntaxMeshMcp,
};
use super::{Fixture, content_json};
use rmcp::handler::server::wrapper::Parameters;
use syntaxmesh_api_model::{
    INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpointResponse, InferencePageResponse,
};
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, GenerationId, GraphDelta, IndexRunId, Node, NodeId, NodeKind,
    Provenance, ProvenanceId, RelationKind, RepositoryId, StableId, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference::{Limits, Rule};
use syntaxmesh_inference_host::InferenceReadBindings;
use syntaxmesh_inference_sqlite::{InferenceMaterializer, RebuildRequest};
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

    let page_limits = syntaxmesh_api_model::InferencePageLimits {
        claims: 1,
        payload_bytes: 16_384,
        premises: 100,
        tree_pages: 4096,
    };
    let mut bindings = InferenceReadBindings::new(repository, worktree, page_limits)?;
    bindings.insert("rules".to_owned(), inference_database, origin)?;
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
