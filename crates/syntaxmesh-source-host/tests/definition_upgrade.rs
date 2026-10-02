//! Reuse the Engine's verified File-store producer-upgrade fixture pattern.

use std::error::Error;
use syntaxmesh_core::{
    FileId, FileVersion, GenerationId, IndexRunId, NodeKind, RepositoryId, SourceSpan, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_ecmascript::{JavaScriptExtractor, TypeScriptExtractor};
use syntaxmesh_lang_python::PythonExtractor;
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, SourceFile,
};
use syntaxmesh_store::{FileGraphStore, GraphStore};

struct IdentifierFixture<E> {
    extractor: E,
    identity: ExtractorIdentity,
    keyword: &'static str,
}

impl<E: LanguageExtractor> LanguageExtractor for IdentifierFixture<E> {
    fn language(&self) -> &'static str {
        self.extractor.language()
    }
    fn producer_identity(&self) -> ExtractorIdentity {
        self.identity.clone()
    }
    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        let mut extraction = self.extractor.extract(source)?;
        extraction
            .provenance
            .producer_version
            .clone_from(&self.identity.version);
        for node in &mut extraction.nodes {
            if node.kind != NodeKind::Function {
                continue;
            }
            let marker = format!("{} {}", self.keyword, node.name);
            let start = source
                .content
                .find(&marker)
                .ok_or_else(|| {
                    ExtractorError::InvalidInput("missing legacy fixture declaration".to_owned())
                })?
                .saturating_add(self.keyword.len())
                .saturating_add(1);
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

fn verify_upgrade<E: LanguageExtractor + Default>(
    path: &str,
    content: &str,
    expected: &str,
    keyword: &'static str,
    old_revision: &str,
    new_revision: &str,
) -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let database = temporary.path().join("graph.snapshot");
    let extractor = E::default();
    let identity = extractor.producer_identity();
    let mut previous_identity = identity.clone();
    previous_identity.version = identity.version.replace(new_revision, old_revision);
    if previous_identity == identity {
        return Err(std::io::Error::other("fixture did not change producer revision").into());
    }
    let repository = RepositoryId::derive(&[path.as_bytes()]);
    let worktree = WorktreeId::derive(&[path.as_bytes()]);
    let first = GenerationId::derive(&[path.as_bytes(), b"before"]);
    let second = GenerationId::derive(&[path.as_bytes(), b"after"]);
    let source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content: content.to_owned(),
    };
    let mut legacy = SyntaxMeshEngine::new(
        FileGraphStore::open(&database)?,
        IdentifierFixture {
            extractor: E::default(),
            identity: previous_identity,
            keyword,
        },
        repository,
        worktree,
    )
    .with_statechronicle_verification();
    legacy.index(
        std::slice::from_ref(&source),
        IndexRunId::derive(&[path.as_bytes(), b"before"]),
        first,
    )?;
    let before = legacy.into_store().historical_snapshot(first)?;
    let mut upgraded = SyntaxMeshEngine::new(
        FileGraphStore::open(&database)?,
        extractor,
        repository,
        worktree,
    )
    .with_statechronicle_verification();
    upgraded.index(
        &[source],
        IndexRunId::derive(&[path.as_bytes(), b"after"]),
        second,
    )?;
    drop(upgraded);
    let reopened = FileGraphStore::open(&database)?;
    let after = reopened.historical_snapshot(second)?;
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
    let old_span = &old.source.as_ref().ok_or("missing old evidence")?.span;
    let new_span = &new.source.as_ref().ok_or("missing new evidence")?.span;
    if before != reopened.historical_snapshot(first)?
        || before.edges != after.edges
        || old.id != new.id
        || content.get(usize::try_from(old_span.start_byte)?..usize::try_from(old_span.end_byte)?)
            != Some("entry")
        || content.get(usize::try_from(new_span.start_byte)?..usize::try_from(new_span.end_byte)?)
            != Some(expected)
        || !after
            .provenance
            .iter()
            .any(|item| item.producer_version == identity.version)
    {
        return Err(std::io::Error::other(
            "upgrade changed retained history, ownership or identity",
        )
        .into());
    }
    let mut restored = SyntaxMeshEngine::new(reopened, E::default(), repository, worktree);
    if restored.verify_statechronicle_history()?.is_none() {
        return Err(std::io::Error::other("verified upgrade chain was not retained").into());
    }
    Ok(())
}

#[test]
fn python_definition_upgrade_retains_verified_identifier_history() -> Result<(), Box<dyn Error>> {
    verify_upgrade::<PythonExtractor>(
        "src/upgrade.py",
        "def entry():\n    helper()\n\ndef helper():\n    pass\n",
        "def entry():\n    helper()",
        "def",
        ";extractor=3",
        ";extractor=4",
    )
}

#[test]
fn typescript_definition_upgrade_retains_verified_identifier_history() -> Result<(), Box<dyn Error>>
{
    verify_upgrade::<TypeScriptExtractor>(
        "src/upgrade.ts",
        "function entry() {\n  helper();\n}\nfunction helper() {}\n",
        "function entry() {\n  helper();\n}",
        "function",
        ";extractor=10",
        ";extractor=11",
    )
}

#[test]
fn javascript_definition_upgrade_retains_verified_identifier_history() -> Result<(), Box<dyn Error>>
{
    verify_upgrade::<JavaScriptExtractor>(
        "src/upgrade.js",
        "function entry() {\n  helper();\n}\nfunction helper() {}\n",
        "function entry() {\n  helper();\n}",
        "function",
        ";extractor=10",
        ";extractor=11",
    )
}
