use super::*;

macro_rules! ensure {
    ($condition:expr, $message:literal) => {
        if !$condition {
            return Err($message.to_owned());
        }
    };
}

fn identity() -> SemanticProviderIdentity {
    SemanticProviderIdentity {
        provider: "local".to_owned(),
        model: "fixture-model".to_owned(),
        model_revision: "rev-1".to_owned(),
        prompt_version: "doc-claims-v1".to_owned(),
        prompt_hash: [1; 32],
        configuration_hash: [2; 32],
    }
}

fn chunk(id: &[u8], file: &[u8], text: &str, offset: u64) -> SemanticDocumentChunk {
    let file_id = syntaxmesh_core::FileId::derive(&[file]);
    let node_id = NodeId::derive(&[id]);
    let start = offset;
    let end = start.saturating_add(u64::try_from(text.len()).unwrap_or(u64::MAX));
    SemanticDocumentChunk {
        node: Node {
            id: node_id,
            kind: NodeKind::DocumentChunk,
            name: text.to_owned(),
            owner_file: Some(file_id),
            source: Some(SourceLocation {
                file_id,
                content_hash: *blake3::hash(text.as_bytes()).as_bytes(),
                span: SourceSpan::new(start, end).unwrap_or(SourceSpan {
                    start_byte: start,
                    end_byte: end,
                }),
            }),
            provenance: ProvenanceId::derive(&[b"source"]),
            extension_payload: None,
        },
        text: text.to_owned(),
        context: Vec::new(),
    }
}

fn claim(evidence: Vec<SemanticEvidence>) -> SemanticClaim {
    SemanticClaim {
        subject: " Runtime-neutral engine ".to_owned(),
        relation: "supports_runtime_agnostic_hosts".to_owned(),
        object: "multiple hosts".to_owned(),
        evidence,
    }
}

#[test]
fn cache_identity_ignores_physical_chunk_ids_but_tracks_content_and_model_config()
-> Result<(), String> {
    let first = SemanticRequest::new(
        identity(),
        vec![chunk(b"first-node", b"first-file", "Exact source text.", 4)],
    )
    .map_err(|error| error.to_string())?;
    let second = SemanticRequest::new(
        identity(),
        vec![chunk(
            b"other-node",
            b"other-file",
            "Exact source text.",
            200,
        )],
    )
    .map_err(|error| error.to_string())?;
    ensure!(
        first.cache_key() == second.cache_key(),
        "same content must reuse cache key"
    );

    let changed = SemanticRequest::new(
        SemanticProviderIdentity {
            model_revision: "rev-2".to_owned(),
            ..identity()
        },
        vec![chunk(b"first-node", b"first-file", "Exact source text.", 4)],
    )
    .map_err(|error| error.to_string())?;
    ensure!(
        first.cache_key() != changed.cache_key(),
        "model revision must change cache key"
    );
    Ok(())
}

#[test]
fn cache_identity_includes_authored_section_context() -> Result<(), String> {
    let mut architecture = chunk(b"architecture-node", b"architecture-file", "Same prose.", 0);
    architecture.context = vec!["Architecture".to_owned(), "Runtime model".to_owned()];
    let mut operations = chunk(b"operations-node", b"operations-file", "Same prose.", 0);
    operations.context = vec!["Operations".to_owned(), "Runtime model".to_owned()];
    let first = SemanticRequest::new(identity(), vec![architecture.clone()])
        .map_err(|error| error.to_string())?;
    let changed_context =
        SemanticRequest::new(identity(), vec![operations]).map_err(|error| error.to_string())?;
    let moved = SemanticRequest::new(
        identity(),
        vec![SemanticDocumentChunk {
            node: chunk(b"moved-node", b"moved-file", "Same prose.", 200).node,
            text: "Same prose.".to_owned(),
            context: architecture.context,
        }],
    )
    .map_err(|error| error.to_string())?;
    ensure!(
        first.cache_key() != changed_context.cache_key(),
        "changing authored headings must invalidate semantic cache"
    );
    ensure!(
        first.cache_key() == moved.cache_key(),
        "same text and headings under another physical path must reuse cache"
    );
    Ok(())
}

