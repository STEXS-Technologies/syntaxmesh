//! Retained identifier evidence survives a producer-only definition-range upgrade.

use std::error::Error;
use syntaxmesh_core::{
    FileId, FileVersion, GenerationId, IndexRunId, NodeKind, RepositoryId, SourceSpan, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, SourceFile,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore};

struct IdentifierEvidenceFixture;

struct LegacyRoleFixture;

pub(super) fn exercise_syntax_failure<S: GraphStore + DurableRecordStore>(
    open: impl Fn() -> Result<S, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"syntax-failure-history"]);
    let worktree = WorktreeId::derive(&[b"syntax-failure-history"]);
    let source = |path: &str, content: &str| -> Result<SourceFile, Box<dyn Error>> {
        Ok(SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[path.as_bytes()]),
                normalized_path: path.to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len())?,
            },
            content: content.to_owned(),
        })
    };
    let first = GenerationId::derive(&[b"syntax-first"]);
    let rejected = GenerationId::derive(&[b"syntax-rejected"]);
    let repaired = GenerationId::derive(&[b"syntax-repaired"]);
    let mut engine = SyntaxMeshEngine::new(open()?, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    engine.index(
        &[source("valid.rs", "fn original() {}")?],
        IndexRunId::derive(&[b"syntax-first"]),
        first,
    )?;
    let before = engine.into_store().historical_snapshot(first)?;
    let mut attempt = SyntaxMeshEngine::new(open()?, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    let result = attempt.index(
        &[
            source("valid.rs", "fn changed() {}")?,
            source("template.rs", "fn fixture_function_{NAME}() {}")?,
        ],
        IndexRunId::derive(&[b"syntax-rejected"]),
        rejected,
    );
    match result {
        Err(syntaxmesh_engine::EngineError::Index(error))
            if format!("{error:?}").starts_with("Extractor(SyntaxError(") => {}
        _ => return Err("mixed update did not reject source syntax before publication".into()),
    }
    drop(attempt);
    let store = open()?;
    if store
        .current_generation(repository, worktree)?
        .map(|manifest| manifest.generation)
        != Some(first)
        || store.historical_snapshot(first)? != before
    {
        return Err("failed extraction changed the retained generation after restart".into());
    }
    let mut recovery = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    if recovery.verify_statechronicle_history()?.is_none() {
        return Err("failed extraction damaged verified history".into());
    }
    recovery.index(
        &[
            source("valid.rs", "fn changed() {}")?,
            source("template.rs", "fn fixture_function_repaired() {}")?,
        ],
        IndexRunId::derive(&[b"syntax-repaired"]),
        repaired,
    )?;
    drop(recovery);
    let repaired_store = open()?;
    let after = repaired_store.historical_snapshot(repaired)?;
    if repaired_store.historical_snapshot(first)? != before
        || !after.nodes.iter().any(|node| node.name == "changed")
        || !after
            .nodes
            .iter()
            .any(|node| node.name == "fixture_function_repaired")
    {
        return Err("repair did not publish both valid inputs with retained history".into());
    }
    let mut verified = SyntaxMeshEngine::new(repaired_store, RustExtractor, repository, worktree);
    if verified.verify_statechronicle_history()?.is_none() {
        return Err("repair lost verified history after restart".into());
    }
    Ok(())
}

impl LanguageExtractor for LegacyRoleFixture {
    fn language(&self) -> &'static str {
        "rust"
    }
    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new("syntaxmesh.lang.rust", "0.16.0")
    }
    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        let mut extraction = RustExtractor.extract(source)?;
        extraction.provenance.producer_version = "0.16.0".to_owned();
        for node in &mut extraction.nodes {
            if let Some(payload) = &mut node.extension_payload
                && payload.namespace == syntaxmesh_language_sdk::SOURCE_ROLE_NAMESPACE
            {
                payload.namespace = "syntaxmesh.lang.rust.test-role".to_owned();
            }
        }
        Ok(extraction)
    }
}

