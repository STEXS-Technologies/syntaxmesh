use std::error::Error;
use std::path::PathBuf;

use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, Node,
    NodeId, NodeKind, Provenance, ProvenanceId, RelationKind, RepositoryId, SourceLocation,
    SourceSpan, WorktreeId,
};
use syntaxmesh_engine::{SemanticBatchLimits, SyntaxMeshEngine};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_semantic::{
    SEMANTIC_NAMESPACE, SemanticClaim, SemanticEvidence, SemanticOutput, SemanticProvider,
    SemanticProviderIdentity, SemanticRequest,
};
use syntaxmesh_store::{DurableRecordStore, FileGraphStore, GraphStore, InMemoryGraphStore};

#[path = "support/semantic_context.rs"]
mod semantic_context;

#[test]
fn semantic_generation_batches_are_bounded_and_span_documents() -> Result<(), Box<dyn Error>> {
    semantic_generation_fixture(InMemoryGraphStore::new()).map(|_| ())
}

#[test]
fn joint_document_context_survives_file_store_restart() -> Result<(), Box<dyn Error>> {
    let snapshot = TestSnapshot::new()?;
    let (generation, claim) = semantic_generation_fixture(FileGraphStore::open(&snapshot.0)?)?;
    let reopened = FileGraphStore::open(&snapshot.0)?;
    semantic_context::verify_joint_sources(
        &syntaxmesh_query::Query::new(&reopened, generation),
        claim,
    )
}

