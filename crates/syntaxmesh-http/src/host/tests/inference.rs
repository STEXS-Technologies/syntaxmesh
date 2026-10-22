use super::*;
use syntaxmesh_api_model::{
    INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpointRequest, InferencePageLimits,
    InferencePageRequest,
};
use syntaxmesh_core::StableId;
use syntaxmesh_inference::ClaimSelection;
use syntaxmesh_inference_host::InferenceReadBindings;
use syntaxmesh_inference_sqlite::{InferenceMaterializer, RebuildRequest, StreamOrigin, Watermark};

#[test]
fn inference_http_rejects_untrusted_input_without_creating_storage()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("lib.rs"), "pub fn fixture() {}\n")?;
    let database = fixture.path().join("canonical.sqlite");
    TursoGraphStore::migrate(&database)?;
    let repository = RepositoryId::derive(&[b"http-inference"]);
    let worktree = WorktreeId::derive(&[b"http-inference"]);
    let generation = GenerationId::derive(&[b"http-inference"]);
    let mut engine = SyntaxMeshEngine::new(
        TursoGraphStore::open(&database)?,
        RustExtractor,
        repository,
        worktree,
    );
    engine.index(
        &scan(&source, &["rs"])?.files,
        IndexRunId::derive(&[b"http-inference"]),
        generation,
    )?;
    let original_manifest = engine.query(generation).manifest()?;
    let host = SyntaxMeshHttp::open(&database)?;
    let limits = InferencePageLimits {
        claims: 10,
        payload_bytes: 4096,
        premises: 100,
        tree_pages: 64,
    };
    let foreign = InferenceReadBindings::new(
        repository,
        WorktreeId::derive(&[b"foreign-http-inference"]),
        limits,
    )?;
    if host.clone().with_inference(foreign).is_ok() {
        return Err("HTTP host accepted a foreign inference registry".into());
    }
    let mut bindings = InferenceReadBindings::new(repository, worktree, limits)?;
    let downstream = fixture.path().join("missing-inference.sqlite");
    bindings.insert(
        "rules".to_owned(),
        downstream.clone(),
        StreamOrigin {
            stream: StableId::derive("test-stream", &[b"stream"]),
            baseline: Watermark {
                operation: StableId::derive("test-operation", &[b"operation"]),
                selection: ClaimSelection {
                    repository,
                    worktree,
                    generation,
                },
                generation_sequence: 0,
                configuration: StableId::derive("test-config", &[b"config"]),
                claim_count: 0,
            },
        },
    )?;
    let checkpoint = InferenceCheckpointRequest {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding: "rules".to_owned(),
        generation,
    };
    let guard = host.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        if host.inference_checkpoint(checkpoint.clone()).await
            != Err(axum::http::StatusCode::SERVICE_UNAVAILABLE)
        {
            return Err("unconfigured inference was not disabled".into());
        }
        let configured = host.with_inference(bindings)?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            configured
                .serve_until(listener, async {
                    drop(shutdown_rx.await);
                })
                .await
        });
        let result = async {
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let checkpoint_url = format!("http://{address}/api/v1/inference/checkpoint");
            let page_url = format!("http://{address}/api/v1/inference/claims-page");
            let mut untrusted = serde_json::to_value(&checkpoint)?;
            untrusted
                .as_object_mut()
                .ok_or("checkpoint must be an object")?
                .insert("database".to_owned(), serde_json::json!("/untrusted/path"));
            let page = InferencePageRequest {
                schema_version: INFERENCE_READ_SCHEMA_VERSION,
                binding: "rules".to_owned(),
                generation,
                limits: InferencePageLimits {
                    claims: 11,
                    ..limits
                },
                cursor: None,
            };
            for (url, body, status) in [
                (&checkpoint_url, untrusted, 400),
                (
                    &checkpoint_url,
                    serde_json::to_value(InferenceCheckpointRequest {
                        binding: "unknown".to_owned(),
                        ..checkpoint.clone()
                    })?,
                    400,
                ),
                (&page_url, serde_json::to_value(page)?, 400),
                (&checkpoint_url, serde_json::to_value(&checkpoint)?, 500),
            ] {
                let response = client.post(url).json(&body).send().await?;
                if response.status().as_u16() != status {
                    return Err(format!(
                        "inference boundary expected {status}, got {}",
                        response.status()
                    )
                    .into());
                }
            }
            let oversized = client
                .post(&checkpoint_url)
                .header("content-type", "application/json")
                .body(" ".repeat(16 * 1024 + 1))
                .send()
                .await?;
            if oversized.status().as_u16() != 413 {
                return Err("oversized inference body admitted".into());
            }
            Ok::<(), Box<dyn Error + Send + Sync>>(())
        }
        .await;
        shutdown_tx
            .send(())
            .map_err(|_value| "HTTP shutdown channel closed")?;
        server.await??;
        result
    })?;
    drop(runtime);
    drop(guard);
    if downstream.exists() {
        return Err("inference HTTP created missing storage".into());
    }
    if engine.query(generation).manifest()? != original_manifest
        || engine.current_generation(repository, worktree)? != Some(original_manifest)
    {
        return Err("rejected inference requests changed canonical selection".into());
    }
    Ok(())
}