#[test]
fn deserialized_request_fingerprints_are_revalidated() -> Result<(), String> {
    let mut request = SemanticRequest::new(
        identity(),
        vec![chunk(b"node", b"file", "Authored prose.", 0)],
    )
    .map_err(|error| error.to_string())?;
    request.input_hash[0] ^= 1;
    ensure!(
        request.validate() == Err(SemanticError::InvalidRequestFingerprint),
        "altered persisted fingerprint must be rejected"
    );
    Ok(())
}

#[test]
fn semantic_batch_contains_exact_source_evidence_and_separate_provenance() -> Result<(), String> {
    let first = chunk(
        b"first-node",
        b"first-file",
        "Architectural decision text.",
        10,
    );
    let second = chunk(b"second-node", b"second-file", "Supporting rationale.", 40);
    let request = SemanticRequest::new(identity(), vec![first.clone(), second.clone()])
        .map_err(|error| error.to_string())?;
    let batch = request
        .into_fact_batch(SemanticOutput {
            claims: vec![claim(vec![
                SemanticEvidence {
                    chunk_content_hash: first.content_hash(),
                    quote: "decision text".to_owned(),
                },
                SemanticEvidence {
                    chunk_content_hash: second.content_hash(),
                    quote: "Supporting rationale".to_owned(),
                },
            ])],
        })
        .map_err(|error| error.to_string())?;
    ensure!(batch.validate().is_ok(), "fact batch must validate");
    ensure!(
        batch.manifest.capabilities == [Capability::AnalysisFacts],
        "analysis capability required"
    );
    ensure!(
        batch.nodes.len() == 3,
        "two concepts and one claim node required"
    );
    ensure!(
        batch.edges.len() == 4,
        "evidence and claim-role edges required"
    );
    ensure!(
        batch
            .provenance
            .iter()
            .all(|item| item.evidence_class == EvidenceClass::SemanticInference),
        "semantic evidence provenance required"
    );
    let start = 10_u64.saturating_add(u64::try_from("Architectural ".len()).unwrap_or(u64::MAX));
    ensure!(
        batch.provenance.iter().any(|item| {
            item.source.as_ref().is_some_and(|location| {
                location.span.start_byte == start
                    && location.span.end_byte
                        == start.saturating_add(
                            u64::try_from("decision text".len()).unwrap_or(u64::MAX),
                        )
            })
        }),
        "exact quote span required"
    );
    Ok(())
}

#[test]
fn output_rejects_quotes_not_in_the_cited_chunk() -> Result<(), String> {
    let source = chunk(b"evidence-node", b"evidence-file", "Authored reason.", 3);
    let request = SemanticRequest::new(identity(), vec![source.clone()])
        .map_err(|error| error.to_string())?;
    let result = request.into_fact_batch(SemanticOutput {
        claims: vec![claim(vec![SemanticEvidence {
            chunk_content_hash: source.content_hash(),
            quote: "invented rationale".to_owned(),
        }])],
    });
    ensure!(
        result == Err(SemanticError::EvidenceQuoteNotFound),
        "uncited quote must fail"
    );
    Ok(())
}

#[test]
fn requests_reject_non_document_nodes_and_malformed_relations() -> Result<(), String> {
    ensure!(
        SemanticRequest::new(identity(), Vec::new()) == Err(SemanticError::EmptyDocumentInput),
        "empty semantic input must fail"
    );
    let mut code = chunk(b"code-node", b"code-file", "source", 0);
    code.node.kind = NodeKind::Function;
    ensure!(
        SemanticRequest::new(identity(), vec![code]) == Err(SemanticError::NotDocumentChunk),
        "non-document nodes must fail"
    );

    let source = chunk(b"doc-node", b"doc-file", "A fact.", 0);
    let mut altered_text = chunk(b"altered-node", b"altered-file", "A fact.", 0);
    altered_text.text = "B fact.".to_owned();
    ensure!(
        SemanticRequest::new(identity(), vec![altered_text])
            == Err(SemanticError::InvalidDocumentChunk),
        "provider text must be identical to the indexed chunk name"
    );
    let request = SemanticRequest::new(identity(), vec![source.clone()])
        .map_err(|error| error.to_string())?;
    let mut invalid = claim(vec![SemanticEvidence {
        chunk_content_hash: source.content_hash(),
        quote: "fact".to_owned(),
    }]);
    invalid.relation = "HAS SPACE".to_owned();
    ensure!(
        request.into_fact_batch(SemanticOutput {
            claims: vec![invalid],
        }) == Err(SemanticError::InvalidRelationLabel),
        "malformed relation must fail"
    );
    Ok(())
}