fn semantic_generation_fixture<S: GraphStore + DurableRecordStore>(
    mut store: S,
) -> Result<(GenerationId, NodeId), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"semantic-batch-repository"]);
    let worktree = WorktreeId::derive(&[b"semantic-batch-worktree"]);
    let generation = GenerationId::derive(&[b"semantic-batch-generation"]);
    let provider_identity = SemanticProviderIdentity {
        provider: "fixture".to_owned(),
        model: "documentation-model".to_owned(),
        model_revision: "rev-1".to_owned(),
        prompt_version: "doc-claims-v1".to_owned(),
        prompt_hash: [4; 32],
        configuration_hash: [5; 32],
    };
    let mut files = Vec::new();
    let mut provenance = Vec::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let docs = [
        ("docs/architecture.md", "The engine is runtime agnostic."),
        ("docs/decisions.md", "Durable workflows use Penelope."),
    ];
    for (path, content) in docs {
        let file_id = FileId::derive(&[path.as_bytes()]);
        let content_hash = *blake3::hash(content.as_bytes()).as_bytes();
        let source = SourceLocation {
            file_id,
            content_hash,
            span: SourceSpan::new(0, u64::try_from(content.len())?)?,
        };
        let producer = Provenance {
            id: ProvenanceId::derive(&[b"semantic-batch-source", path.as_bytes()]),
            producer_namespace: "syntaxmesh.lang.documentation".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: Some(source.clone()),
        };
        let document = Node {
            id: NodeId::derive(&[b"semantic-batch-document", path.as_bytes()]),
            kind: NodeKind::Document,
            name: path.to_owned(),
            owner_file: Some(file_id),
            source: Some(source.clone()),
            provenance: producer.id,
            extension_payload: None,
        };
        let section = Node {
            id: NodeId::derive(&[b"semantic-batch-section", path.as_bytes()]),
            kind: NodeKind::Section,
            name: "Architecture decisions".to_owned(),
            owner_file: Some(file_id),
            source: Some(SourceLocation {
                span: SourceSpan::new(0, 0)?,
                ..source.clone()
            }),
            provenance: producer.id,
            extension_payload: None,
        };
        let chunk = Node {
            id: NodeId::derive(&[b"semantic-batch-chunk", path.as_bytes()]),
            kind: NodeKind::DocumentChunk,
            name: content.to_owned(),
            owner_file: Some(file_id),
            source: Some(source),
            provenance: producer.id,
            extension_payload: None,
        };
        edges.push(Edge {
            id: EdgeId::derive(&[&document.id.0.0, &section.id.0.0, b"contains"]),
            source: document.id,
            target: section.id,
            relation: RelationKind::Contains,
            provenance: producer.id,
            extension_payload: None,
        });
        edges.push(Edge {
            id: EdgeId::derive(&[&section.id.0.0, &chunk.id.0.0, b"contains"]),
            source: section.id,
            target: chunk.id,
            relation: RelationKind::Contains,
            provenance: producer.id,
            extension_payload: None,
        });
        files.push(FileVersion {
            file_id,
            normalized_path: path.to_owned(),
            content_hash,
            size_bytes: u64::try_from(content.len())?,
        });
        provenance.push(producer);
        nodes.extend([document, section, chunk]);
    }
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"semantic-batch-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: files,
        removed_files: Vec::new(),
        upsert_provenance: provenance,
        upsert_nodes: nodes,
        upsert_edges: edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let mut engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);

    let single_chunk_batches = engine.semantic_requests_for_generation(
        generation,
        &provider_identity,
        SemanticBatchLimits {
            max_chunks: 1,
            max_bytes: 1024,
        },
    )?;
    if single_chunk_batches.len() != 2
        || single_chunk_batches
            .iter()
            .any(|request| request.prompt_chunks().len() != 1)
    {
        return Err("semantic generation batches did not respect the chunk bound".into());
    }

    let document_requests = engine.semantic_document_requests_for_generation(
        generation,
        &provider_identity,
        SemanticBatchLimits {
            max_chunks: 32,
            max_bytes: 1024,
        },
    )?;
    if document_requests.len() != 2
        || document_requests.iter().any(|request| {
            let chunks = request.prompt_chunks();
            chunks.len() != 1
                || chunks
                    .iter()
                    .any(|chunk| chunk.context != ["Architecture decisions"])
        })
    {
        return Err(
            "document semantic requests did not preserve isolated documents and context".into(),
        );
    }
    let request_owners = document_requests
        .iter()
        .filter_map(|request| {
            request
                .prompt_chunks()
                .first()
                .map(|chunk| chunk.content_hash)
        })
        .collect::<std::collections::BTreeSet<_>>();
    if request_owners.len() != 2 {
        return Err("document semantic requests reused one document cache key".into());
    }
    let first_document = document_requests
        .first()
        .ok_or("first document request is missing")?;
    let second_document = document_requests
        .get(1)
        .ok_or("second document request is missing")?;
    let second_chunk = second_document
        .prompt_chunks()
        .into_iter()
        .next()
        .ok_or("second document chunk is missing")?;
    if first_document
        .clone()
        .into_fact_batch(SemanticOutput {
            claims: vec![SemanticClaim {
                subject: "Durable workflows".to_owned(),
                relation: "use".to_owned(),
                object: "Penelope".to_owned(),
                evidence: vec![SemanticEvidence {
                    chunk_content_hash: second_chunk.content_hash,
                    quote: "Durable workflows use Penelope.".to_owned(),
                }],
            }],
        })
        .is_ok()
    {
        return Err("a document request accepted evidence owned by another document".into());
    }

    let combined = engine.semantic_requests_for_generation(
        generation,
        &provider_identity,
        SemanticBatchLimits {
            max_chunks: 2,
            max_bytes: 1024,
        },
    )?;
    let combined_request = combined
        .first()
        .ok_or("semantic generation batch was unexpectedly empty")?;
    if combined.len() != 1
        || combined_request.prompt_chunks().len() != 2
        || combined_request.source_document_count() != 2
        || first_document.source_document_count() != 1
    {
        return Err("semantic generation request could not span multiple documents".into());
    }
    if combined_request
        .prompt_chunks()
        .iter()
        .any(|chunk| chunk.context != ["Architecture decisions"])
    {
        return Err("semantic generation batching lost document section context".into());
    }
    if engine
        .semantic_requests_for_generation(
            generation,
            combined_request.identity(),
            SemanticBatchLimits {
                max_chunks: 2,
                max_bytes: 4,
            },
        )
        .is_ok()
    {
        return Err("oversized semantic chunk was accepted by the byte bound".into());
    }
    let source_snapshot = engine.query(generation).graph_at(generation)?;
    let joint_output = SemanticOutput {
        claims: vec![SemanticClaim {
            subject: "runtime agnostic engine".to_owned(),
            relation: "uses_durable_workflows".to_owned(),
            object: "Penelope".to_owned(),
            evidence: combined_request
                .prompt_chunks()
                .into_iter()
                .map(|chunk| SemanticEvidence {
                    chunk_content_hash: chunk.content_hash,
                    quote: chunk.text,
                })
                .collect(),
        }],
    };
    let mut invalid_output = joint_output.clone();
    invalid_output
        .claims
        .first_mut()
        .and_then(|claim| claim.evidence.first_mut())
        .ok_or("joint fixture lacks evidence")?
        .quote = "This quote does not occur in either document.".to_owned();
    if engine
        .enrich_document_semantics_many(
            combined.clone(),
            &FixtureProvider {
                identity: provider_identity.clone(),
                output: invalid_output,
            },
        )
        .is_ok()
        || engine.query(generation).graph_at(generation)? != source_snapshot
    {
        return Err("invalid joint evidence was accepted or changed source facts".into());
    }
    let provider = FixtureProvider {
        identity: provider_identity.clone(),
        output: joint_output,
    };
    let (validated_request, validated_output) =
        engine.enrich_document_semantic_outputs_many(combined.clone(), &provider)?;
    let facts = validated_request
        .clone()
        .into_fact_batch(validated_output.clone())?;
    let (cached_request, cached_output) = engine.enrich_document_semantic_outputs_many(
        combined.clone(),
        &FailingProvider(provider_identity.clone()),
    )?;
    if cached_request != validated_request || cached_output != validated_output {
        return Err("output composition path changed cached request or claims".into());
    }
    let (document_sources, document_output) = engine.enrich_document_semantic_outputs_many(
        document_requests,
        &DocumentExtractionProvider(provider_identity.clone()),
    )?;
    let layered_output = SemanticOutput::merge([document_output, validated_output]);
    let layered_facts = document_sources.into_fact_batch(layered_output)?;
    let layered_claims = layered_facts
        .nodes
        .iter()
        .filter(|node| {
            matches!(&node.kind, NodeKind::External { namespace, kind }
            if namespace == SEMANTIC_NAMESPACE && kind == "claim")
        })
        .count();
    let layered_supports = layered_facts
        .edges
        .iter()
        .filter(|edge| {
            matches!(&edge.relation, RelationKind::External { namespace, relation }
            if namespace == SEMANTIC_NAMESPACE && relation == "supports")
        })
        .count();
    if layered_claims != 3 || layered_supports != 4 {
        return Err("semantic layer composition lost document claims or joint evidence".into());
    }
    let cached =
        engine.enrich_document_semantics_many(combined, &FailingProvider(provider_identity))?;
    if cached != facts {
        return Err("cross-document request did not reuse its existing Penelope cache".into());
    }
    let claim = facts
        .nodes
        .iter()
        .find(|node| {
            matches!(&node.kind, NodeKind::External { namespace, kind }
            if namespace == SEMANTIC_NAMESPACE && kind == "claim")
        })
        .ok_or("cross-document claim was not constructed")?;
    let claim_id = claim.id;
    let support_sources = facts
        .edges
        .iter()
        .filter(|edge| {
            edge.target == claim_id
                && matches!(&edge.relation,
            RelationKind::External { namespace, relation }
                if namespace == SEMANTIC_NAMESPACE && relation == "supports")
        })
        .map(|edge| edge.source)
        .collect::<std::collections::BTreeSet<_>>();
    if support_sources.len() != 2 {
        return Err("joint claim lost one document's support".into());
    }
    let source_files = support_sources
        .iter()
        .map(|id| -> Result<FileId, Box<dyn Error>> {
            engine
                .query(generation)
                .node(*id)?
                .and_then(|node| node.owner_file)
                .ok_or_else(|| std::io::Error::other("joint evidence lost source file").into())
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    if source_files.len() != 2 {
        return Err("joint claim was not backed by distinct source documents".into());
    }
    let published = engine
        .ingest_extension_facts(
            facts,
            IndexRunId::derive(&[b"joint-document-run"]),
            GenerationId::derive(&[b"joint-document-result"]),
        )?
        .publication
        .generation
        .generation;
    if engine.query(published).node(claim_id)?.is_none()
        || engine.query(published).graph_at(generation)? != source_snapshot
    {
        return Err("joint semantic publication changed its historical source graph".into());
    }
    semantic_context::verify_joint_sources(&engine.query(published), claim_id)?;
    Ok((published, claim_id))
}

struct TestSnapshot(PathBuf);

impl TestSnapshot {
    fn new() -> Result<Self, Box<dyn Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        Ok(Self(std::env::temp_dir().join(format!(
            "syntaxmesh-semantic-document-{}-{stamp}.snapshot",
            std::process::id()
        ))))
    }
}

