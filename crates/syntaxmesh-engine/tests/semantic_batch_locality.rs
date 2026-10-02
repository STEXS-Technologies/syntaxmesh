use std::error::Error;

use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, Node,
    NodeId, NodeKind, Provenance, ProvenanceId, RelationKind, RepositoryId, SourceLocation,
    SourceSpan, WorktreeId,
};
use syntaxmesh_engine::{SemanticBatchLimits, SyntaxMeshEngine};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_semantic::SemanticProviderIdentity;
use syntaxmesh_store::{GraphStore, InMemoryGraphStore};

#[test]
fn compound_partition_prefers_directories_and_source_order() -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"locality"]);
    let worktree = WorktreeId::derive(&[b"locality"]);
    let generation = GenerationId::derive(&[b"locality"]);
    let identity = SemanticProviderIdentity {
        provider: "fixture".to_owned(),
        model: "fixture".to_owned(),
        model_revision: "1".to_owned(),
        prompt_version: "1".to_owned(),
        prompt_hash: [1; 32],
        configuration_hash: [2; 32],
    };
    let mut file_ids = [b"a", b"b", b"c", b"d"].map(|seed| FileId::derive(&[seed]));
    file_ids.sort();
    // Identity order intentionally alternates directories and reverses file names.
    let paths = ["docs/a/z.md", "docs/b/z.md", "docs/a/a.md", "docs/b/a.md"];
    let mut files = Vec::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut producers = Vec::new();
    for (file_id, path) in file_ids.into_iter().zip(paths) {
        let first = format!("{path} first.");
        let second = format!("{path} second.");
        let content = format!("{first}{second}");
        let content_hash = *blake3::hash(content.as_bytes()).as_bytes();
        let source = SourceLocation {
            file_id,
            content_hash,
            span: SourceSpan::new(0, u64::try_from(content.len())?)?,
        };
        let producer = Provenance {
            id: ProvenanceId::derive(&[path.as_bytes()]),
            producer_namespace: "syntaxmesh.lang.documentation".to_owned(),
            producer_version: "fixture".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: Some(source.clone()),
        };
        let mut node_ids = [
            NodeId::derive(&[path.as_bytes(), b"first"]),
            NodeId::derive(&[path.as_bytes(), b"second"]),
        ];
        node_ids.sort();
        node_ids.reverse();
        let document_id = NodeId::derive(&[path.as_bytes(), b"document"]);
        nodes.push(Node {
            id: document_id,
            kind: NodeKind::Document,
            name: path.to_owned(),
            owner_file: Some(file_id),
            source: Some(source.clone()),
            provenance: producer.id,
            extension_payload: None,
        });
        let boundary = u64::try_from(first.len())?;
        for ((name, span), node_id) in [
            (first, SourceSpan::new(0, boundary)?),
            (second, SourceSpan::new(boundary, source.span.end_byte)?),
        ]
        .into_iter()
        .zip(node_ids)
        {
            edges.push(Edge {
                id: EdgeId::derive(&[&document_id.0.0, &node_id.0.0]),
                source: document_id,
                target: node_id,
                relation: RelationKind::Contains,
                provenance: producer.id,
                extension_payload: None,
            });
            nodes.push(Node {
                id: node_id,
                kind: NodeKind::DocumentChunk,
                name,
                owner_file: Some(file_id),
                source: Some(SourceLocation {
                    span,
                    ..source.clone()
                }),
                provenance: producer.id,
                extension_payload: None,
            });
        }
        files.push(FileVersion {
            file_id,
            normalized_path: path.to_owned(),
            content_hash,
            size_bytes: u64::try_from(content.len())?,
        });
        producers.push(producer);
    }
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"locality"]),
        expected_base: None,
        next_generation: generation,
        changed_files: files,
        removed_files: Vec::new(),
        upsert_provenance: producers,
        upsert_nodes: nodes,
        upsert_edges: edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    let limits = SemanticBatchLimits {
        max_chunks: 4,
        max_bytes: 1024,
    };
    let batches = engine.semantic_requests_for_generation(generation, &identity, limits)?;
    if batches.len() != 2 {
        return Err("unexpected compound partition count".into());
    }
    for (request, directory) in batches.iter().zip(["docs/a/", "docs/b/"]) {
        if request.source_document_count() != 2
            || request.prompt_chunks().len() != 4
            || request
                .prompt_chunks()
                .iter()
                .any(|chunk| !chunk.text.starts_with(directory))
        {
            return Err("compound partition ignored directory locality".into());
        }
    }
    let repeated = engine.semantic_requests_for_generation(generation, &identity, limits)?;
    if batches
        .iter()
        .map(|request| request.cache_key())
        .collect::<Vec<_>>()
        != repeated
            .iter()
            .map(|request| request.cache_key())
            .collect::<Vec<_>>()
    {
        return Err("compound partitions were not deterministic".into());
    }
    let singles = engine.semantic_requests_for_generation(
        generation,
        &identity,
        SemanticBatchLimits {
            max_chunks: 1,
            max_bytes: 1024,
        },
    )?;
    let expected = ["docs/a/a.md", "docs/a/z.md", "docs/b/a.md", "docs/b/z.md"]
        .into_iter()
        .flat_map(|path| [format!("{path} first."), format!("{path} second.")])
        .collect::<Vec<_>>();
    let actual = singles
        .iter()
        .flat_map(|request| request.prompt_chunks())
        .map(|chunk| chunk.text)
        .collect::<Vec<_>>();
    if actual != expected {
        return Err("partition ignored path/source order or chunk bounds".into());
    }
    Ok(())
}
