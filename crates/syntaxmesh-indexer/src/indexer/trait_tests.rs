use super::*;
use syntaxmesh_core::{EvidenceClass, FileVersion, Provenance};
use syntaxmesh_language_sdk::{Extraction, ExtractorIdentity};
use syntaxmesh_store::InMemoryGraphStore;

/// Synthetic SDK producer: tests typed input, not Rust impl extraction.
pub(super) struct TraitFixture;

impl LanguageExtractor for TraitFixture {
    fn language(&self) -> &'static str {
        "trait-fixture"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new("syntaxmesh.test.trait-fixture", "1")
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        let location = SourceLocation {
            file_id: source.file.file_id,
            content_hash: source.file.content_hash,
            span: SourceSpan {
                start_byte: 0,
                end_byte: source.file.size_bytes,
            },
        };
        let provenance = Provenance {
            id: ProvenanceId::derive(&[&source.file.file_id.0.0, &source.file.content_hash]),
            producer_namespace: self.producer_identity().namespace,
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: Some(location.clone()),
        };
        let implementation = source.content == "implementation";
        let kind = if implementation {
            NodeKind::Struct
        } else if source.content == "trait" {
            NodeKind::Trait
        } else {
            NodeKind::Function
        };
        let node = Node {
            id: NodeId::derive(&[&source.file.file_id.0.0, source.content.as_bytes()]),
            kind,
            name: if implementation {
                "Service"
            } else {
                "Contract"
            }
            .to_owned(),
            owner_file: Some(source.file.file_id),
            source: Some(location.clone()),
            provenance: provenance.id,
            extension_payload: None,
        };
        let references = if implementation {
            vec![Reference {
                resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
                id: NodeId::derive(&[b"trait-fixture-reference"]),
                source: node.id,
                target: "Contract".to_owned(),
                relation: RelationKind::Implements,
                source_location: location,
                provenance: provenance.id,
            }]
        } else {
            Vec::new()
        };
        Ok(Extraction {
            provenance,
            nodes: vec![node],
            edges: Vec::new(),
            references,
            imports: Vec::new(),
            exports: Vec::new(),
        })
    }
}

pub(super) fn source(path: &str, content: &str) -> SourceFile {
    SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: content.len() as u64,
        },
        content: content.to_owned(),
    }
}

#[test]
fn trait_edits_revisit_unchanged_implementation_occurrences_and_retain_history()
-> Result<(), IndexError> {
    let mut indexer = Indexer::new(
        InMemoryGraphStore::new(),
        TraitFixture,
        RepositoryId::derive(&[b"trait-fixture-repository"]),
        WorktreeId::derive(&[b"trait-fixture-worktree"]),
    );
    let first = GenerationId::derive(&[b"no-trait"]);
    let resolved = GenerationId::derive(&[b"trait-added"]);
    let retracted = GenerationId::derive(&[b"trait-removed"]);
    for (generation, target, expected_edges) in [
        (first, "function", 0),
        (resolved, "trait", 1),
        (retracted, "function", 0),
    ] {
        indexer.index(
            &[
                source("implementation.fixture", "implementation"),
                source("target.fixture", target),
            ],
            IndexRunId::derive(&[&generation.0.0]),
            generation,
        )?;
        let snapshot = indexer.store().historical_snapshot(generation)?;
        if snapshot
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Implements)
            .count()
            != expected_edges
        {
            return Err(IndexError::Store(StoreError::Integrity(
                "trait edit did not update implementation edges".to_owned(),
            )));
        }
    }
    let retained = indexer.store().historical_snapshot(resolved)?;
    let trait_id = retained
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Trait)
        .map(|node| node.id)
        .ok_or_else(|| {
            IndexError::Store(StoreError::Integrity(
                "retained trait disappeared".to_owned(),
            ))
        })?;
    if !retained
        .edges
        .iter()
        .any(|edge| edge.relation == RelationKind::Implements && edge.target == trait_id)
    {
        return Err(IndexError::Store(StoreError::Integrity(
            "retained implementation edge disappeared".to_owned(),
        )));
    }
    Ok(())
}