impl Drop for TestSnapshot {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.0));
    }
}

struct FixtureProvider {
    identity: SemanticProviderIdentity,
    output: SemanticOutput,
}

struct DocumentExtractionProvider(SemanticProviderIdentity);

impl SemanticProvider for DocumentExtractionProvider {
    fn identity(&self) -> SemanticProviderIdentity {
        self.0.clone()
    }

    fn extract(&self, request: &SemanticRequest) -> Result<SemanticOutput, String> {
        let chunks = request.prompt_chunks();
        let chunk = chunks.first().ok_or("missing document source")?;
        if chunks.len() != 1 {
            return Err("document fixture received a compound request".to_owned());
        }
        let (subject, relation, object) = if chunk.text == "The engine is runtime agnostic." {
            ("engine", "has_constraint", "runtime agnostic")
        } else if chunk.text == "Durable workflows use Penelope." {
            ("durable workflows", "use", "Penelope")
        } else {
            return Err("unexpected document fixture source".to_owned());
        };
        Ok(SemanticOutput {
            claims: vec![SemanticClaim {
                subject: subject.to_owned(),
                relation: relation.to_owned(),
                object: object.to_owned(),
                evidence: vec![SemanticEvidence {
                    chunk_content_hash: chunk.content_hash,
                    quote: chunk.text.clone(),
                }],
            }],
        })
    }
}

