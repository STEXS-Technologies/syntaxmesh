use super::*;
use crate::SyntaxMeshEngine;
use syntaxmesh_core::{FileId, FileVersion, IndexRunId};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_store::InMemoryGraphStore;

#[test]
fn source_preparation_can_be_discarded_before_verified_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let repository = RepositoryId::derive(&[b"prepare-repository"]);
    let worktree = WorktreeId::derive(&[b"prepare-worktree"]);
    let generation = GenerationId::derive(&[b"prepared-generation"]);
    let run = IndexRunId::derive(&[b"prepared-run"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    )
    .with_statechronicle_verification();
    let content = "pub fn prepared() {}";
    let sources = [SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"prepared-file"]),
            normalized_path: "lib.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content: content.to_owned(),
    }];
    let discarded = engine.prepare_source_delta(&sources, run, generation)?;
    if discarded.upsert_nodes.is_empty()
        || engine.status()?.is_some()
        || engine.query(generation).manifest().is_ok()
    {
        return Err("preparation accepted the candidate generation".into());
    }
    drop(discarded);
    let graph = engine.prepare_source_delta(&sources, run, generation)?;
    let receipt = engine.publish_prepared_with_lineage(syntaxmesh_core::GraphDeltaWithLineage {
        graph,
        lineage: syntaxmesh_core::ChangeSetDelta::default(),
    })?;
    if receipt.publication.generation.generation != generation
        || receipt.verification != syntaxmesh_workflow::WorkflowStatus::Verified
        || engine.verify_statechronicle_history()?.is_none()
    {
        return Err("prepared publication bypassed normal verification".into());
    }
    Ok(())
}

#[test]
fn inventory_packing_preserves_utf8_byte_lengths_and_content_hashes()
-> Result<(), Box<dyn std::error::Error>> {
    let files = [SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"unicode-file"]),
            normalized_path: "é.rs".to_owned(),
            content_hash: [7; 32],
            size_bytes: 0,
        },
        content: String::new(),
    }];
    let mut expected = 5_u64.to_le_bytes().to_vec();
    expected.extend_from_slice("é.rs".as_bytes());
    expected.extend_from_slice(&[7; 32]);
    if source_inventory_fingerprint(&files)? != expected
        || !source_inventory_fingerprint(&[])?.is_empty()
    {
        return Err("source packing changed byte layout".into());
    }
    Ok(())
}

#[test]
fn shared_planning_skips_repeat_and_distinguishes_reverted_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    let repository = RepositoryId::derive(&[b"planning-repository"]);
    let worktree = WorktreeId::derive(&[b"planning-worktree"]);
    let fingerprint = RustExtractor.configuration_fingerprint();
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    engine.recover_pending_workflows()?;
    let mut generations = Vec::new();
    for content in [
        "pub fn before() {}",
        "pub fn after() {}",
        "pub fn before() {}",
    ] {
        let file = SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[b"planning-file"]),
                normalized_path: "lib.rs".to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len())?,
            },
            content: content.to_owned(),
        };
        let sources = [file];
        let (generation, needed) =
            engine.prepare_source_index(&sources, &fingerprint, content.as_bytes())?;
        if !needed || generations.contains(&generation) {
            return Err("edit/revert planning reused an earlier generation".into());
        }
        // Replanning uncommitted intent must still require publication.
        if engine.prepare_source_index(&sources, &fingerprint, content.as_bytes())?
            != (generation, true)
        {
            return Err("planning marker was mistaken for completed publication".into());
        }
        engine.index(
            &sources,
            IndexRunId::derive(&[generation.0.0.as_slice()]),
            generation,
        )?;
        if engine.prepare_source_index(&sources, &fingerprint, content.as_bytes())?
            != (generation, false)
        {
            return Err("unchanged reconciliation required another publication".into());
        }
        generations.push(generation);
    }
    for generation in generations {
        engine.query(generation).manifest()?;
    }
    Ok(())
}