#[test]
fn inference_http_serves_retained_checkpoint_and_claim_page()
-> Result<(), Box<dyn Error + Send + Sync>> {
    use std::collections::BTreeMap;
    use syntaxmesh_core::{
        Edge, EdgeId, EvidenceClass, GraphDelta, Node, NodeId, NodeKind, Provenance, ProvenanceId,
        RelationKind,
    };
    use syntaxmesh_engine::SyntaxMeshEngine;
    use syntaxmesh_inference::{Limits, Rule};
    use syntaxmesh_lang_rust::RustExtractor;
    use syntaxmesh_store::GraphStore;
    use syntaxmesh_store_turso::TursoGraphStore;

    let fixture_dir = tempfile::tempdir()?;
    let canonical_database = fixture_dir.path().join("canonical.sqlite");
    let inference_database = fixture_dir.path().join("inference.sqlite");
    TursoGraphStore::migrate(&canonical_database)?;
    InferenceMaterializer::migrate(&inference_database).map_err(|error| error.to_string())?;
    let mut store = TursoGraphStore::open(&canonical_database)?;
    let repository = RepositoryId::derive(&[b"http-inference-retained"]);
    let worktree = WorktreeId::derive(&[b"http-inference-retained"]);
    let first = GenerationId::derive(&[b"http-inference-first"]);
    let generation = GenerationId::derive(&[b"http-inference-declared"]);
    let [entry, writer, target, concept] =
        ["entry", "writer", "target", "concept"].map(|name| NodeId::derive(&[name.as_bytes()]));
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"http-inference-provenance"]),
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
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"http-inference-first-run"]),
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
        run_id: IndexRunId::derive(&[b"http-inference-declaration-run"]),
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
    let manifest = store.manifest(generation)?;
    let limits = Limits {
        input_bytes: 16_384,
        facts: 20,
        edge_visits: 100,
        depth: 2,
        claims: 10,
    };
    let first_rule = Rule {
        id: "example:write_path".to_owned(),
        version: "1".to_owned(),
        ontology_versions: BTreeMap::new(),
        configuration_fingerprint: StableId::derive("config", &[b"http-inference"]),
        path_relation: RelationKind::Calls,
        join_relation: external("writes"),
        declaration_relation: external("instance_of"),
        concept,
        output_relation: external("participates"),
    };
    let second_rule = Rule {
        id: "example:write_path_alt".to_owned(),
        configuration_fingerprint: StableId::derive("config", &[b"http-inference-alt"]),
        ..first_rule.clone()
    };
    let rules = [first_rule, second_rule];
    let rebuild = RebuildRequest {
        generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 2,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let mut engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    let origin = engine.initialize_inference_stream(&inference_database, &rebuild, 4096)?;
    drop(engine);

    let policy = InferencePageLimits {
        claims: 1,
        payload_bytes: 16_384,
        premises: 100,
        tree_pages: 4096,
    };
    let mut bindings = InferenceReadBindings::new(manifest.repository, manifest.worktree, policy)?;
    bindings.insert("rules".to_owned(), inference_database, origin)?;
    let host = SyntaxMeshHttp::open(&canonical_database)?.with_inference(bindings)?;
    let guard = host.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            host.serve_until(listener, async {
                drop(shutdown_rx.await);
            })
            .await
        });
        let result = async {
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let checkpoint: syntaxmesh_api_model::InferenceCheckpointResponse = client
                .post(format!("http://{address}/api/v1/inference/checkpoint"))
                .json(&InferenceCheckpointRequest {
                    schema_version: INFERENCE_READ_SCHEMA_VERSION,
                    binding: "rules".to_owned(),
                    generation,
                })
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let retained = checkpoint
                .checkpoint
                .ok_or("retained generation checkpoint is absent")?;
            if checkpoint.schema_version != INFERENCE_READ_SCHEMA_VERSION
                || checkpoint.binding != "rules"
                || checkpoint.generation != generation
                || retained.active_claims != 2
            {
                return Err("HTTP checkpoint did not preserve retained generation".into());
            }
            let first_page: syntaxmesh_api_model::InferencePageResponse = client
                .post(format!("http://{address}/api/v1/inference/claims-page"))
                .json(&InferencePageRequest {
                    schema_version: INFERENCE_READ_SCHEMA_VERSION,
                    binding: "rules".to_owned(),
                    generation,
                    limits: policy,
                    cursor: None,
                })
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            if first_page.schema_version != INFERENCE_READ_SCHEMA_VERSION
                || first_page.binding != "rules"
                || first_page.checkpoint != retained
                || first_page.claims.len() != 1
            {
                return Err("HTTP first claim page did not preserve its continuation".into());
            }
            let first_id = first_page
                .claims
                .first()
                .ok_or("HTTP first claim page is empty")?
                .id;
            let cursor = first_page
                .next
                .ok_or("HTTP first page omitted its cursor")?;
            let second_page: syntaxmesh_api_model::InferencePageResponse = client
                .post(format!("http://{address}/api/v1/inference/claims-page"))
                .json(&InferencePageRequest {
                    schema_version: INFERENCE_READ_SCHEMA_VERSION,
                    binding: "rules".to_owned(),
                    generation,
                    limits: policy,
                    cursor: Some(cursor),
                })
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let second_id = second_page
                .claims
                .first()
                .ok_or("HTTP second claim page is empty")?
                .id;
            if second_page.checkpoint != retained
                || second_page.claims.len() != 1
                || second_page.next.is_some()
                || second_page.binding != "rules"
                || second_page.schema_version != INFERENCE_READ_SCHEMA_VERSION
                || second_id == first_id
            {
                return Err("HTTP continuation did not return the next retained claim".into());
            }
            Ok::<(), Box<dyn Error + Send + Sync>>(())
        }
        .await;
        shutdown_tx
            .send(())
            .map_err(|_value| "HTTP shutdown channel closed")?;
        server.await??;
        result
    })?;
    drop(runtime);
    drop(guard);
    Ok(())
}