impl SemanticProvider for FixtureProvider {
    fn identity(&self) -> SemanticProviderIdentity {
        self.identity.clone()
    }

    fn extract(&self, _request: &SemanticRequest) -> Result<SemanticOutput, String> {
        Ok(self.output.clone())
    }
}

struct FailingProvider(SemanticProviderIdentity);

impl SemanticProvider for FailingProvider {
    fn identity(&self) -> SemanticProviderIdentity {
        self.0.clone()
    }

    fn extract(&self, _request: &SemanticRequest) -> Result<SemanticOutput, String> {
        Err("injected provider failure".to_owned())
    }
}

#[test]
fn semantic_document_claims_publish_as_queryable_generation_scoped_facts()
-> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"semantic-document-repository"]);
    let worktree = WorktreeId::derive(&[b"semantic-document-worktree"]);
    let snapshot = TestSnapshot::new()?;
    let file_id = FileId::derive(&[b"docs/architecture.md"]);
    let orphan_file_id = FileId::derive(&[b"docs/orphan.md"]);
    let content = "The runtime-neutral engine supports multiple hosts.";
    let content_hash = *blake3::hash(content.as_bytes()).as_bytes();
    let orphan_content = "This chunk has no containment parent.";
    let orphan_hash = *blake3::hash(orphan_content.as_bytes()).as_bytes();
    let source_location = SourceLocation {
        file_id,
        content_hash,
        span: SourceSpan::new(0, u64::try_from(content.len())?)?,
    };
    let document_provenance = Provenance {
        id: ProvenanceId::derive(&[b"semantic-document-producer", &content_hash]),
        producer_namespace: "syntaxmesh.lang.documentation".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: Some(source_location.clone()),
    };
    let orphan_provenance = Provenance {
        id: ProvenanceId::derive(&[b"semantic-orphan-producer", &orphan_hash]),
        producer_namespace: "syntaxmesh.lang.documentation".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: Some(SourceLocation {
            file_id: orphan_file_id,
            content_hash: orphan_hash,
            span: SourceSpan::new(0, u64::try_from(orphan_content.len())?)?,
        }),
    };
    let chunk = Node {
        id: NodeId::derive(&[b"semantic-document-chunk"]),
        kind: NodeKind::DocumentChunk,
        name: content.to_owned(),
        owner_file: Some(file_id),
        source: Some(source_location),
        provenance: document_provenance.id,
        extension_payload: None,
    };
    let document = Node {
        id: NodeId::derive(&[b"semantic-document-root"]),
        kind: NodeKind::Document,
        name: "docs/architecture.md".to_owned(),
        owner_file: Some(file_id),
        source: Some(SourceLocation {
            file_id,
            content_hash,
            span: SourceSpan::new(0, u64::try_from(content.len())?)?,
        }),
        provenance: document_provenance.id,
        extension_payload: None,
    };
    let section = Node {
        id: NodeId::derive(&[b"semantic-document-section"]),
        kind: NodeKind::Section,
        name: "Runtime architecture".to_owned(),
        owner_file: Some(file_id),
        source: Some(SourceLocation {
            file_id,
            content_hash,
            span: SourceSpan::new(0, 0)?,
        }),
        provenance: document_provenance.id,
        extension_payload: None,
    };
    let nested_section = Node {
        id: NodeId::derive(&[b"semantic-document-nested-section"]),
        kind: NodeKind::Section,
        name: "Hosting".to_owned(),
        owner_file: Some(file_id),
        source: Some(SourceLocation {
            file_id,
            content_hash,
            span: SourceSpan::new(0, 0)?,
        }),
        provenance: document_provenance.id,
        extension_payload: None,
    };
    let orphan_chunk = Node {
        id: NodeId::derive(&[b"semantic-orphan-chunk"]),
        kind: NodeKind::DocumentChunk,
        name: orphan_content.to_owned(),
        owner_file: Some(orphan_file_id),
        source: Some(SourceLocation {
            file_id: orphan_file_id,
            content_hash: orphan_hash,
            span: SourceSpan::new(0, u64::try_from(orphan_content.len())?)?,
        }),
        provenance: orphan_provenance.id,
        extension_payload: None,
    };
    let document_section = Edge {
        id: EdgeId::derive(&[&document.id.0.0, &section.id.0.0, b"contains"]),
        source: document.id,
        target: section.id,
        relation: RelationKind::Contains,
        provenance: document_provenance.id,
        extension_payload: None,
    };
    let section_nested = Edge {
        id: EdgeId::derive(&[&section.id.0.0, &nested_section.id.0.0, b"contains"]),
        source: section.id,
        target: nested_section.id,
        relation: RelationKind::Contains,
        provenance: document_provenance.id,
        extension_payload: None,
    };
    let nested_chunk = Edge {
        id: EdgeId::derive(&[&nested_section.id.0.0, &chunk.id.0.0, b"contains"]),
        source: nested_section.id,
        target: chunk.id,
        relation: RelationKind::Contains,
        provenance: document_provenance.id,
        extension_payload: None,
    };
    let base_generation = GenerationId::derive(&[b"semantic-document-base"]);
    let mut store = FileGraphStore::open(&snapshot.0)?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"semantic-document-base-run"]),
        expected_base: None,
        next_generation: base_generation,
        changed_files: vec![
            FileVersion {
                file_id,
                normalized_path: "docs/architecture.md".to_owned(),
                content_hash,
                size_bytes: u64::try_from(content.len())?,
            },
            FileVersion {
                file_id: orphan_file_id,
                normalized_path: "docs/orphan.md".to_owned(),
                content_hash: orphan_hash,
                size_bytes: u64::try_from(orphan_content.len())?,
            },
        ],
        removed_files: Vec::new(),
        upsert_provenance: vec![document_provenance, orphan_provenance],
        upsert_nodes: vec![
            chunk.clone(),
            document,
            section,
            nested_section,
            orphan_chunk,
        ],
        upsert_edges: vec![document_section, section_nested, nested_chunk],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;

    let provider_identity = SemanticProviderIdentity {
        provider: "fixture".to_owned(),
        model: "documentation-model".to_owned(),
        model_revision: "rev-1".to_owned(),
        prompt_version: "doc-claims-v1".to_owned(),
        prompt_hash: [4; 32],
        configuration_hash: [5; 32],
    };
    let subject_id = NodeId::derive(&[
        SEMANTIC_NAMESPACE.as_bytes(),
        b"concept",
        b"runtime-neutral engine",
    ]);
    let mut engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    if engine
        .semantic_request_for_file(base_generation, orphan_file_id, provider_identity.clone())
        .is_ok()
    {
        return Err("semantic request silently accepted a missing containment hierarchy".into());
    }
    let request =
        engine.semantic_request_for_file(base_generation, file_id, provider_identity.clone())?;
    let prompt_chunks = request.prompt_chunks();
    if prompt_chunks.len() != 1
        || prompt_chunks
            .first()
            .is_none_or(|item| item.text.as_str() != content)
    {
        return Err("semantic request did not reuse indexed document chunk text".into());
    }
    let prompt_chunk = prompt_chunks
        .first()
        .ok_or("semantic request did not contain its indexed chunk")?;
    if prompt_chunk.context != ["Runtime architecture", "Hosting"] {
        return Err("semantic request dropped or misordered authored section context".into());
    }
    if prompt_chunk
        .context
        .iter()
        .any(|part| part.contains("docs/"))
    {
        return Err("semantic request exposed a physical document path".into());
    }
    let provider = FixtureProvider {
        identity: provider_identity,
        output: SemanticOutput {
            claims: vec![SemanticClaim {
                subject: "runtime-neutral engine".to_owned(),
                relation: "supports_runtime_agnostic_hosts".to_owned(),
                object: "multiple hosts".to_owned(),
                evidence: vec![SemanticEvidence {
                    chunk_content_hash: prompt_chunk.content_hash,
                    quote: content.to_owned(),
                }],
            }],
        },
    };
    let failing_provider = FailingProvider(provider.identity.clone());
    if engine
        .enrich_document_semantics(request.clone(), &failing_provider)
        .is_ok()
    {
        return Err("injected semantic provider failure unexpectedly succeeded".into());
    }
    if engine.query(base_generation).node(chunk.id)?.is_none() {
        return Err("semantic failure mutated deterministic document facts".into());
    }
    let semantic_batch = engine.enrich_document_semantics(request, &provider)?;
    let receipt = engine.ingest_extension_facts(
        semantic_batch,
        IndexRunId::derive(&[b"semantic-document-run"]),
        GenerationId::derive(&[b"semantic-document-result"]),
    )?;
    let generation = receipt.publication.generation.generation;
    let subject = engine
        .query(generation)
        .node(subject_id)?
        .ok_or("semantic concept was not queryable")?;
    if subject.name != "runtime-neutral engine"
        || !matches!(subject.kind, NodeKind::External { ref namespace, ref kind }
            if namespace == SEMANTIC_NAMESPACE && kind == "concept")
    {
        return Err("semantic concept identity or generation scope is incorrect".into());
    }

    let supported_claims = engine
        .query(generation)
        .neighbors(chunk.id)?
        .into_iter()
        .filter(|(edge, node)| {
            matches!(&edge.relation, syntaxmesh_core::RelationKind::External { namespace, relation }
                if namespace == SEMANTIC_NAMESPACE && relation == "supports")
                && matches!(&node.kind, NodeKind::External { namespace, kind }
                    if namespace == SEMANTIC_NAMESPACE && kind == "claim")
        })
        .collect::<Vec<_>>();
    if supported_claims.len() != 1 {
        return Err("document chunk did not link to exactly one semantic claim".into());
    }
    let claim_id = supported_claims
        .first()
        .map(|(_, node)| node.id)
        .ok_or("semantic claim edge disappeared")?;
    let claim_targets = engine.query(generation).neighbors(claim_id)?;
    if claim_targets.len() != 2
        || !claim_targets.iter().any(|(_, node)| node.id == subject_id)
        || !claim_targets
            .iter()
            .any(|(_, node)| node.name == "multiple hosts")
    {
        return Err("semantic claim did not link to both concepts".into());
    }
    let semantic_provenance =
        engine
            .query(generation)
            .export_records()?
            .into_iter()
            .any(|record| {
                matches!(record, syntaxmesh_api_model::GraphRecord::Provenance { provenance, .. }
                if provenance.evidence_class == EvidenceClass::SemanticInference
                    && provenance.source.is_some())
            });
    if !semantic_provenance {
        return Err("semantic inference lost its source-grounded provenance".into());
    }
    drop(engine);
    let persisted = FileGraphStore::open(&snapshot.0)?;
    if persisted
        .historical_node(base_generation, subject_id)?
        .is_some()
    {
        return Err("semantic concept leaked into the prior generation".into());
    }
    Ok(())
}
