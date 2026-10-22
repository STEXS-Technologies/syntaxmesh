use super::*;
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ContentBlock},
    transport::StreamableHttpClientTransport,
};
use syntaxmesh_api_model::{
    INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpointRequest, InferenceCheckpointResponse,
    InferencePageLimits, InferencePageRequest, InferencePageResponse,
};
use syntaxmesh_core::{
    EvidenceClass, GenerationId, GraphDelta, IndexRunId, Node, NodeId, NodeKind, Provenance,
    ProvenanceId, RelationKind, StableId,
};
use syntaxmesh_inference::{Limits, Rule};
use syntaxmesh_inference_sqlite::{InferenceMaterializer, RebuildRequest, StreamOrigin};
use syntaxmesh_source_host::{ProjectConfig, configured_source_engine, repository_scope};
use syntaxmesh_store::GraphStore;

struct PreparedInference {
    origin: StreamOrigin,
    generation: GenerationId,
    limits: InferencePageLimits,
}

#[test]
fn invalid_inference_config_fails_before_canonical_reconciliation() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("lib.rs"), "pub fn must_not_be_indexed() {}")?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let config = fixture.path().join("inference-read.json");
    std::fs::write(&config, br#"{"schema_version":99}"#)?;

    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"))
        .arg(&root)
        .arg(&database)
        .args(["127.0.0.1:0", "--inference-read-config"])
        .arg(&config)
        .output()?;
    if output.status.success() {
        return Err("daemon accepted an unsupported inference config schema".into());
    }

    let canonical_root = root.canonicalize()?;
    let (repository, worktree) = syntaxmesh_source_host::repository_scope(&canonical_root);
    let store = TursoGraphStore::open(&database)?;
    if store.current_generation(repository, worktree)?.is_some() {
        return Err("invalid inference config was checked after canonical reconciliation".into());
    }
    Ok(())
}

#[test]
fn daemon_inference_binding_reaches_http_and_mcp_without_downstream_writes()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    let state = fixture.path().join("state");
    std::fs::create_dir(&root)?;
    std::fs::create_dir(&state)?;
    std::fs::write(root.join("lib.rs"), "pub fn configured_reads() {}")?;
    let canonical_database = state.join("graph.db");
    let inference_database = state.join("inference.sqlite");
    TursoGraphStore::migrate(&canonical_database)?;
    InferenceMaterializer::migrate(&inference_database)?;
    let canonical_root = root.canonicalize()?;
    let prepared = prepare_inference(&canonical_root, &canonical_database, &inference_database)?;

    let origin_path = state.join("origin.json");
    std::fs::write(&origin_path, serde_json::to_vec(&prepared.origin)?)?;
    let config_path = state.join("inference-reads.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "limits": prepared.limits,
            "bindings": [{
                "id": "rules",
                "database": "inference.sqlite",
                "origin_file": "origin.json"
            }]
        }))?,
    )?;
    let downstream_before = std::fs::read(&inference_database)?;
    let config = config_path.to_str().ok_or("config path is not UTF-8")?;
    let (mut process, base) = start_with_extra_options(
        &canonical_root,
        &canonical_database,
        true,
        true,
        false,
        Some("30000"),
        &["--inference-read-config", config],
    )?;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()?;
    let generation = prepared.generation;
    let request = InferenceCheckpointRequest {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding: "rules".to_owned(),
        generation,
    };
    let http_checkpoint: InferenceCheckpointResponse = client
        .post(format!("{base}/api/v1/inference/checkpoint"))
        .json(&request)
        .send()?
        .error_for_status()?
        .json()?;
    let retained = http_checkpoint
        .checkpoint
        .ok_or("daemon HTTP host did not receive the configured binding")?;
    if http_checkpoint.binding != "rules" || retained.active_claims != 0 {
        return Err("daemon HTTP host returned a substituted retained checkpoint".into());
    }
    let page: InferencePageResponse = client
        .post(format!("{base}/api/v1/inference/claims-page"))
        .json(&InferencePageRequest {
            schema_version: INFERENCE_READ_SCHEMA_VERSION,
            binding: "rules".to_owned(),
            generation,
            limits: prepared.limits,
            cursor: None,
        })
        .send()?
        .error_for_status()?
        .json()?;
    if page.checkpoint != retained || !page.claims.is_empty() || page.next.is_some() {
        return Err("daemon HTTP host returned an invalid retained page".into());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let service = ()
            .serve(StreamableHttpClientTransport::from_uri(format!(
                "{base}/mcp"
            )))
            .await?;
        let arguments = serde_json::json!({
            "schema_version": INFERENCE_READ_SCHEMA_VERSION,
            "binding": "rules",
            "generation": generation.0.to_hex()
        });
        let arguments = arguments
            .as_object()
            .cloned()
            .ok_or("MCP arguments must be a JSON object")?;
        let result = service
            .peer()
            .call_tool(CallToolRequestParams::new("inference_checkpoint").with_arguments(arguments))
            .await?;
        let Some(ContentBlock::Text(text)) = result.content.into_iter().next() else {
            return Err::<(), Box<dyn Error>>("MCP checkpoint tool returned no text".into());
        };
        let mcp_checkpoint: InferenceCheckpointResponse = serde_json::from_str(&text.text)?;
        if mcp_checkpoint.binding != "rules"
            || mcp_checkpoint.checkpoint != Some(retained)
            || mcp_checkpoint.generation != generation
        {
            return Err("daemon MCP host did not receive the configured binding".into());
        }
        service.cancel().await?;
        Ok::<(), Box<dyn Error>>(())
    })?;

    #[cfg(unix)]
    {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(i32::try_from(process.0.id())?),
            nix::sys::signal::Signal::SIGTERM,
        )?;
        wait_exit(&mut process)?;
    }
    #[cfg(not(unix))]
    {
        process.0.kill()?;
        wait_status(&mut process, false)?;
    }
    if std::fs::read(&inference_database)? != downstream_before {
        return Err("daemon wrote to the configured downstream database".into());
    }
    Ok(())
}

