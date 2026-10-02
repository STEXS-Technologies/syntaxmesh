use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, Node, NodeId, NodeKind, Provenance, ProvenanceId, RelationKind,
    SourceLocation, SourceSpan, StableId,
};
use syntaxmesh_extension_sdk::{
    Capability, EXTENSION_MANIFEST_SCHEMA_VERSION, ExtensionManifest, FactBatch,
};

pub const SEMANTIC_NAMESPACE: &str = "syntaxmesh.semantic";

/// Stable model/extraction configuration used for content-addressed reuse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticProviderIdentity {
    pub provider: String,
    pub model: String,
    pub model_revision: String,
    pub prompt_version: String,
    pub prompt_hash: [u8; 32],
    pub configuration_hash: [u8; 32],
}

impl SemanticProviderIdentity {
    fn validate(&self) -> Result<(), SemanticError> {
        if [
            &self.provider,
            &self.model,
            &self.model_revision,
            &self.prompt_version,
        ]
        .into_iter()
        .any(|value| value.trim().is_empty())
        {
            return Err(SemanticError::InvalidProviderIdentity);
        }
        Ok(())
    }

    /// Stable producer version used by semantic fact manifests and provenance.
    #[must_use]
    pub fn producer_version(&self) -> String {
        format!(
            "{}:{}@{};prompt={}:{};config={}",
            self.provider,
            self.model,
            self.model_revision,
            self.prompt_version,
            hex_hash(&self.prompt_hash),
            hex_hash(&self.configuration_hash)
        )
    }
}

/// One indexed document chunk provided as semantic-extraction input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticDocumentChunk {
    pub node: Node,
    pub text: String,
    /// Authored section headings from outermost to nearest. These inform the
    /// provider but are not accepted as quote evidence for the chunk.
    #[serde(default)]
    pub context: Vec<String>,
}

impl SemanticDocumentChunk {
    /// Content identity for exact chunk text plus authored heading context.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        if self.context.is_empty() {
            return *blake3::hash(self.text.as_bytes()).as_bytes();
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"syntaxmesh-semantic-document-chunk-v1\0");
        update_hash_component(&mut hasher, self.text.as_bytes());
        for section in &self.context {
            update_hash_component(&mut hasher, section.as_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

/// Evidence points to content, not physical node IDs, so cached model output
/// can be rebound to identical chunks in another snapshot or worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticEvidence {
    pub chunk_content_hash: [u8; 32],
    pub quote: String,
}

/// Provider-visible content without repository paths or physical graph IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticPromptChunk {
    /// Hash of both exact text and ordered section context.
    pub content_hash: [u8; 32],
    pub text: String,
    pub context: Vec<String>,
}

/// One model-derived subject/relation/object assertion with exact evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticClaim {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub evidence: Vec<SemanticEvidence>,
}

/// Structured provider output. No generated prose is accepted as source text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticOutput {
    pub claims: Vec<SemanticClaim>,
}

impl SemanticOutput {
    /// Merge claims from independent semantic requests in deterministic order.
    /// Identical normalized assertions retain all evidence, allowing one
    /// corpus-level fact batch to bind duplicate claims to every cited source.
    #[must_use]
    pub fn merge(outputs: impl IntoIterator<Item = Self>) -> Self {
        let mut claims = BTreeMap::<(String, String, String), SemanticClaim>::new();
        for output in outputs {
            for claim in output.claims {
                let key = (
                    normalized_text_key(&claim.subject),
                    claim.relation.trim().to_ascii_lowercase(),
                    normalized_text_key(&claim.object),
                );
                if let Some(existing) = claims.get_mut(&key) {
                    existing.evidence.extend(claim.evidence);
                } else {
                    claims.insert(key, claim);
                }
            }
        }
        for claim in claims.values_mut() {
            claim
                .evidence
                .sort_by_key(|item| (item.chunk_content_hash, item.quote.clone()));
            claim.evidence.dedup();
        }
        Self {
            claims: claims.into_values().collect(),
        }
    }

