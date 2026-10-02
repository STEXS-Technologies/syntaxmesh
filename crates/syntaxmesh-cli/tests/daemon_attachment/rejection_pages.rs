use super::transport::{Guard, exercise_command};
use axum::http::StatusCode;
use serde_json::json;
use std::error::Error;

pub(super) fn prepare_rejections(
    database: &std::path::Path,
    root: &std::path::Path,
) -> Result<String, Box<dyn Error>> {
    use syntaxmesh_core::{
        ChangeSetDelta, GenerationId, GraphDelta, GraphDeltaWithLineage, IndexRunId,
    };
    use syntaxmesh_source_host::{ProjectConfig, configured_source_engine};
    use syntaxmesh_store_turso::TursoGraphStore;
    let mut engine = configured_source_engine(
        TursoGraphStore::open(database)?,
        root,
        &ProjectConfig::load(root)?,
        false,
    )?
    .engine;
    let manifest = engine
        .status()?
        .ok_or("rejection fixture generation missing")?
        .manifest;
    let node = engine
        .query(manifest.generation)
        .search("attached_before", 1)?
        .into_iter()
        .next()
        .ok_or("rejection fixture node missing")?;
    for label in [
        b"cli-rejection-one".as_slice(),
        b"cli-rejection-two".as_slice(),
    ] {
        if engine
            .publish_prepared_with_lineage(GraphDeltaWithLineage {
                graph: GraphDelta {
                    repository: manifest.repository,
                    worktree: manifest.worktree,
                    run_id: IndexRunId::derive(&[label]),
                    expected_base: None,
                    next_generation: GenerationId::derive(&[label]),
                    changed_files: vec![],
                    removed_files: vec![],
                    upsert_provenance: vec![],
                    upsert_nodes: vec![node.clone()],
                    upsert_edges: vec![],
                    remove_nodes: vec![],
                    remove_edges: vec![],
                },
                lineage: ChangeSetDelta::default(),
            })
            .is_ok()
        {
            return Err("stale CLI fixture unexpectedly published".into());
        }
    }
    let page = engine.workflow_rejections(None, 1)?;
    Ok(page
        .next_cursor
        .ok_or("rejection fixture continuation missing")?
        .0
        .to_hex())
}

#[test]
fn rejection_attachment_rejects_counts_order_reasons_and_cursor_without_output()
-> Result<(), Box<dyn Error>> {
    let first = json!({"run_id":"b".repeat(64),"reason":{"kind":"unknown_legacy"}});
    let second = json!({"run_id":"c".repeat(64),"reason":{"kind":"unknown_legacy"}});
    for (items, cursor, error) in [
        (
            vec![first.clone(); 101],
            None,
            "inconsistent workflow rejection page",
        ),
        (
            vec![first.clone(), first.clone()],
            None,
            "inconsistent workflow rejection page",
        ),
        (
            vec![second, first.clone()],
            None,
            "inconsistent workflow rejection page",
        ),
        (
            vec![],
            Some("b".repeat(64)),
            "inconsistent workflow rejection page",
        ),
        (
            vec![first],
            Some("c".repeat(64)),
            "inconsistent workflow rejection page",
        ),
        (
            vec![json!({"run_id":"b".repeat(64),"reason":{"kind":"invented"}})],
            None,
            "encoding error",
        ),
        (
            vec![
                json!({"run_id":"b".repeat(64),"reason":{"kind":"unknown_legacy","unexpected":true}}),
            ],
            None,
            "encoding error",
        ),
    ] {
        let body = json!({"schema_version":1,"generation":"a".repeat(64),"data":{"items":items,"next_cursor":cursor}}).to_string();
        exercise_command(
            StatusCode::OK,
            Guard::Matching,
            body,
            Some(error),
            "workflow-rejections-turso",
            &["100"],
        )?;
    }
    Ok(())
}