pub(super) fn exercise_legacy_role<S: GraphStore + DurableRecordStore>(
    open: impl Fn() -> Result<S, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"legacy-role-upgrade"]);
    let worktree = WorktreeId::derive(&[b"legacy-role-upgrade"]);
    let content = "#[test] fn check() {}";
    let source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"src/roles.rs"]),
            normalized_path: "src/roles.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content: content.to_owned(),
    };
    let first = GenerationId::derive(&[b"legacy-role-first"]);
    let second = GenerationId::derive(&[b"canonical-role-second"]);
    let mut legacy = SyntaxMeshEngine::new(open()?, LegacyRoleFixture, repository, worktree)
        .with_statechronicle_verification();
    legacy.index(
        std::slice::from_ref(&source),
        IndexRunId::derive(&[b"legacy-role-first"]),
        first,
    )?;
    let before = legacy.into_store().historical_snapshot(first)?;
    let mut upgraded = SyntaxMeshEngine::new(open()?, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    upgraded.index(
        &[source],
        IndexRunId::derive(&[b"canonical-role-second"]),
        second,
    )?;
    drop(upgraded);
    let store = open()?;
    let after = store.historical_snapshot(second)?;
    let old = before
        .nodes
        .iter()
        .find(|node| node.name == "check")
        .ok_or("missing legacy role")?;
    let new = after
        .nodes
        .iter()
        .find(|node| node.name == "check")
        .ok_or("missing canonical role")?;
    if store.historical_snapshot(first)? != before
        || old.id != new.id
        || old
            .extension_payload
            .as_ref()
            .is_none_or(|payload| payload.namespace != "syntaxmesh.lang.rust.test-role")
        || new.extension_payload.as_ref().is_none_or(|payload| {
            payload.namespace != syntaxmesh_language_sdk::SOURCE_ROLE_NAMESPACE
        })
        || syntaxmesh_language_sdk::SourceRole::from_payload(old.extension_payload.as_ref())?
            != syntaxmesh_language_sdk::SourceRole::TestIntent
        || syntaxmesh_language_sdk::SourceRole::from_payload(new.extension_payload.as_ref())?
            != syntaxmesh_language_sdk::SourceRole::TestIntent
    {
        return Err(std::io::Error::other(
            "role codec upgrade changed identity, decoding or retained history",
        )
        .into());
    }
    let mut final_engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    if final_engine.verify_statechronicle_history()?.is_none() {
        return Err(std::io::Error::other("role codec upgrade lost verification").into());
    }
    Ok(())
}

impl LanguageExtractor for IdentifierEvidenceFixture {
    fn language(&self) -> &'static str {
        "rust"
    }
    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new("syntaxmesh.lang.rust", "0.14.0")
    }
    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        let mut extraction = RustExtractor.extract(source)?;
        extraction.provenance.producer_version = "0.14.0".to_owned();
        for node in &mut extraction.nodes {
            if node.kind != NodeKind::Function {
                continue;
            }
            node.extension_payload = None;
            let marker = format!("fn {}", node.name);
            let start = source
                .content
                .find(&marker)
                .ok_or_else(|| {
                    ExtractorError::InvalidInput("missing fixture identifier".to_owned())
                })?
                .saturating_add(3);
            let start = u64::try_from(start)
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
            let length = u64::try_from(node.name.len())
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
            if let Some(location) = &mut node.source {
                location.span = SourceSpan::new(start, start.saturating_add(length))
                    .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
            }
        }
        Ok(extraction)
    }
}

