use super::transport::{Guard, exercise_command, exercise_guarded_pages, exercise_pages};
use axum::http::StatusCode;
use serde_json::json;
use std::error::Error;
use syntaxmesh_core::{
    FileId, FileVersion, GenerationId, GenerationManifest, GenerationStatus, RepositoryId,
    StableId, WorktreeId,
};

#[test]
fn status_rejects_generation_and_incomplete_duplicate_or_unsolicited_inventory()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().to_str().ok_or("fixture path is not UTF-8")?;
    let root_arguments = [root];
    let file = FileVersion {
        file_id: FileId::derive(&[b"file"]),
        normalized_path: "file.rs".to_owned(),
        content_hash: [0; 32],
        size_bytes: 0,
    };
    let manifest = GenerationManifest {
        repository: RepositoryId::derive(&[b"status-fixture"]),
        worktree: WorktreeId::derive(&[b"status-fixture"]),
        generation: GenerationId(StableId([0xaa; 32])),
        parent: None,
        graph_root: [0; 32],
        configuration_hash: [0; 32],
        extractor_set_hash: [0; 32],
        schema_version: 1,
        status: GenerationStatus::Durable,
    };
    for (generation, count, inventory, source) in [
        ("b".repeat(64), 0, None, false),
        ("a".repeat(64), 0, Some(vec![]), false),
        ("a".repeat(64), 1, None, true),
        ("a".repeat(64), 1, Some(vec![]), true),
        (
            "a".repeat(64),
            2,
            Some(vec![file.clone(), file.clone()]),
            true,
        ),
    ] {
        let body = json!({"schema_version":1,"generation":generation,"data":{
            "manifest":manifest,"files":count,"nodes":0,"edges":0,"provenance":0,
            "graph_root_matches":true,"references_valid":true,
            "workflow_prepared":0,"workflow_completed":0,"workflow_rejected":0,
            "indexed_files":inventory,
        }})
        .to_string();
        if source && inventory.is_none() {
            let empty = json!({"schema_version":1,"generation":"a".repeat(64),"data":{
                "repository":manifest.repository.0.to_hex(),"worktree":manifest.worktree.0.to_hex(),
                "items":[],"has_more":false,"next_after":null,
            }})
            .to_string();
            exercise_pages(
                StatusCode::OK,
                Guard::Matching,
                vec![body, empty],
                Some("inconsistent file inventory page"),
                "status-turso",
                &root_arguments,
            )?;
            continue;
        }
        exercise_command(
            StatusCode::OK,
            Guard::Matching,
            body,
            Some("inconsistent status report"),
            "status-turso",
            if source { &root_arguments } else { &[] },
        )?;
    }
    let report = json!({"schema_version":1,"generation":"a".repeat(64),"data":{
        "manifest":manifest,"files":2,"nodes":0,"edges":0,"provenance":0,
        "graph_root_matches":true,"references_valid":true,
        "workflow_prepared":0,"workflow_completed":0,"workflow_rejected":0,"indexed_files":null,
    }})
    .to_string();
    for (generation, repository, items, next) in [
        (
            "a".repeat(64),
            manifest.repository.0.to_hex(),
            vec![file.clone(), file.clone()],
            None,
        ),
        ("a".repeat(64), manifest.repository.0.to_hex(), vec![], None),
        (
            "a".repeat(64),
            manifest.repository.0.to_hex(),
            vec![file.clone()],
            Some(file.file_id.0.to_hex()),
        ),
        ("a".repeat(64), "b".repeat(64), vec![], None),
        ("b".repeat(64), manifest.repository.0.to_hex(), vec![], None),
    ] {
        let page = json!({"schema_version":1,"generation":generation,"data":{
            "repository":repository,"worktree":manifest.worktree.0.to_hex(),"items":items,
            "has_more":next.is_some(),"next_after":next,
        }})
        .to_string();
        exercise_pages(
            StatusCode::OK,
            Guard::Matching,
            vec![report.clone(), page],
            Some("inconsistent file inventory page"),
            "status-turso",
            &root_arguments,
        )?;
    }
    let mut guarded_report: serde_json::Value = serde_json::from_str(&report)?;
    *guarded_report
        .get_mut("data")
        .and_then(|data| data.get_mut("files"))
        .ok_or("fixture report count missing")? = json!(101);
    let files = (1_u8..=100)
        .map(|index| FileVersion {
            file_id: FileId(StableId([index; 32])),
            normalized_path: format!("file-{index}.rs"),
            content_hash: [0; 32],
            size_bytes: 0,
        })
        .collect::<Vec<_>>();
    let page = json!({"schema_version":1,"generation":"a".repeat(64),"data":{
        "repository":manifest.repository.0.to_hex(),"worktree":manifest.worktree.0.to_hex(),
        "items":files,"has_more":true,"next_after":StableId([100;32]).to_hex(),
    }})
    .to_string();
    for guard in [Guard::Missing, Guard::Wrong, Guard::Duplicate] {
        exercise_guarded_pages(
            StatusCode::OK,
            vec![
                (Guard::Matching, guarded_report.to_string()),
                (Guard::Matching, page.clone()),
                (guard, "{}".to_owned()),
            ],
            Some("owner instance response mismatch"),
            "status-turso",
            &root_arguments,
        )?;
    }
    Ok(())
}