    /// Construct an empty, valid semantic fact batch for clearing/replacing a
    /// prior semantic layer when the current generation has no document
    /// chunks or the provider extracts no claims.
    #[must_use]
    pub fn empty_fact_batch(identity: &SemanticProviderIdentity) -> FactBatch {
        FactBatch {
            manifest: ExtensionManifest {
                schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
                namespace: SEMANTIC_NAMESPACE.to_owned(),
                producer_version: identity.producer_version(),
                capabilities: vec![Capability::AnalysisFacts],
            },
            provenance: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            observations: Vec::new(),
        }
    }
}

/// Provider-neutral request, ready for a hosted or local model adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticRequest {
    identity: SemanticProviderIdentity,
    chunks: Vec<SemanticDocumentChunk>,
    input_hash: [u8; 32],
    cache_key: StableId,
}

/// Isolated semantic provider contract. The core/indexer do not depend on it.
pub trait SemanticProvider {
    /// Stable identity including provider/model/prompt/configuration revisions.
    fn identity(&self) -> SemanticProviderIdentity;

    /// Extract semantic claims from the request's source-backed chunks.
    ///
    /// # Errors
    /// Returns a provider-specific failure without affecting deterministic
    /// graph publication.
    fn extract(&self, request: &SemanticRequest) -> Result<SemanticOutput, String>;

    /// Extract several independent bounded requests.
    ///
    /// The default is deterministic and sequential. Providers with safe
    /// concurrent transport can override it to reduce batch wall time. The
    /// returned results must have the same length and order as `requests`.
    fn extract_many(&self, requests: &[SemanticRequest]) -> Vec<Result<SemanticOutput, String>> {
        requests
            .iter()
            .map(|request| self.extract(request))
            .collect()
    }
}

impl SemanticRequest {
    /// Validate source chunks and compute a content-addressed cache key.
    ///
    /// # Errors
    /// Returns an error if provider identity is incomplete or any chunk is not
    /// a valid source-backed `DocumentChunk`.
    pub fn new(
        identity: SemanticProviderIdentity,
        mut chunks: Vec<SemanticDocumentChunk>,
    ) -> Result<Self, SemanticError> {
        identity.validate()?;
        if chunks.is_empty() {
            return Err(SemanticError::EmptyDocumentInput);
        }
        chunks.sort_by_key(|chunk| (chunk.content_hash(), chunk.node.id));
        let mut identities = BTreeSet::new();
        for chunk in &chunks {
            if chunk.node.kind != NodeKind::DocumentChunk {
                return Err(SemanticError::NotDocumentChunk);
            }
            let source = chunk
                .node
                .source
                .as_ref()
                .ok_or(SemanticError::MissingSourceLocation)?;
            if chunk.text.is_empty()
                || chunk.node.name != chunk.text
                || chunk.node.owner_file != Some(source.file_id)
                || u64::try_from(chunk.text.len()).ok()
                    != Some(source.span.end_byte.saturating_sub(source.span.start_byte))
                || !identities.insert(chunk.node.id)
            {
                return Err(SemanticError::InvalidDocumentChunk);
            }
        }
        let input_hash = input_hash(&chunks);
        let cache_key = StableId::derive(
            "semantic-enrichment-cache-v1",
            &[
                identity.provider.as_bytes(),
                identity.model.as_bytes(),
                identity.model_revision.as_bytes(),
                identity.prompt_version.as_bytes(),
                &identity.prompt_hash,
                &identity.configuration_hash,
                &input_hash,
            ],
        );
        Ok(Self {
            identity,
            chunks,
            input_hash,
            cache_key,
        })
    }

    /// Combine bounded requests from the same provider identity into one
    /// evidence-validation request without invoking the provider. This is used
    /// after batch inference to validate and bind cross-batch output against
    /// the complete source corpus.
    ///
    /// # Errors
    /// Returns an error if requests use different provider identities or any
    /// combined source chunk is invalid.
    pub fn combine(requests: Vec<Self>) -> Result<Self, SemanticError> {
        let mut requests = requests.into_iter();
        let first = requests.next().ok_or(SemanticError::EmptyDocumentInput)?;
        let identity = first.identity;
        let mut chunks = first.chunks;
        for request in requests {
            if request.identity != identity {
                return Err(SemanticError::MixedProviderIdentities);
            }
            chunks.extend(request.chunks);
        }
        Self::new(identity, chunks)
    }