pub(super) fn exercise<S: GraphStore + DurableRecordStore>(
    open: impl Fn() -> Result<S, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"rust-definition-upgrade"]);
    let worktree = WorktreeId::derive(&[b"rust-definition-upgrade"]);
    let content = "#[tokio::test]\nfn entry() {\n  helper();\n}\nfn helper() {}\n";
    let source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"src/upgrade.rs"]),
            normalized_path: "src/upgrade.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content: content.to_owned(),
    };
    let first = GenerationId::derive(&[b"rust-definition-before"]);
    let second = GenerationId::derive(&[b"rust-definition-after"]);
    let mut legacy =
        SyntaxMeshEngine::new(open()?, IdentifierEvidenceFixture, repository, worktree)
            .with_statechronicle_verification();
    legacy.index(
        std::slice::from_ref(&source),
        IndexRunId::derive(&[b"rust-definition-before"]),
        first,
    )?;
    let before = legacy.into_store().historical_snapshot(first)?;
    let mut upgraded = SyntaxMeshEngine::new(open()?, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    upgraded.index(
        &[source],
        IndexRunId::derive(&[b"rust-definition-after"]),
        second,
    )?;
    if upgraded.verify_statechronicle_history()?.is_none() {
        return Err(std::io::Error::other("upgrade omitted verified generation history").into());
    }
    drop(upgraded);
    let reopened = open()?;
    let after = reopened.historical_snapshot(second)?;
    if reopened.historical_snapshot(first)? != before || before.edges != after.edges {
        return Err(
            std::io::Error::other("upgrade changed retained history or call ownership").into(),
        );
    }
    let old = before
        .nodes
        .iter()
        .find(|node| node.name == "entry")
        .ok_or("missing old entry")?;
    let new = after
        .nodes
        .iter()
        .find(|node| node.name == "entry")
        .ok_or("missing new entry")?;
    let old_span = &old.source.as_ref().ok_or("missing old range")?.span;
    let new_span = &new.source.as_ref().ok_or("missing new range")?.span;
    if old.id != new.id
        || old.extension_payload.is_some()
        || new.extension_payload.as_ref().is_none_or(|payload| {
            payload.namespace != syntaxmesh_language_sdk::SOURCE_ROLE_NAMESPACE
                || payload.schema_version != 1
                || payload.bytes != [1]
        })
        || old_span == new_span
        || content.get(usize::try_from(old_span.start_byte)?..usize::try_from(old_span.end_byte)?)
            != Some("entry")
        || content.get(usize::try_from(new_span.start_byte)?..usize::try_from(new_span.end_byte)?)
            != Some("#[tokio::test]\nfn entry() {\n  helper();\n}")
        || !after
            .provenance
            .iter()
            .any(|item| item.producer_version == "0.18.0")
    {
        return Err(std::io::Error::other(
            "unchanged source was not upgraded with stable identity",
        )
        .into());
    }
    let mut restored = SyntaxMeshEngine::new(reopened, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    if restored.verify_statechronicle_history()?.is_none() {
        return Err(
            std::io::Error::other("verified upgrade history did not survive restart").into(),
        );
    }
    let removed_content = "fn entry() {\n  helper();\n}\nfn helper() {}\n";
    let removed_source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"src/upgrade.rs"]),
            normalized_path: "src/upgrade.rs".to_owned(),
            content_hash: *blake3::hash(removed_content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(removed_content.len())?,
        },
        content: removed_content.to_owned(),
    };
    let third = GenerationId::derive(&[b"rust-role-removed"]);
    restored.index(
        &[removed_source],
        IndexRunId::derive(&[b"rust-role-removed"]),
        third,
    )?;
    drop(restored);
    let final_store = open()?;
    let removed = final_store.historical_snapshot(third)?;
    let removed_entry = removed
        .nodes
        .iter()
        .find(|node| node.name == "entry")
        .ok_or("missing removed-role entry")?;
    if removed_entry.id != new.id
        || removed_entry.extension_payload.is_some()
        || final_store.historical_snapshot(first)? != before
        || final_store.historical_snapshot(second)? != after
    {
        return Err(std::io::Error::other(
            "test-role removal changed identity or retained evidence",
        )
        .into());
    }
    let mut final_engine = SyntaxMeshEngine::new(final_store, RustExtractor, repository, worktree);
    if final_engine.verify_statechronicle_history()?.is_none() {
        return Err(std::io::Error::other("test-role removal lost verified history").into());
    }
    Ok(())
}