fn prepare_inference(
    root: &Path,
    canonical_database: &Path,
    inference_database: &Path,
) -> Result<PreparedInference, Box<dyn Error>> {
    let (repository, worktree) = repository_scope(root);
    let generation = GenerationId::derive(&[b"daemon-inference-e2e-generation"]);
    let concept = NodeId::derive(&[b"daemon-inference-e2e-concept"]);
    let provenance_id = ProvenanceId::derive(&[b"daemon-inference-e2e-provenance"]);
    let mut store = TursoGraphStore::open(canonical_database)?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"daemon-inference-e2e-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![Provenance {
            id: provenance_id,
            producer_namespace: "test.daemon".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::UserAsserted,
            source: None,
        }],
        upsert_nodes: vec![Node {
            id: concept,
            kind: NodeKind::Function,
            name: "daemon inference concept".to_owned(),
            owner_file: None,
            source: None,
            provenance: provenance_id,
            extension_payload: None,
        }],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    drop(store);
    let project = ProjectConfig::load(root)?;
    let mut source = configured_source_engine(
        TursoGraphStore::open(canonical_database)?,
        root,
        &project,
        false,
    )?;
    let relation = |value: &str| RelationKind::External {
        namespace: "test".to_owned(),
        relation: value.to_owned(),
    };
    let rules = [Rule {
        id: "test:unused".to_owned(),
        version: "1".to_owned(),
        ontology_versions: std::collections::BTreeMap::new(),
        configuration_fingerprint: StableId::derive("test-config", &[b"daemon-inference-e2e"]),
        path_relation: RelationKind::Calls,
        join_relation: relation("joins"),
        declaration_relation: relation("declares"),
        concept,
        output_relation: relation("inferred"),
    }];
    let inference_limits = Limits {
        input_bytes: 16_384,
        facts: 1,
        edge_visits: 1,
        depth: 1,
        claims: 1,
    };
    let rebuild = RebuildRequest {
        generation,
        rules: &rules,
        limits: &inference_limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 1,
    };
    let origin = source
        .engine
        .initialize_inference_stream(inference_database, &rebuild, 4096)?;
    Ok(PreparedInference {
        origin,
        generation,
        limits: InferencePageLimits {
            claims: 1,
            payload_bytes: 16_384,
            premises: 1,
            tree_pages: 4096,
        },
    })
}
