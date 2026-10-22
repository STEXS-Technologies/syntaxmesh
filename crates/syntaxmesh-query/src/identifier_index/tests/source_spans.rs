use crate::{
    ContextSourceProvider, GenerationIdentifierIndex, LexicalCandidateRequest, LexicalChannel,
    QueryError, SourceSpanIndexLimits,
};
use syntaxmesh_core::{FileVersion, GenerationId, GraphDelta, NodeKind};
use syntaxmesh_store::{GraphStore, InMemoryGraphStore};

struct FixtureSource(Vec<u8>);

impl ContextSourceProvider for FixtureSource {
    fn read_source(&self, _file: &FileVersion) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(self.0.clone()))
    }
}

fn prepared() -> Result<(InMemoryGraphStore, GenerationId, FixtureSource), QueryError> {
    let (_, mut delta) = super::path_fixture()?;
    let body = "fn load() { audit_database_connection(); } // π"
        .as_bytes()
        .to_vec();
    let hash = *blake3::hash(&body).as_bytes();
    let file = delta
        .changed_files
        .first_mut()
        .ok_or(QueryError::InvalidLimit)?;
    file.content_hash = hash;
    file.size_bytes =
        u64::try_from(body.len()).map_err(|error| QueryError::Context(error.to_string()))?;
    let node = delta
        .upsert_nodes
        .first_mut()
        .ok_or(QueryError::InvalidLimit)?;
    node.kind = NodeKind::Function;
    node.source
        .as_mut()
        .ok_or(QueryError::InvalidLimit)?
        .content_hash = hash;
    node.source
        .as_mut()
        .ok_or(QueryError::InvalidLimit)?
        .span
        .end_byte =
        u64::try_from(body.len()).map_err(|error| QueryError::Context(error.to_string()))?;
    let generation = delta.next_generation;
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(delta)?;
    Ok((store, generation, FixtureSource(body)))
}

fn build(
    store: &InMemoryGraphStore,
    generation: GenerationId,
    source: &FixtureSource,
    span_budget: usize,
    byte_budget: usize,
    posting_budget: usize,
    payload_budget: usize,
) -> Result<GenerationIdentifierIndex, QueryError> {
    GenerationIdentifierIndex::build_source_span_planner(
        store,
        generation,
        source,
        SourceSpanIndexLimits {
            node_budget: 10,
            posting_budget: 100,
            payload_budget: 10_000,
            source_span_budget: span_budget,
            source_byte_budget: byte_budget,
            source_posting_budget: posting_budget,
            source_payload_budget: payload_budget,
        },
    )
}

#[test]
fn source_span_terms_are_available_to_exact_and_stemmed_planner_channels() -> Result<(), QueryError>
{
    let (store, generation, source) = prepared()?;
    let index = build(&store, generation, &source, 1, 1024, 100, 10_000)?;
    for (query, channel) in [
        ("database", LexicalChannel::Exact),
        ("databases", LexicalChannel::Stemmed),
    ] {
        let candidates = index.lexical_candidates(
            &store,
            generation,
            &LexicalCandidateRequest {
                query,
                channel,
                family: None,
                eligible: None,
                posting_budget: 100,
                limit: 10,
            },
        )?;
        if candidates.len() != 1
            || candidates
                .first()
                .is_none_or(|candidate| candidate.node.name != "load")
        {
            return Err(QueryError::Context(
                "source-span text did not select its owning fact".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn source_span_builder_fails_closed_on_hash_and_each_source_budget() -> Result<(), QueryError> {
    let (store, generation, source) = prepared()?;
    if build(&store, generation, &source, 0, 1024, 100, 10_000).is_ok()
        || build(&store, generation, &source, 1, 1, 100, 10_000).is_ok()
        || build(&store, generation, &source, 1, 1024, 1, 10_000).is_ok()
        || build(&store, generation, &source, 1, 1024, 100, 1).is_ok()
    {
        return Err(QueryError::Context(
            "source span builder accepted an exhausted budget".to_owned(),
        ));
    }
    let stale = FixtureSource(b"fn load() { altered(); }".to_vec());
    if build(&store, generation, &stale, 1, 1024, 100, 10_000).is_ok() {
        return Err(QueryError::Context(
            "source span builder accepted stale source bytes".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(feature = "benchmark-instrumentation")]
#[test]
fn source_span_build_metrics_include_text_work() -> Result<(), QueryError> {
    let (store, generation, source) = prepared()?;
    let index = build(&store, generation, &source, 1, 1024, 100, 10_000)?;
    let metrics = index.build_metrics();
    if metrics.node_scans != 3
        || metrics.source_spans != 1
        || metrics.source_files != 1
        || metrics.source_bytes != source.0.len()
        || metrics.postings <= 5
        || metrics.payload_bytes <= 5 * std::mem::size_of::<syntaxmesh_core::NodeId>()
    {
        return Err(QueryError::Context(
            "source-span build metrics omitted admitted text work".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn source_span_builder_rejects_non_boundary_utf8_slices() -> Result<(), QueryError> {
    let (mut store, generation, source) = prepared()?;
    let snapshot = store.historical_snapshot(generation)?;
    let manifest = store.manifest(generation)?;
    let mut delta = GraphDelta {
        repository: manifest.repository,
        worktree: manifest.worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"invalid-span-run"]),
        expected_base: Some(generation),
        next_generation: GenerationId::derive(&[b"invalid-span-generation"]),
        changed_files: snapshot.files,
        removed_files: Vec::new(),
        upsert_provenance: snapshot.provenance,
        upsert_nodes: snapshot.nodes,
        upsert_edges: snapshot.edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let node = delta
        .upsert_nodes
        .first_mut()
        .ok_or(QueryError::InvalidLimit)?;
    node.source
        .as_mut()
        .ok_or(QueryError::InvalidLimit)?
        .span
        .start_byte = u64::try_from(source.0.len().saturating_sub(1))
        .map_err(|error| QueryError::Context(error.to_string()))?;
    let next = delta.next_generation;
    store.apply_delta(delta)?;
    if build(&store, next, &source, 10, 1024, 100, 10_000).is_ok() {
        return Err(QueryError::Context(
            "source span builder accepted a non-boundary UTF-8 slice".to_owned(),
        ));
    }
    Ok(())
}