    /// Cache identity covering provider, model, prompt, configuration, and
    /// content-only input, deliberately excluding physical node IDs/paths.
    #[must_use]
    pub const fn cache_key(&self) -> StableId {
        self.cache_key
    }

    /// Content fingerprint of the exact semantic input corpus.
    #[must_use]
    pub const fn input_hash(&self) -> [u8; 32] {
        self.input_hash
    }

    /// Provider/model identity used to produce this request.
    #[must_use]
    pub const fn identity(&self) -> &SemanticProviderIdentity {
        &self.identity
    }

    /// Count distinct source documents without exposing paths to the provider.
    #[must_use]
    pub fn source_document_count(&self) -> usize {
        self.chunks
            .iter()
            .filter_map(|chunk| chunk.node.owner_file)
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// Recompute and validate the request's content and cache fingerprints.
    ///
    /// Persisted requests must be checked after deserialization before they
    /// are passed to a provider or used as durable cache identities.
    ///
    /// # Errors
    /// Returns an error if any source chunk is invalid or either stored
    /// fingerprint disagrees with the canonical request content.
    pub fn validate(&self) -> Result<(), SemanticError> {
        let canonical = Self::new(self.identity.clone(), self.chunks.clone())?;
        if canonical.input_hash != self.input_hash || canonical.cache_key != self.cache_key {
            return Err(SemanticError::InvalidRequestFingerprint);
        }
        Ok(())
    }

    /// Provider-visible text in stable content order. This omits repository
    /// paths and graph row IDs; providers cite the content hash and exact quote.
    #[must_use]
    pub fn prompt_chunks(&self) -> Vec<SemanticPromptChunk> {
        self.chunks
            .iter()
            .map(|chunk| SemanticPromptChunk {
                content_hash: chunk.content_hash(),
                text: chunk.text.clone(),
                context: chunk.context.clone(),
            })
            .collect()
    }

    /// Validate output evidence and turn claims into namespaced extension
    /// facts that can be published through the ordinary engine workflow.
    ///
    /// # Errors
    /// Returns an error for ungrounded claims, malformed labels, missing
    /// evidence, or invalid source-span arithmetic.
    pub fn into_fact_batch(self, output: SemanticOutput) -> Result<FactBatch, SemanticError> {
        let producer_version = self.identity.producer_version();
        let mut source_chunks = BTreeMap::<[u8; 32], Vec<&SemanticDocumentChunk>>::new();
        for chunk in &self.chunks {
            let hash = chunk.content_hash();
            source_chunks.entry(hash).or_default().push(chunk);
        }

        let mut concepts = BTreeMap::<String, Node>::new();
        let mut claims = BTreeMap::<String, Node>::new();
        let mut provenances = BTreeMap::<ProvenanceId, Provenance>::new();
        let mut edges = BTreeMap::<EdgeId, Edge>::new();

        let mut output_claims = output.claims;
        output_claims.sort_by_key(|claim| {
            (
                claim.subject.to_lowercase(),
                claim.relation.to_lowercase(),
                claim.object.to_lowercase(),
            )
        });
        for mut claim in output_claims {
            let subject = canonical_label(&claim.subject)?;
            let object = canonical_label(&claim.object)?;
            validate_relation(&claim.relation)?;
            if subject == object || claim.evidence.is_empty() {
                return Err(SemanticError::InvalidClaim);
            }
            let subject_id = semantic_node_id("concept", &subject);
            let object_id = semantic_node_id("concept", &object);
            let relation = claim.relation.to_ascii_lowercase();
            let claim_key = format!("{subject}\0{relation}\0{object}");
            let claim_id = semantic_node_id("claim", &claim_key);
            let claim_name = format!("{subject} —{relation}→ {object}");
            let mut first_location = None;
            let mut first_provenance = None;
            claim
                .evidence
                .sort_by_key(|evidence| (evidence.chunk_content_hash, evidence.quote.clone()));

            for evidence in claim.evidence {
                let candidates = source_chunks
                    .get(&evidence.chunk_content_hash)
                    .ok_or(SemanticError::EvidenceChunkNotFound)?;
                if evidence.quote.is_empty() {
                    return Err(SemanticError::EmptyEvidenceQuote);
                }
                for chunk in candidates {
                    let location = quote_location(chunk, &evidence.quote)?;
                    let provenance = semantic_provenance(
                        &self.identity,
                        &producer_version,
                        &location,
                        &evidence.chunk_content_hash,
                    );
                    first_location.get_or_insert_with(|| location.clone());
                    first_provenance.get_or_insert(provenance.id);
                    provenances.insert(provenance.id, provenance.clone());

                    let support_relation = external_relation("supports");
                    let support = make_edge(
                        chunk.node.id,
                        claim_id,
                        support_relation,
                        provenance.id,
                        &[
                            b"semantic-support",
                            &chunk.node.id.0.0,
                            &claim_id.0.0,
                            &location.span.start_byte.to_le_bytes(),
                            &location.span.end_byte.to_le_bytes(),
                        ],
                    );
                    edges.insert(support.id, support);
                }
            }

            let location = first_location.ok_or(SemanticError::MissingEvidence)?;
            let provenance_id = first_provenance.ok_or(SemanticError::MissingEvidence)?;
            let claim_node = make_node(
                claim_id,
                "claim",
                claim_name,
                location.clone(),
                provenance_id,
            );
            claims.insert(claim_key, claim_node);
            concepts.entry(subject.clone()).or_insert_with(|| {
                make_node(
                    subject_id,
                    "concept",
                    subject.clone(),
                    location.clone(),
                    provenance_id,
                )
            });
            concepts.entry(object.clone()).or_insert_with(|| {
                make_node(
                    object_id,
                    "concept",
                    object.clone(),
                    location.clone(),
                    provenance_id,
                )
            });
            for (role, concept_id) in [("subject", subject_id), ("object", object_id)] {
                let edge = make_edge(
                    claim_id,
                    concept_id,
                    external_relation(role),
                    provenance_id,
                    &[b"semantic-claim-role", &claim_id.0.0, role.as_bytes()],
                );
                edges.insert(edge.id, edge);
            }
        }

        let mut nodes = concepts.into_values().collect::<Vec<_>>();
        nodes.extend(claims.into_values());
        Ok(FactBatch {
            manifest: ExtensionManifest {
                schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
                namespace: SEMANTIC_NAMESPACE.to_owned(),
                producer_version,
                capabilities: vec![Capability::AnalysisFacts],
            },
            provenance: provenances.into_values().collect(),
            nodes,
            edges: edges.into_values().collect(),
            observations: Vec::new(),
        })
    }
}

/// Structured errors from semantic request validation and fact construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticError {
    InvalidProviderIdentity,
    EmptyDocumentInput,
    InvalidRequestFingerprint,
    MixedProviderIdentities,
    NotDocumentChunk,
    MissingSourceLocation,
    InvalidDocumentChunk,
    InvalidConceptLabel,
    InvalidRelationLabel,
    InvalidClaim,
    EvidenceChunkNotFound,
    EmptyEvidenceQuote,
    EvidenceQuoteNotFound,
    InvalidEvidenceSpan,
    MissingEvidence,
}

