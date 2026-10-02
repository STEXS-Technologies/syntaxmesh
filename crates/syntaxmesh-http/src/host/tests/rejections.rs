use std::error::Error;

use reqwest::blocking::Client;
use serde_json::{Value, json};
use syntaxmesh_core::{
    ChangeSetDelta, GenerationId, GraphDelta, GraphDeltaWithLineage, IndexRunId, NodeId,
};
use syntaxmesh_engine::{SyntaxMeshEngine, WorkflowRejectionReason};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store_turso::TursoGraphStore;

pub(super) fn verify(
    client: &Client,
    base: &str,
    engine: &mut SyntaxMeshEngine<TursoGraphStore, RustExtractor>,
    generation: GenerationId,
    node: NodeId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let manifest = engine.query(generation).manifest()?;
    let retained_node = engine
        .query(generation)
        .node(node)?
        .ok_or("rejection fixture node missing")?;
    for label in [
        b"http-rejection-one".as_slice(),
        b"http-rejection-two".as_slice(),
    ] {
        let result = engine.publish_prepared_with_lineage(GraphDeltaWithLineage {
            graph: GraphDelta {
                repository: manifest.repository,
                worktree: manifest.worktree,
                run_id: IndexRunId::derive(&[label]),
                expected_base: None,
                next_generation: GenerationId::derive(&[label]),
                changed_files: vec![],
                removed_files: vec![],
                upsert_provenance: vec![],
                upsert_nodes: vec![retained_node.clone()],
                upsert_edges: vec![],
                remove_nodes: vec![],
                remove_edges: vec![],
            },
            lineage: ChangeSetDelta::default(),
        });
        if result.is_ok() {
            return Err("stale fixture unexpectedly published".into());
        }
    }
    let counts = engine.workflow_diagnostics()?;
    let before = engine.workflow_rejections(None, 100)?;
    if before.items.len() != 2 {
        return Err("durable rejection fixture missing".into());
    }
    let url = format!("{base}/api/v1/workflow-rejections");
    let mut after = None;
    for expected in &before.items {
        let mut request = client.get(&url).query(&[("limit", "1")]);
        if let Some(cursor) = after {
            request = request.query(&[("after_run", cursor)]);
        }
        let response = super::require_status(request.send()?, 200)?;
        let reason = match expected.reason {
            WorkflowRejectionReason::StaleBase {
                repository,
                worktree,
                expected,
                actual,
            } => json!({
                "kind":"stale_base","repository":repository.0.to_hex(),"worktree":worktree.0.to_hex(),
                "expected":expected.map(|id| id.0.to_hex()),"actual":actual.map(|id| id.0.to_hex()),
            }),
            WorkflowRejectionReason::UnknownLegacy => {
                return Err("typed fixture unexpectedly legacy".into());
            }
        };
        let data = response.get("data").ok_or("rejection data missing")?;
        if data.get("items")
            != Some(&json!([{"run_id":expected.run_id.0.to_hex(),"reason":reason}]))
            || response.get("generation").and_then(Value::as_str)
                != Some(generation.0.to_hex().as_str())
        {
            return Err("HTTP rejection differs from existing Penelope reader".into());
        }
        after = data
            .get("next_cursor")
            .and_then(Value::as_str)
            .map(str::to_owned);
    }
    if after.is_some()
        || engine.workflow_diagnostics()? != counts
        || engine.workflow_rejections(None, 100)? != before
    {
        return Err("rejection reads changed journal or continuation".into());
    }
    Ok(())
}