fn normalized_text_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SemanticError {}

fn input_hash(chunks: &[SemanticDocumentChunk]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"syntaxmesh-semantic-input-v1\0");
    for chunk in chunks {
        hasher.update(&chunk.content_hash());
    }
    *hasher.finalize().as_bytes()
}

fn update_hash_component(hasher: &mut blake3::Hasher, component: &[u8]) {
    hasher.update(
        &u64::try_from(component.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    hasher.update(component);
}

fn canonical_label(label: &str) -> Result<String, SemanticError> {
    let normalized = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(SemanticError::InvalidConceptLabel);
    }
    Ok(normalized.to_lowercase())
}

fn validate_relation(relation: &str) -> Result<(), SemanticError> {
    let mut chars = relation.chars();
    if !matches!(chars.next(), Some('a'..='z'))
        || !chars.all(|character| character.is_ascii_lowercase() || character == '_')
    {
        return Err(SemanticError::InvalidRelationLabel);
    }
    Ok(())
}

fn semantic_node_id(kind: &str, canonical_name: &str) -> NodeId {
    NodeId::derive(&[
        SEMANTIC_NAMESPACE.as_bytes(),
        kind.as_bytes(),
        canonical_name.as_bytes(),
    ])
}

fn external_relation(relation: &str) -> RelationKind {
    RelationKind::External {
        namespace: SEMANTIC_NAMESPACE.to_owned(),
        relation: relation.to_owned(),
    }
}

fn semantic_provenance(
    identity: &SemanticProviderIdentity,
    producer_version: &str,
    source: &SourceLocation,
    content_hash: &[u8; 32],
) -> Provenance {
    let start = source.span.start_byte.to_le_bytes();
    let end = source.span.end_byte.to_le_bytes();
    Provenance {
        id: ProvenanceId::derive(&[
            SEMANTIC_NAMESPACE.as_bytes(),
            producer_version.as_bytes(),
            &identity.prompt_hash,
            &identity.configuration_hash,
            &source.file_id.0.0,
            &source.content_hash,
            &start,
            &end,
            content_hash,
        ]),
        producer_namespace: SEMANTIC_NAMESPACE.to_owned(),
        producer_version: producer_version.to_owned(),
        evidence_class: EvidenceClass::SemanticInference,
        source: Some(source.clone()),
    }
}

fn quote_location(
    chunk: &SemanticDocumentChunk,
    quote: &str,
) -> Result<SourceLocation, SemanticError> {
    let start_in_chunk = chunk
        .text
        .find(quote)
        .ok_or(SemanticError::EvidenceQuoteNotFound)?;
    let start_in_source = chunk
        .node
        .source
        .as_ref()
        .ok_or(SemanticError::MissingSourceLocation)?
        .span
        .start_byte
        .checked_add(
            u64::try_from(start_in_chunk).map_err(|_error| SemanticError::InvalidEvidenceSpan)?,
        )
        .ok_or(SemanticError::InvalidEvidenceSpan)?;
    let end_in_source = start_in_source
        .checked_add(
            u64::try_from(quote.len()).map_err(|_error| SemanticError::InvalidEvidenceSpan)?,
        )
        .ok_or(SemanticError::InvalidEvidenceSpan)?;
    let chunk_source = chunk
        .node
        .source
        .as_ref()
        .ok_or(SemanticError::MissingSourceLocation)?;
    let span = SourceSpan::new(start_in_source, end_in_source)
        .map_err(|_error| SemanticError::InvalidEvidenceSpan)?;
    if span.end_byte > chunk_source.span.end_byte {
        return Err(SemanticError::InvalidEvidenceSpan);
    }
    Ok(SourceLocation {
        file_id: chunk_source.file_id,
        content_hash: chunk_source.content_hash,
        span,
    })
}

fn make_node(
    id: NodeId,
    kind: &str,
    name: String,
    source: SourceLocation,
    provenance: ProvenanceId,
) -> Node {
    Node {
        id,
        kind: NodeKind::External {
            namespace: SEMANTIC_NAMESPACE.to_owned(),
            kind: kind.to_owned(),
        },
        name,
        owner_file: Some(source.file_id),
        source: Some(source),
        provenance,
        extension_payload: None,
    }
}

fn make_edge(
    source: NodeId,
    target: NodeId,
    relation: RelationKind,
    provenance: ProvenanceId,
    identity_parts: &[&[u8]],
) -> Edge {
    let mut parts = vec![
        SEMANTIC_NAMESPACE.as_bytes(),
        &source.0.0,
        &target.0.0,
        &provenance.0.0,
    ];
    parts.extend_from_slice(identity_parts);
    Edge {
        id: EdgeId::derive(&parts),
        source,
        target,
        relation,
        provenance,
        extension_payload: None,
    }
}

fn hex_hash(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests;
