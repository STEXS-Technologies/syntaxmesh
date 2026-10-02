//! Generation-scoped graph queries over the store port.

mod lexical_plans;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Write};

use crate::sizing::{
    ExportOutputSizer, HistoricalNeighborhoodLimits, HistoricalNeighborhoodOutputSizer,
};
use syntaxmesh_api_model::{
    ACCEPTANCE_TIMELINE_SCHEMA_VERSION, CONSEQUENCE_TRACE_SCHEMA_VERSION, ContextPack,
    ContextRequest, GRAPH_EXPORT_SCHEMA_VERSION, GraphRecord,
    HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION, HistoricalNeighborCursor,
    TEMPORAL_EXPORT_SCHEMA_VERSION, TemporalQueryMode, TemporalRecord,
};
use syntaxmesh_core::{
    AcceptanceTime, AcceptedGenerationCursor, ChangeEventCorrelation, ChangeEventCorrelationCursor,
    ChangeEventCursor, ChangeSetId, ConsequenceEdge, ConsequenceEdgeCursor, ConsequenceEdgeId,
    ConsequenceEdgeVersion, ConsequenceKind, ConsequenceRangeCursor, Edge, EdgeDirection,
    EventsForChangeSetCursor, EvidenceClass, FactRef, FactVersionRef, GenerationId, GraphSnapshot,
    LineageEndpoint, Node, NodeId, ObservationTime, ObservedFactCursor, ProvenanceId,
};
use syntaxmesh_store::{
    ConsequenceEdgePage, ConsequenceRangePage, FactHistoryVersion, GenerationChange, GraphStore,
    HistoricalEdgePage, MAX_HISTORICAL_EDGE_PAGE_SIZE, NodeHistoryVersion, StoreError,
};

#[derive(Debug)]
pub enum QueryError {
    Store(StoreError),
    InvalidLimit,
    InvalidContextRequest,
    Context(String),
    ContextBudgetTooSmall {
        minimum_tokens: u64,
        requested_tokens: u64,
    },
    OutputBudgetTooSmall {
        requested_bytes: usize,
    },
    Serialization(String),
    UnknownSeed(NodeId),
    InvalidCorrelationAnchor,
    InvalidHistoricalNeighborCursor,
    UnknownAcceptanceHistory(GenerationId),
    GenerationNotKnownBy {
        generation: GenerationId,
        accepted_by: AcceptanceTime,
        accepted_through: AcceptanceTime,
    },
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for QueryError {}

/// One consequence edge reached at its shortest BFS depth from any seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsequenceTraversalEdge {
    pub depth: usize,
    pub edge: ConsequenceEdge,
}

/// Deterministic, bounded, weakly connected consequence neighborhood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsequenceNeighborhood {
    /// All selected seed and reached endpoints in stable identity order.
    pub endpoints: Vec<LineageEndpoint>,
    /// Edges in deterministic breadth-first discovery order.
    pub edges: Vec<ConsequenceTraversalEdge>,
    /// Incident consequence-edge entries examined, including duplicate edges
    /// encountered from the opposite endpoint.
    pub scanned_incidences: usize,
    /// True when endpoint, edge, or scan limits omitted eligible work.
    pub truncated: bool,
}

/// One endpoint state reachable at a specific time and hop depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConsequenceTraceState {
    pub endpoint: LineageEndpoint,
    pub reached_generation: GenerationId,
    pub depth: usize,
}

/// A consequence assertion traversed at a generation when the assertion was
/// active. Its full version retains evidence, derivation, and provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsequenceTraceHop {
    pub depth: usize,
    pub source_reached_generation: GenerationId,
    pub reached_generation: GenerationId,
    pub version: ConsequenceEdgeVersion,
}

/// Bounded temporal evidence DAG across an explicit inclusive generation
/// range. Distinct endpoint/time/depth states are retained; this does not infer
/// causation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsequenceTrace {
    pub origin: LineageEndpoint,
    pub from_generation: GenerationId,
    pub until_generation: GenerationId,
    pub states: Vec<ConsequenceTraceState>,
    pub hops: Vec<ConsequenceTraceHop>,
    pub scanned_incidences: usize,
    pub truncated: bool,
    /// Read-work counters emitted only by benchmark-instrumented builds.
    #[cfg(feature = "benchmark-instrumentation")]
    pub read_metrics: ConsequenceTraceReadMetrics,
}

/// Query-level counters for composed temporal consequence-trace benchmarks.
#[cfg(feature = "benchmark-instrumentation")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsequenceTraceReadMetrics {
    /// Store lookups for distinct generation IDs; query-local cache hits are excluded.
    pub generation_sequence_lookups: usize,
    /// Temporal endpoint pages requested from the store.
    pub endpoint_range_pages: usize,
    /// Calls resolving producer provenance for evidence-class filtering.
    pub provenance_lookup_calls: usize,
    /// Provenance IDs passed to those keyed lookups.
    pub provenance_ids_requested: usize,
    /// Storage work reported by all endpoint pages.
    pub range_page_reads: syntaxmesh_store::ConsequenceRangeReadMetrics,
}

/// Explicit temporal and work bounds for a multi-generation consequence trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsequenceTraceRequest {
    pub origin: LineageEndpoint,
    pub from_generation: GenerationId,
    pub until_generation: GenerationId,
    pub max_hops: usize,
    pub max_endpoints: usize,
    pub max_edges: usize,
    pub max_scanned_incidences: usize,
    /// Empty means include all relation classes; otherwise this is an exact set.
    pub included_kinds: Vec<ConsequenceKind>,
    /// Empty means include all assertion-producer evidence classes. This is
    /// categorical filtering, not a minimum trust or certainty threshold.
    pub included_evidence_classes: Vec<EvidenceClass>,
}

/// Maximum breadth of one fixed-generation consequence traversal.
pub const MAX_CONSEQUENCE_NEIGHBORHOOD_HOPS: usize = 64;
/// Maximum unique endpoints or consequence edges returned by one traversal.
pub const MAX_CONSEQUENCE_NEIGHBORHOOD_ITEMS: usize = 10_000;
/// Maximum incident consequence-edge entries examined by one traversal.
pub const MAX_CONSEQUENCE_NEIGHBORHOOD_SCANS: usize = 100_000;

/// One historical edge and the node at its opposite end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalNeighbor {
    pub edge: Edge,
    pub neighbor: Node,
}

/// Bounded page from one exact generation and endpoint direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalNeighborPage {
    pub generation: GenerationId,
    pub endpoint: NodeId,
    pub direction: EdgeDirection,
    pub items: Vec<HistoricalNeighbor>,
    pub has_more: bool,
    pub next_cursor: Option<HistoricalNeighborCursor>,
}

/// One historical edge in a bounded weakly connected BFS result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalTraversalEdge {
    /// Shortest discovered hop count from any seed.
    pub depth: usize,
    pub edge: Edge,
}

/// Selected node and its shortest discovered BFS depth (seeds have depth 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoricalTraversalNode {
    pub depth: usize,
    pub id: NodeId,
}

/// Deterministic neighborhood from one exact retained graph generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalNeighborhood {
    /// Selected nodes, ordered by stable identity.
    pub nodes: Vec<HistoricalTraversalNode>,
    /// Edges in deterministic BFS discovery order, retaining source/target direction.
    pub edges: Vec<HistoricalTraversalEdge>,
    /// Incident edge entries examined, including duplicate edges seen from
    /// the opposite endpoint.
    pub scanned_incidence_entries: usize,
    /// Sum of selected item sizes from the output sizer; the default export
    /// path counts NDJSON lines, including newlines.
    pub serialized_item_bytes: usize,
    /// True when a scan, node, or edge cap omitted eligible work or output.
    pub truncated: bool,
}

/// Maximum accepted breadth for one exact-generation historical traversal.
pub const MAX_HISTORICAL_NEIGHBORHOOD_HOPS: usize = 64;
/// Maximum number of selected nodes or edges returned by one traversal.
pub const MAX_HISTORICAL_NEIGHBORHOOD_ITEMS: usize = 10_000;
/// Maximum incident edge entries examined by one traversal.
pub const MAX_HISTORICAL_NEIGHBORHOOD_SCANS: usize = 100_000;
/// Maximum serialized historical-neighborhood result size, matching the
/// bounded-result scale used by Shardline query paths.
pub const MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES: usize = 16 * 1024 * 1024;

struct LimitedByteCounter {
    bytes_written: usize,
    limit: usize,
    exceeded: bool,
}

impl Write for LimitedByteCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.bytes_written);
        if buffer.len() > remaining {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "serialized record exceeds the remaining result-byte budget",
            ));
        }
        self.bytes_written = self.bytes_written.saturating_add(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn serialized_line_size(
    record: &TemporalRecord,
    max_line_bytes: usize,
) -> Result<Option<usize>, QueryError> {
    let Some(body_limit) = max_line_bytes.checked_sub(1) else {
        return Ok(None);
    };
    let mut counter = LimitedByteCounter {
        bytes_written: 0,
        limit: body_limit,
        exceeded: false,
    };
    match serde_json::to_writer(&mut counter, record) {
        Ok(()) => Ok(Some(counter.bytes_written.saturating_add(1))),
        Err(_error) if counter.exceeded => Ok(None),
        Err(error) => Err(QueryError::Serialization(error.to_string())),
    }
}

impl From<StoreError> for QueryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

pub struct Query<'store, S: ?Sized> {
    store: &'store S,
    generation: GenerationId,
}

impl<'store, S> Query<'store, S>
where
    S: GraphStore + ?Sized,
{
    fn trace_generation_sequence(
        &self,
        generation: GenerationId,
        cache: &mut BTreeMap<GenerationId, u64>,
        #[cfg(feature = "benchmark-instrumentation")]
        read_metrics: &mut ConsequenceTraceReadMetrics,
    ) -> Result<u64, QueryError> {
        if let Some(sequence) = cache.get(&generation) {
            return Ok(*sequence);
        }
        let sequence = self.store.generation_sequence(generation)?;
        cache.insert(generation, sequence);
        #[cfg(feature = "benchmark-instrumentation")]
        {
            read_metrics.generation_sequence_lookups =
                read_metrics.generation_sequence_lookups.saturating_add(1);
        }
        Ok(sequence)
    }

    #[must_use]
    pub const fn new(store: &'store S, generation: GenerationId) -> Self {
        Self { store, generation }
    }

    /// Return the generation this query is pinned to.
    #[must_use]
    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    /// Analyze cyclic components in this retained generation's complete graph.
    /// Reuses the projection and exact optional relation filter; materializes
    /// the full graph rather than a bounded neighborhood.
    ///
    /// # Errors
    /// Rejects unavailable/corrupt history and selected dangling endpoints.
    pub fn cyclic_components(
        &self,
        relation: Option<&syntaxmesh_core::RelationKind>,
    ) -> Result<Vec<Vec<NodeId>>, QueryError> {
        Ok(
            syntaxmesh_graph::GenerationGraph::load_retained(self.store, self.generation)?
                .cyclic_components(relation)?,
        )
    }

    /// Read the pinned generation manifest.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn manifest(&self) -> Result<syntaxmesh_core::GenerationManifest, QueryError> {
        Ok(self.store.manifest(self.generation)?)
    }

    /// Read canonical file identities for this pinned generation.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn files(&self) -> Result<Vec<syntaxmesh_core::FileVersion>, QueryError> {
        Ok(self.store.files(self.generation)?)
    }

    /// Read one stable-ID-ordered inventory page from this retained generation.
    ///
    /// # Errors
    /// Rejects invalid limits and unavailable/corrupt retained generations.
    pub fn historical_files_page(
        &self,
        after: Option<syntaxmesh_core::FileId>,
        limit: usize,
    ) -> Result<syntaxmesh_store::HistoricalFilePage, QueryError> {
        Ok(self
            .store
            .historical_files_page(self.generation, after, limit)?)
    }

    /// Read reference ranked candidates pinned to this query's generation.
    ///
    /// # Errors
    /// Returns invalid-limit, incomplete-statistics, or retained-store errors.
    pub fn reference_ranked_candidates(
        &self,
        query: &str,
        scan_budget: usize,
        limit: usize,
    ) -> Result<Vec<crate::RankedCandidate>, QueryError> {
        crate::reference_ranked_candidates(self.store, self.generation, query, scan_budget, limit)
    }

    /// Build a reusable identifier channel for this query's retained generation.
    /// Payload bytes count term UTF-8 bytes and retained node IDs, not RSS.
    ///
    /// # Errors
    /// Rejects exhausted budgets, unavailable generations and malformed pages.
    pub fn build_identifier_index(
        &self,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
    ) -> Result<crate::GenerationIdentifierIndex, QueryError> {
        crate::GenerationIdentifierIndex::build(
            self.store,
            self.generation,
            node_budget,
            posting_budget,
            payload_budget,
        )
    }

    /// Build an opt-in label/path channel from this retained generation.
    /// Inventory and posting payload budgets are separate logical byte bounds.
    ///
    /// # Errors
    /// Rejects exhausted budgets, malformed pages and missing source files.
    pub fn build_path_identifier_index(
        &self,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
        file_budget: usize,
        inventory_payload_budget: usize,
    ) -> Result<crate::GenerationIdentifierIndex, QueryError> {
        crate::GenerationIdentifierIndex::build_with_paths(
            self.store,
            self.generation,
            node_budget,
            posting_budget,
            payload_budget,
            file_budget,
            inventory_payload_budget,
        )
    }

    /// Read the explicit identifier channel without changing canonical search.
    ///
    /// # Errors
    /// Rejects foreign indexes, exhausted posting budgets and invalid limits.
    pub fn identifier_candidates(
        &self,
        index: &crate::GenerationIdentifierIndex,
        query: &str,
        posting_budget: usize,
        limit: usize,
    ) -> Result<Vec<crate::RankedCandidate>, QueryError> {
        index.candidates(self.store, self.generation, query, posting_budget, limit)
    }

    /// Return outgoing edges without materializing their targets.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn neighbor_edges(&self, id: NodeId) -> Result<Vec<Edge>, QueryError> {
        Ok(self.store.outgoing(self.generation, id)?)
    }

    /// Return incoming edges without materializing their sources.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn incoming_edges(&self, id: NodeId) -> Result<Vec<Edge>, QueryError> {
        Ok(self.store.incoming(self.generation, id)?)
    }

    /// Inspect bounded current selection without source or token work.
    ///
    /// # Errors
    /// Rejects invalid requests, unknown seeds and unavailable/corrupt generations.
    pub fn context_selection(
        &self,
        request: &syntaxmesh_api_model::ContextRequest,
    ) -> Result<crate::ContextSelectionReport, QueryError> {
        crate::context::selection_report(
            self,
            request,
            false,
            crate::context::SeedMode::LexicalAndExplicit,
        )
    }

    /// Inspect bounded retained-generation selection without source or token work.
    ///
    /// # Errors
    /// Rejects invalid requests, unknown seeds and unavailable/corrupt generations.
    pub fn historical_context_selection(
        &self,
        request: &syntaxmesh_api_model::ContextRequest,
    ) -> Result<crate::ContextSelectionReport, QueryError> {
        crate::context::selection_report(
            self,
            request,
            true,
            crate::context::SeedMode::LexicalAndExplicit,
        )
    }

    /// Inspect explicit seeds and their bounded graph expansion without lexical lookup.
    ///
    /// # Errors
    /// Rejects missing/excess seeds, invalid requests and unavailable/corrupt generations.
    pub fn seeded_context_selection(
        &self,
        request: &ContextRequest,
    ) -> Result<crate::ContextSelectionReport, QueryError> {
        crate::context::selection_report(
            self,
            request,
            false,
            crate::context::SeedMode::ExplicitOnly,
        )
    }

    /// Inspect explicit-seed expansion in this retained generation.
    ///
    /// # Errors
    /// Rejects missing/excess seeds, invalid requests and unavailable/corrupt generations.
    pub fn historical_seeded_context_selection(
        &self,
        request: &ContextRequest,
    ) -> Result<crate::ContextSelectionReport, QueryError> {
        crate::context::selection_report(
            self,
            request,
            true,
            crate::context::SeedMode::ExplicitOnly,
        )
    }

    /// Compile from explicit seeds only, preserving the original query as metadata.
    /// Does not merge additional lexical candidates into the caller's plan.
    ///
    /// # Errors
    /// Rejects missing/excess seeds and returns the ordinary compiler's request,
    /// store, source-provider, tokenizer and serialized-budget errors.
    pub fn seeded_context(
        &self,
        request: &ContextRequest,
        source_provider: &impl crate::ContextSourceProvider,
        token_counter: &impl crate::ContextTokenCounter,
    ) -> Result<ContextPack, QueryError> {
        crate::context::compile_seeded_context(
            self,
            request,
            source_provider,
            token_counter,
            false,
            crate::context::SeedMode::ExplicitOnly,
        )
    }

    /// Compile from explicit seeds in this retained generation, preserving query intent.
    /// Source bytes must match the historical file hash.
    ///
    /// # Errors
    /// Rejects missing/excess seeds and returns the ordinary historical compiler's
    /// request, store, source-provider, tokenizer and serialized-budget errors.
    pub fn historical_seeded_context(
        &self,
        request: &ContextRequest,
        source_provider: &impl crate::ContextSourceProvider,
        token_counter: &impl crate::ContextTokenCounter,
    ) -> Result<ContextPack, QueryError> {
        crate::context::compile_seeded_context(
            self,
            request,
            source_provider,
            token_counter,
            true,
            crate::context::SeedMode::ExplicitOnly,
        )
    }

    /// Compile a deterministic, exact-token-budget context pack for this generation.
    ///
    /// # Errors
    /// Returns request, seed, source-provider, tokenizer or serialized-budget errors.
    pub fn context(
        &self,
        request: &ContextRequest,
        source_provider: &impl crate::ContextSourceProvider,
        token_counter: &impl crate::ContextTokenCounter,
    ) -> Result<ContextPack, QueryError> {
        crate::context::compile_context(self, request, source_provider, token_counter)
    }

    /// Compile context from this retained generation using temporal reads.
    /// Source bytes are host-supplied and must match the historical file hash.
    ///
    /// # Errors
    /// Returns request, seed, retained-store, source-provider, tokenizer, or
    /// serialized-budget errors, just as the ordinary context compiler does.
    pub fn historical_context(
        &self,
        request: &ContextRequest,
        source_provider: &impl crate::ContextSourceProvider,
        token_counter: &impl crate::ContextTokenCounter,
    ) -> Result<ContextPack, QueryError> {
        crate::context::compile_historical_context(self, request, source_provider, token_counter)
    }

    pub(crate) fn context_provenance(
        &self,
        ids: &[ProvenanceId],
    ) -> Result<Vec<syntaxmesh_core::Provenance>, QueryError> {
        Ok(self.store.provenance_for_ids(self.generation, ids)?)
    }

    /// Read one node from the query's pinned generation.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn node(&self, id: NodeId) -> Result<Option<Node>, QueryError> {
        Ok(self.store.node(self.generation, id)?)
    }

    /// Read one node from the generation bound to this query without
    /// materializing the complete historical graph. Durable stores use their
    /// identity/validity index; reference stores may reconstruct the snapshot.
    ///
    /// # Errors
    /// Returns a store error if the bound generation is unavailable or its
    /// retained history cannot be read consistently.
    pub fn historical_node(&self, id: NodeId) -> Result<Option<Node>, QueryError> {
        Ok(self.store.historical_node(self.generation, id)?)
    }

    /// Read one file version from the query's retained generation. Durable
    /// stores use their identity/validity index, without replay or a full graph.
    ///
    /// # Errors
    /// Returns a store error if the bound generation is unavailable or invalid.
    pub fn historical_file(
        &self,
        id: syntaxmesh_core::FileId,
    ) -> Result<Option<syntaxmesh_core::FileVersion>, QueryError> {
        Ok(self.store.historical_file(self.generation, id)?)
    }

    /// Read one provenance version from the query's retained generation.
    ///
    /// # Errors
    /// Returns a store error if the bound generation is unavailable or invalid.
    pub fn historical_provenance(
        &self,
        id: ProvenanceId,
    ) -> Result<Option<syntaxmesh_core::Provenance>, QueryError> {
        Ok(self.store.historical_provenance(self.generation, id)?)
    }

    /// Search node names in this retained generation, ordered by stable ID.
    /// Durable stores scan only the selected generation's node-family range.
    ///
    /// # Errors
    /// Returns a store error if the generation or its retained pages are invalid.
    pub fn historical_search(&self, text: &str, limit: usize) -> Result<Vec<Node>, QueryError> {
        Ok(self
            .store
            .historical_search_nodes(self.generation, text, limit)?)
    }

    /// Return a deterministic page of incident edges without reconstructing
    /// the historical graph. Durable stores seek their generation-scoped
    /// incidence root; the cursor is bound to this query's generation,
    /// endpoint, and direction.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for zero or oversized pages,
    /// [`QueryError::InvalidHistoricalNeighborCursor`] for a cursor bound to a
    /// different query, or a store error for missing/corrupt history.
    pub fn historical_neighbors(
        &self,
        endpoint: NodeId,
        direction: EdgeDirection,
        limit: usize,
        after: Option<HistoricalNeighborCursor>,
    ) -> Result<HistoricalNeighborPage, QueryError> {
        let (stored, next_cursor) = self.historical_edge_page(endpoint, direction, limit, after)?;
        let mut items = Vec::with_capacity(stored.items.len());
        for edge in stored.items {
            let neighbor_id = match direction {
                EdgeDirection::Outgoing => edge.target,
                EdgeDirection::Incoming => edge.source,
            };
            let neighbor = self
                .store
                .historical_node(self.generation, neighbor_id)?
                .ok_or_else(|| {
                    StoreError::Integrity(format!(
                        "historical incidence edge {:?} refers to missing endpoint {neighbor_id:?}",
                        edge.id
                    ))
                })?;
            items.push(HistoricalNeighbor { edge, neighbor });
        }
        Ok(HistoricalNeighborPage {
            generation: self.generation,
            endpoint,
            direction,
            items,
            has_more: stored.has_more,
            next_cursor,
        })
    }

    pub(crate) fn historical_edge_page(
        &self,
        endpoint: NodeId,
        direction: EdgeDirection,
        limit: usize,
        after: Option<HistoricalNeighborCursor>,
    ) -> Result<(HistoricalEdgePage, Option<HistoricalNeighborCursor>), QueryError> {
        if limit == 0 || limit > MAX_HISTORICAL_EDGE_PAGE_SIZE {
            return Err(QueryError::InvalidLimit);
        }
        if after.is_some_and(|cursor| {
            cursor.generation != self.generation
                || cursor.endpoint != endpoint
                || cursor.direction != direction
        }) {
            return Err(QueryError::InvalidHistoricalNeighborCursor);
        }
        let stored = self.store.historical_incident_edges(
            self.generation,
            endpoint,
            direction,
            after.map(|cursor| cursor.after_edge),
            limit,
        )?;
        let next_cursor = if stored.has_more {
            let last_edge = stored
                .items
                .last()
                .ok_or_else(|| {
                    StoreError::Integrity(
                        "historical edge page reports more items but is empty".to_owned(),
                    )
                })?
                .id;
            Some(HistoricalNeighborCursor {
                generation: self.generation,
                endpoint,
                direction,
                after_edge: last_edge,
            })
        } else {
            None
        };
        Ok((stored, next_cursor))
    }

    /// Traverse a bounded, weakly connected neighborhood in this query's
    /// exact historical generation. Durable stores use the generation-scoped
    /// incidence index; this operation never replays generation deltas or
    /// materializes the complete graph.
    ///
    /// Edges retain their stored direction. The `max_scanned_edges` limit
    /// counts incident entries examined, including an edge seen from both
    /// endpoints; this explicit work bound prevents high-degree and cyclic
    /// neighborhoods from turning into unbounded requests.
    /// Reaching `max_hops` alone is intentional and does not set `truncated`.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for empty seeds, zero/oversized
    /// bounds, or more unique seeds than `max_nodes`; returns
    /// [`QueryError::UnknownSeed`] if a seed is absent at this generation.
    pub fn historical_neighborhood(
        &self,
        seeds: &[NodeId],
        max_hops: usize,
        max_nodes: usize,
        max_edges: usize,
        max_scanned_edges: usize,
        max_result_bytes: usize,
    ) -> Result<HistoricalNeighborhood, QueryError> {
        self.historical_neighborhood_with_output(
            seeds,
            HistoricalNeighborhoodLimits {
                max_hops,
                max_nodes,
                max_edges,
                max_scanned_edges,
                max_result_bytes,
            },
            &ExportOutputSizer,
        )
    }

    /// Traverse with the existing indexed BFS and caller-specific output accounting.
    ///
    /// # Errors
    /// Returns invalid-limit, unknown-seed, output-budget, store, or sizer errors.
    pub fn historical_neighborhood_with_output(
        &self,
        seeds: &[NodeId],
        limits: HistoricalNeighborhoodLimits,
        output: &impl HistoricalNeighborhoodOutputSizer,
    ) -> Result<HistoricalNeighborhood, QueryError> {
        let HistoricalNeighborhoodLimits {
            max_hops,
            max_nodes,
            max_edges,
            max_scanned_edges,
            max_result_bytes,
        } = limits;
        if seeds.is_empty()
            || max_hops == 0
            || max_hops > MAX_HISTORICAL_NEIGHBORHOOD_HOPS
            || max_nodes == 0
            || max_nodes > MAX_HISTORICAL_NEIGHBORHOOD_ITEMS
            || max_edges == 0
            || max_edges > MAX_HISTORICAL_NEIGHBORHOOD_ITEMS
            || max_scanned_edges == 0
            || max_scanned_edges > MAX_HISTORICAL_NEIGHBORHOOD_SCANS
            || max_result_bytes == 0
            || max_result_bytes > MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES
        {
            return Err(QueryError::InvalidLimit);
        }
        let mut selected_nodes = seeds.iter().copied().collect::<BTreeSet<_>>();
        if selected_nodes.len() > max_nodes {
            return Err(QueryError::InvalidLimit);
        }

        let footer_bytes = output.metadata(self.generation, max_result_bytes)?.ok_or(
            QueryError::OutputBudgetTooSmall {
                requested_bytes: max_result_bytes,
            },
        )?;
        let item_budget = max_result_bytes.saturating_sub(footer_bytes);
        let mut serialized_item_bytes = 0_usize;
        for seed in &selected_nodes {
            let node = self
                .store
                .historical_node(self.generation, *seed)?
                .ok_or(QueryError::UnknownSeed(*seed))?;
            let remaining = item_budget.saturating_sub(serialized_item_bytes);
            let node_bytes = output.node(self.generation, 0, node, remaining)?.ok_or(
                QueryError::OutputBudgetTooSmall {
                    requested_bytes: max_result_bytes,
                },
            )?;
            serialized_item_bytes = serialized_item_bytes.saturating_add(node_bytes);
        }

        let mut frontier = selected_nodes.clone();
        let mut node_depths = selected_nodes
            .iter()
            .copied()
            .map(|id| (id, 0))
            .collect::<BTreeMap<_, _>>();
        let mut selected_edges = BTreeSet::new();
        let mut edges = Vec::new();
        let mut scanned_edges = 0;
        let mut truncated = false;

        'bfs: for depth in 1..=max_hops {
            if frontier.is_empty() {
                break;
            }
            let mut next_frontier = BTreeSet::new();
            for endpoint in frontier {
                for direction in [EdgeDirection::Outgoing, EdgeDirection::Incoming] {
                    let mut cursor = None;
                    loop {
                        if scanned_edges == max_scanned_edges {
                            truncated = true;
                            break 'bfs;
                        }
                        let page_limit = max_scanned_edges
                            .saturating_sub(scanned_edges)
                            .saturating_add(1)
                            .min(MAX_HISTORICAL_EDGE_PAGE_SIZE);
                        let page =
                            self.historical_neighbors(endpoint, direction, page_limit, cursor)?;
                        let page_has_more = page.has_more;
                        cursor = page.next_cursor;
                        for item in page.items {
                            if scanned_edges == max_scanned_edges {
                                truncated = true;
                                break 'bfs;
                            }
                            scanned_edges = scanned_edges.saturating_add(1);
                            if selected_edges.contains(&item.edge.id) {
                                continue;
                            }
                            let neighbor = item.neighbor.id;
                            let is_new_node = !selected_nodes.contains(&neighbor);
                            if is_new_node && selected_nodes.len() == max_nodes {
                                truncated = true;
                                continue;
                            }
                            if edges.len() == max_edges {
                                truncated = true;
                                break 'bfs;
                            }
                            let candidate_node_bytes = if is_new_node {
                                output.node(
                                    self.generation,
                                    depth,
                                    item.neighbor.clone(),
                                    item_budget.saturating_sub(serialized_item_bytes),
                                )?
                            } else {
                                Some(0)
                            };
                            let Some(candidate_node_bytes) = candidate_node_bytes else {
                                truncated = true;
                                continue;
                            };
                            let edge_bytes = output.edge(
                                self.generation,
                                depth,
                                item.edge.clone(),
                                item_budget
                                    .saturating_sub(serialized_item_bytes)
                                    .saturating_sub(candidate_node_bytes),
                            )?;
                            let Some(edge_bytes) = edge_bytes else {
                                truncated = true;
                                continue;
                            };
                            serialized_item_bytes = serialized_item_bytes
                                .saturating_add(candidate_node_bytes)
                                .saturating_add(edge_bytes);
                            selected_edges.insert(item.edge.id);
                            if is_new_node {
                                selected_nodes.insert(neighbor);
                                node_depths.insert(neighbor, depth);
                                if depth < max_hops {
                                    next_frontier.insert(neighbor);
                                }
                            }
                            edges.push(HistoricalTraversalEdge {
                                depth,
                                edge: item.edge,
                            });
                        }
                        if !page_has_more {
                            break;
                        }
                        if cursor.is_none() {
                            return Err(QueryError::Store(StoreError::Integrity(
                                "historical neighbor page reports more without a cursor".to_owned(),
                            )));
                        }
                    }
                }
            }
            frontier = next_frontier;
        }

        let mut nodes = node_depths
            .into_iter()
            .map(|(id, depth)| HistoricalTraversalNode { depth, id })
            .collect::<Vec<_>>();
        nodes.sort_unstable_by_key(|node| (node.depth, node.id));
        Ok(HistoricalNeighborhood {
            nodes,
            edges,
            scanned_incidence_entries: scanned_edges,
            serialized_item_bytes,
            truncated,
        })
    }

    /// Serialize one bounded historical neighborhood as deterministic
    /// generation-tagged node/edge records followed by a summary footer.
    ///
    /// # Errors
    /// Returns the same validation/seed/store errors as
    /// [`Self::historical_neighborhood`], or an integrity error if a selected
    /// node disappeared while hydrating the pinned generation.
    pub fn historical_neighborhood_records(
        &self,
        seeds: &[NodeId],
        max_hops: usize,
        max_nodes: usize,
        max_edges: usize,
        max_scanned_edges: usize,
        max_result_bytes: usize,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        let neighborhood = self.historical_neighborhood(
            seeds,
            max_hops,
            max_nodes,
            max_edges,
            max_scanned_edges,
            max_result_bytes,
        )?;
        let generation = self.generation;
        let mut records = Vec::with_capacity(
            neighborhood
                .nodes
                .len()
                .saturating_add(neighborhood.edges.len())
                .saturating_add(1),
        );
        for selected in &neighborhood.nodes {
            let node = self
                .store
                .historical_node(generation, selected.id)?
                .ok_or_else(|| {
                    StoreError::Integrity(format!(
                        "historical neighborhood selected missing node {:?}",
                        selected.id
                    ))
                })?;
            records.push(TemporalRecord::HistoricalNeighborhoodNode {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                depth: selected.depth,
                node,
            });
        }
        records.extend(neighborhood.edges.iter().map(|selected| {
            TemporalRecord::HistoricalNeighborhoodEdge {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                depth: selected.depth,
                edge: selected.edge.clone(),
            }
        }));
        records.push(TemporalRecord::HistoricalNeighborhoodFooter {
            schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            generation,
            nodes: u64::try_from(neighborhood.nodes.len()).unwrap_or(u64::MAX),
            edges: u64::try_from(neighborhood.edges.len()).unwrap_or(u64::MAX),
            scanned_incidence_entries: u64::try_from(neighborhood.scanned_incidence_entries)
                .unwrap_or(u64::MAX),
            serialized_item_bytes: u64::try_from(neighborhood.serialized_item_bytes)
                .unwrap_or(u64::MAX),
            truncated: neighborhood.truncated,
        });
        Ok(records)
    }

    /// Serialize one bounded historical-neighbor page as item/footer records.
    ///
    /// # Errors
    /// Returns the same validation and store errors as
    /// [`Self::historical_neighbors`].
    pub fn historical_neighbor_records(
        &self,
        endpoint: NodeId,
        direction: EdgeDirection,
        limit: usize,
        after: Option<HistoricalNeighborCursor>,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        Ok(self
            .historical_neighbors(endpoint, direction, limit, after)?
            .into_records())
    }

    /// Return module-resolution diagnostics accepted in this query's pinned
    /// generation. Diagnostic nodes are retained canonical facts; queries do
    /// not call the configured resolver or inspect a host filesystem.
    ///
    /// # Errors
    /// Returns a store error if the generation or persisted diagnostic facts
    /// cannot be read reliably.
    pub fn module_resolution_diagnostics(&self) -> Result<Vec<Node>, QueryError> {
        Ok(self
            .store
            .module_resolution_nodes(self.generation)?
            .into_iter()
            .filter(|node| {
                matches!(
                    &node.kind,
                    syntaxmesh_core::NodeKind::ModuleResolutionDiagnostic { .. }
                )
            })
            .collect())
    }

    /// Scan at most `scan_limit` retained nodes for resolution diagnostics.
    /// The exclusive seek and continuation refer to scanned nodes, not matches;
    /// an empty match page can therefore still have a continuation.
    ///
    /// # Errors
    /// Rejects invalid limits and unavailable or corrupt retained generations.
    pub fn module_resolution_diagnostic_page(
        &self,
        after: Option<NodeId>,
        scan_limit: usize,
    ) -> Result<crate::ResolutionDiagnosticPage, QueryError> {
        crate::diagnostics::diagnostic_page(
            self.generation,
            self.store
                .historical_nodes_page(self.generation, after, scan_limit)?,
        )
    }

    /// Read globally ordered fact versions through this pinned generation.
    /// Durable adapters reuse their composite identity/sequence history index.
    ///
    /// # Errors
    /// Rejects limits outside 1–1000, foreign-generation cursors, and unavailable
    /// or malformed retained history.
    pub fn fact_history_page(
        &self,
        after: Option<syntaxmesh_store::FactHistoryCursor>,
        limit: usize,
    ) -> Result<syntaxmesh_store::FactHistoryPage, QueryError> {
        if limit == 0 || limit > 1000 {
            return Err(QueryError::InvalidLimit);
        }
        Ok(self
            .store
            .fact_history_page(self.generation, after, limit)?)
    }

    /// Return the retained versions of one node across accepted generations.
    ///
    /// # Errors
    /// Returns a store error when the retained history is unavailable or
    /// malformed.
    pub fn history(&self, id: NodeId) -> Result<Vec<TemporalRecord>, QueryError> {
        Ok(self
            .store
            .node_history(id)?
            .into_iter()
            .map(|version: NodeHistoryVersion| TemporalRecord::NodeVersion {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                valid_from: version.valid_from,
                valid_until: version.valid_until,
                node: version.node,
            })
            .collect())
    }

    /// Return retained typed versions of a canonical graph fact.
    ///
    /// # Errors
    /// Returns a store error when the fact history is unavailable or malformed.
    pub fn fact_history(&self, fact: FactRef) -> Result<Vec<TemporalRecord>, QueryError> {
        Ok(self
            .store
            .fact_history(fact)?
            .into_iter()
            .map(|version: FactHistoryVersion| TemporalRecord::FactVersion {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                valid_from: version.valid_from,
                valid_until: version.valid_until,
                observed_at: version.observed_at,
                accepted_at: version.accepted_at,
                payload: version.payload,
            })
            .collect())
    }

    /// Link consecutive retained versions of one stable fact identity.
    ///
    /// A link is emitted only when the prior version's half-open validity
    /// interval ends at the next version's start. Removal followed by a later
    /// reintroduction is deliberately not represented as supersession.
    /// Durable stores read this from the fact-identity history index; the
    /// result is proportional to the number of returned version links.
    ///
    /// # Errors
    /// Returns a store error when the retained fact history is unavailable or
    /// malformed.
    pub fn fact_lineage(&self, fact: FactRef) -> Result<Vec<TemporalRecord>, QueryError> {
        let versions = self.store.fact_history(fact)?;
        Ok(versions
            .windows(2)
            .filter_map(|pair| {
                let prior = pair.first()?;
                let current = pair.get(1)?;
                (prior.valid_until == Some(current.valid_from)).then_some(
                    TemporalRecord::FactSupersedes {
                        schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                        query_mode: TemporalQueryMode::HistoricalConclusion,
                        prior: FactVersionRef {
                            fact,
                            valid_from: prior.valid_from,
                        },
                        current: FactVersionRef {
                            fact,
                            valid_from: current.valid_from,
                        },
                        accepted_at: current.accepted_at,
                    },
                )
            })
            .collect())
    }

    /// Return a bounded page of accepted changes that directly affected one
    /// fact identity. This relation is transition membership, not causation.
    ///
    /// # Errors
    /// Returns a store error if retained lineage is unavailable or invalid.
    pub fn change_events_for_fact(
        &self,
        fact: FactRef,
        after: Option<ChangeEventCursor>,
        limit: usize,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        let page = self.store.change_events_for_fact(fact, after, limit)?;
        let returned = u64::try_from(page.items.len()).map_err(|error| {
            QueryError::Store(StoreError::Backend(format!(
                "change-event page length is not representable: {error}"
            )))
        })?;
        let has_more = page.next_cursor.is_some();
        let mut records = page
            .items
            .into_iter()
            .map(|event| TemporalRecord::ChangeEvent {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                event,
            })
            .collect::<Vec<_>>();
        records.push(TemporalRecord::ChangeEventFooter {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            returned,
            has_more,
            next_cursor: page.next_cursor,
        });
        Ok(records)
    }

    /// Return a bounded page of events explicitly assigned to a ChangeSet at
    /// this query's pinned generation.
    ///
    /// Membership validity and event time are both evaluated against the same
    /// retained snapshot; this does not infer grouping from adjacent events.
    ///
    /// # Errors
    /// Returns a store error for a stale snapshot, invalid cursor, or unavailable
    /// ChangeSet lineage.
    pub fn events_for_change_set(
        &self,
        change_set: ChangeSetId,
        after: Option<EventsForChangeSetCursor>,
        limit: usize,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        let page = self
            .store
            .events_for_change_set(change_set, self.generation, after, limit)?;
        let returned = u64::try_from(page.items.len()).map_err(|error| {
            QueryError::Store(StoreError::Backend(format!(
                "ChangeSet event page length is not representable: {error}"
            )))
        })?;
        let has_more = page.next_cursor.is_some();
        let mut records = page
            .items
            .into_iter()
            .map(|item| TemporalRecord::ChangeSetEvent {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                item,
            })
            .collect::<Vec<_>>();
        records.push(TemporalRecord::ChangeSetEventsFooter {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            returned,
            has_more,
            next_cursor: page.next_cursor,
        });
        Ok(records)
    }

    /// Read a bounded page of consequence assertions directly incident to an endpoint.
    ///
    /// The query is pinned to this `Query`'s generation. It returns only explicit
    /// assertions and does not infer causality or traverse beyond one edge.
    ///
    /// # Errors
    /// Returns a store error when the generation is stale, the cursor is mismatched,
    /// or the backend cannot query consequence history.
    pub fn consequences_for_endpoint(
        &self,
        endpoint: LineageEndpoint,
        after: Option<ConsequenceEdgeCursor>,
        limit: usize,
    ) -> Result<ConsequenceEdgePage, QueryError> {
        Ok(self
            .store
            .consequence_edges_for_endpoint(endpoint, self.generation, after, limit)?)
    }

    /// Read a bounded page of consequence assertions incident to an endpoint
    /// over an inclusive generation range. Each result carries the assertion's
    /// full validity interval; this page primitive does not infer a path.
    /// Durable stores use endpoint/interval indexes rather than replaying the
    /// generations in the requested range.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for an oversized page and a store
    /// error for unavailable generations, reversed ranges, or mismatched
    /// continuation cursors.
    pub fn consequences_for_endpoint_range(
        &self,
        endpoint: LineageEndpoint,
        from_generation: GenerationId,
        until_generation: GenerationId,
        after: Option<ConsequenceRangeCursor>,
        limit: usize,
    ) -> Result<ConsequenceRangePage, QueryError> {
        if limit > MAX_HISTORICAL_EDGE_PAGE_SIZE {
            return Err(QueryError::InvalidLimit);
        }
        Ok(self.store.consequence_edges_for_endpoint_range(
            endpoint,
            from_generation,
            until_generation,
            after,
            limit,
        )?)
    }

    /// Traverse a bounded, weakly connected consequence neighborhood without
    /// replaying generation deltas. The selected generation is pinned by this
    /// query; returned edges retain their original source/target direction.
    ///
    /// Reuses the deterministic bounded BFS semantics of `export_subgraph`.
    /// The scan budget counts incident entries examined, including duplicates
    /// seen at the opposite endpoint. Endpoint pages are fetched in bounded
    /// batches. Reaching `max_hops` alone is intentional; endpoint, edge, or
    /// scan-cap omissions set `truncated`.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for empty seeds, zero or oversized
    /// limits, or more unique seeds than `max_endpoints`; returns a store error
    /// if indexed endpoint reads are unavailable.
    pub fn consequence_neighborhood(
        &self,
        seeds: &[LineageEndpoint],
        max_hops: usize,
        max_endpoints: usize,
        max_edges: usize,
        max_scanned_incidences: usize,
    ) -> Result<ConsequenceNeighborhood, QueryError> {
        if seeds.is_empty()
            || max_hops == 0
            || max_hops > MAX_CONSEQUENCE_NEIGHBORHOOD_HOPS
            || max_endpoints == 0
            || max_endpoints > MAX_CONSEQUENCE_NEIGHBORHOOD_ITEMS
            || max_edges == 0
            || max_edges > MAX_CONSEQUENCE_NEIGHBORHOOD_ITEMS
            || max_scanned_incidences == 0
            || max_scanned_incidences > MAX_CONSEQUENCE_NEIGHBORHOOD_SCANS
        {
            return Err(QueryError::InvalidLimit);
        }
        let mut endpoints = seeds.iter().copied().collect::<BTreeSet<_>>();
        if endpoints.len() > max_endpoints {
            return Err(QueryError::InvalidLimit);
        }
        let mut frontier = endpoints.clone();
        let mut selected_edges = BTreeSet::new();
        let mut edges = Vec::new();
        let mut scanned_incidences = 0_usize;
        let mut truncated = false;

        'bfs: for depth in 1..=max_hops {
            if frontier.is_empty() {
                break;
            }
            let mut next_frontier = BTreeSet::new();
            for endpoint in frontier {
                let mut cursor = None;
                loop {
                    if scanned_incidences == max_scanned_incidences {
                        truncated = true;
                        break 'bfs;
                    }
                    let page_limit = max_scanned_incidences
                        .saturating_sub(scanned_incidences)
                        .min(MAX_HISTORICAL_EDGE_PAGE_SIZE);
                    let page = self.store.consequence_edges_for_endpoint(
                        endpoint,
                        self.generation,
                        cursor,
                        page_limit,
                    )?;
                    let has_more = page.next_cursor.is_some();
                    cursor = page.next_cursor;
                    for edge in page.items {
                        scanned_incidences = scanned_incidences.saturating_add(1);
                        if selected_edges.contains(&edge.id) {
                            continue;
                        }
                        let neighbor = if edge.source == endpoint {
                            edge.target
                        } else {
                            edge.source
                        };
                        let is_new_endpoint = !endpoints.contains(&neighbor);
                        if is_new_endpoint && endpoints.len() == max_endpoints {
                            truncated = true;
                            continue;
                        }
                        if edges.len() == max_edges {
                            truncated = true;
                            break 'bfs;
                        }
                        selected_edges.insert(edge.id);
                        if is_new_endpoint {
                            endpoints.insert(neighbor);
                            if depth < max_hops {
                                next_frontier.insert(neighbor);
                            }
                        }
                        edges.push(ConsequenceTraversalEdge { depth, edge });
                    }
                    if !has_more {
                        break;
                    }
                    if scanned_incidences == max_scanned_incidences {
                        truncated = true;
                        break 'bfs;
                    }
                }
            }
            frontier = next_frontier;
        }
        Ok(ConsequenceNeighborhood {
            endpoints: endpoints.into_iter().collect(),
            edges,
            scanned_incidences,
            truncated,
        })
    }

    /// Trace outgoing consequence assertions chronologically across an
    /// inclusive generation range. Durable stores use indexed interval pages;
    /// this method does not replay `GraphAt` for each generation.
    ///
    /// Distinct endpoint/time/depth states and their assertions are retained as
    /// a provenance-backed DAG, including later alternate paths. A later hop
    /// may occur after an earlier assertion was retracted; every hop records
    /// its own validity interval and assigned generation. This is historical
    /// evidence, not a causal inference.
    /// An empty kind filter includes every consequence relation class.
    ///
    /// Reaching `max_hops` alone is intentional. Hitting endpoint, edge, or
    /// incidence caps before exhausting eligible work sets `truncated`.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for empty or oversized limits and
    /// a store error for unknown/reversed bounds, an upper bound newer than
    /// this query's pinned generation, or corrupt history.
    pub fn consequence_trace(
        &self,
        request: &ConsequenceTraceRequest,
    ) -> Result<ConsequenceTrace, QueryError> {
        if request.max_hops == 0
            || request.max_hops > MAX_CONSEQUENCE_NEIGHBORHOOD_HOPS
            || request.max_endpoints == 0
            || request.max_endpoints > MAX_CONSEQUENCE_NEIGHBORHOOD_ITEMS
            || request.max_edges == 0
            || request.max_edges > MAX_CONSEQUENCE_NEIGHBORHOOD_ITEMS
            || request.max_scanned_incidences == 0
            || request.max_scanned_incidences > MAX_CONSEQUENCE_NEIGHBORHOOD_SCANS
        {
            return Err(QueryError::InvalidLimit);
        }
        #[cfg(feature = "benchmark-instrumentation")]
        let mut read_metrics = ConsequenceTraceReadMetrics::default();
        let mut generation_sequence_cache = BTreeMap::new();
        let from_sequence = self.trace_generation_sequence(
            request.from_generation,
            &mut generation_sequence_cache,
            #[cfg(feature = "benchmark-instrumentation")]
            &mut read_metrics,
        )?;
        let until_sequence = self.trace_generation_sequence(
            request.until_generation,
            &mut generation_sequence_cache,
            #[cfg(feature = "benchmark-instrumentation")]
            &mut read_metrics,
        )?;
        let query_sequence = self.trace_generation_sequence(
            self.generation,
            &mut generation_sequence_cache,
            #[cfg(feature = "benchmark-instrumentation")]
            &mut read_metrics,
        )?;
        if from_sequence > until_sequence || until_sequence > query_sequence {
            return Err(QueryError::Store(StoreError::InvalidTemporalRange));
        }

        // Each temporal state is expanded once. Depth is part of the state,
        // so equal-generation cycles become an acyclic layered result.
        let mut queue = BTreeSet::from([(
            from_sequence,
            0_usize,
            request.origin,
            request.from_generation,
        )]);
        let mut visited_states = BTreeSet::from([(request.origin, from_sequence, 0_usize)]);
        let mut unique_endpoints = BTreeSet::from([request.origin]);
        let mut states = BTreeSet::from([ConsequenceTraceState {
            endpoint: request.origin,
            reached_generation: request.from_generation,
            depth: 0,
        }]);
        let mut hops = BTreeMap::<(ConsequenceEdgeId, u64, u64, usize), ConsequenceTraceHop>::new();
        let mut scanned_incidences = 0_usize;
        let mut truncated = false;

        'trace: while let Some((arrival_sequence, depth, endpoint, reached_generation)) =
            queue.pop_first()
        {
            if depth >= request.max_hops {
                continue;
            }

            let mut cursor = None;
            loop {
                if scanned_incidences == request.max_scanned_incidences {
                    truncated = true;
                    break 'trace;
                }
                let page_limit = request
                    .max_scanned_incidences
                    .saturating_sub(scanned_incidences)
                    .min(MAX_HISTORICAL_EDGE_PAGE_SIZE);
                let page = self.store.consequence_edges_for_endpoint_range(
                    endpoint,
                    request.from_generation,
                    request.until_generation,
                    cursor,
                    page_limit,
                )?;
                #[cfg(feature = "benchmark-instrumentation")]
                {
                    read_metrics.endpoint_range_pages =
                        read_metrics.endpoint_range_pages.saturating_add(1);
                    read_metrics.range_page_reads.sql_read_statements = read_metrics
                        .range_page_reads
                        .sql_read_statements
                        .saturating_add(page.read_metrics.sql_read_statements);
                    read_metrics
                        .range_page_reads
                        .reference_generation_entries_materialized = read_metrics
                        .range_page_reads
                        .reference_generation_entries_materialized
                        .saturating_add(
                            page.read_metrics.reference_generation_entries_materialized,
                        );
                    read_metrics
                        .range_page_reads
                        .reference_consequence_entries_materialized = read_metrics
                        .range_page_reads
                        .reference_consequence_entries_materialized
                        .saturating_add(
                            page.read_metrics.reference_consequence_entries_materialized,
                        );
                    read_metrics
                        .range_page_reads
                        .reference_consequence_entries_scanned = read_metrics
                        .range_page_reads
                        .reference_consequence_entries_scanned
                        .saturating_add(page.read_metrics.reference_consequence_entries_scanned);
                    read_metrics
                        .range_page_reads
                        .reference_consequence_mutations_scanned = read_metrics
                        .range_page_reads
                        .reference_consequence_mutations_scanned
                        .saturating_add(page.read_metrics.reference_consequence_mutations_scanned);
                    read_metrics.range_page_reads.consequence_rows_returned = read_metrics
                        .range_page_reads
                        .consequence_rows_returned
                        .saturating_add(page.read_metrics.consequence_rows_returned);
                }
                let has_more = page.next_cursor.is_some();
                cursor = page.next_cursor;
                let provenance_classes = if request.included_evidence_classes.is_empty() {
                    None
                } else {
                    let provenance_ids = page
                        .items
                        .iter()
                        .filter(|version| {
                            version.edge.source == endpoint
                                && (request.included_kinds.is_empty()
                                    || request.included_kinds.contains(&version.edge.kind))
                        })
                        .map(|version| version.edge.provenance)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<ProvenanceId>>();
                    let records = if provenance_ids.is_empty() {
                        Vec::new()
                    } else {
                        #[cfg(feature = "benchmark-instrumentation")]
                        {
                            read_metrics.provenance_lookup_calls =
                                read_metrics.provenance_lookup_calls.saturating_add(1);
                            read_metrics.provenance_ids_requested = read_metrics
                                .provenance_ids_requested
                                .saturating_add(provenance_ids.len());
                        }
                        self.store
                            .provenance_for_ids(self.generation, &provenance_ids)?
                    };
                    Some(
                        records
                            .into_iter()
                            .map(|provenance| (provenance.id, provenance.evidence_class))
                            .collect::<BTreeMap<_, _>>(),
                    )
                };
                for version in page.items {
                    scanned_incidences = scanned_incidences.saturating_add(1);
                    let edge = &version.edge;
                    if edge.source != endpoint
                        || (!request.included_kinds.is_empty()
                            && !request.included_kinds.contains(&edge.kind))
                    {
                        continue;
                    }
                    if let Some(classes) = &provenance_classes {
                        let class = classes.get(&edge.provenance).ok_or_else(|| {
                            QueryError::Store(StoreError::Integrity(format!(
                                "consequence edge {:?} references missing provenance {:?}",
                                edge.id, edge.provenance
                            )))
                        })?;
                        if !request.included_evidence_classes.contains(class) {
                            continue;
                        }
                    }
                    let start_sequence = self.trace_generation_sequence(
                        version.valid_from,
                        &mut generation_sequence_cache,
                        #[cfg(feature = "benchmark-instrumentation")]
                        &mut read_metrics,
                    )?;
                    let reached_sequence = arrival_sequence.max(start_sequence);
                    if reached_sequence > until_sequence {
                        continue;
                    }
                    if let Some(valid_until) = version.valid_until
                        && self.trace_generation_sequence(
                            valid_until,
                            &mut generation_sequence_cache,
                            #[cfg(feature = "benchmark-instrumentation")]
                            &mut read_metrics,
                        )? <= reached_sequence
                    {
                        continue;
                    }
                    let next_generation = if arrival_sequence >= start_sequence {
                        reached_generation
                    } else {
                        version.valid_from
                    };
                    let next_depth = depth.saturating_add(1);
                    let next_endpoint = edge.target;
                    if !unique_endpoints.contains(&next_endpoint)
                        && unique_endpoints.len() == request.max_endpoints
                    {
                        truncated = true;
                        continue;
                    }
                    let hop_key = (edge.id, arrival_sequence, reached_sequence, next_depth);
                    if !hops.contains_key(&hop_key) && hops.len() == request.max_edges {
                        truncated = true;
                        break 'trace;
                    }
                    hops.entry(hop_key).or_insert_with(|| ConsequenceTraceHop {
                        depth: next_depth,
                        source_reached_generation: reached_generation,
                        reached_generation: next_generation,
                        version: version.clone(),
                    });
                    unique_endpoints.insert(next_endpoint);
                    states.insert(ConsequenceTraceState {
                        endpoint: next_endpoint,
                        reached_generation: next_generation,
                        depth: next_depth,
                    });
                    if visited_states.insert((next_endpoint, reached_sequence, next_depth)) {
                        queue.insert((
                            reached_sequence,
                            next_depth,
                            next_endpoint,
                            next_generation,
                        ));
                    }
                }
                if !has_more {
                    break;
                }
                if scanned_incidences == request.max_scanned_incidences {
                    truncated = true;
                    break 'trace;
                }
            }
        }

        let mut states = states.into_iter().collect::<Vec<_>>();
        states.sort_by_key(|item| (item.depth, item.reached_generation, item.endpoint));
        let mut hops = hops.into_values().collect::<Vec<_>>();
        hops.sort_by_key(|item| {
            (
                item.depth,
                item.reached_generation,
                item.source_reached_generation,
                item.version.edge.id,
            )
        });
        Ok(ConsequenceTrace {
            origin: request.origin,
            from_generation: request.from_generation,
            until_generation: request.until_generation,
            states,
            hops,
            scanned_incidences,
            truncated,
            #[cfg(feature = "benchmark-instrumentation")]
            read_metrics,
        })
    }

    /// Serialize one bounded chronological consequence trace as deterministic
    /// versioned NDJSON records, preserving the typed trace's states and hops.
    /// The byte cap includes the header and footer and never cuts a record.
    ///
    /// # Errors
    /// Returns the same range/limit/store errors as [`Self::consequence_trace`],
    /// [`QueryError::InvalidLimit`] for an invalid byte cap, and
    /// [`QueryError::OutputBudgetTooSmall`] when the cap cannot fit framing.
    pub fn consequence_trace_records(
        &self,
        request: &ConsequenceTraceRequest,
        max_result_bytes: usize,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        if max_result_bytes == 0 || max_result_bytes > MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES {
            return Err(QueryError::InvalidLimit);
        }
        let trace = self.consequence_trace(request)?;
        let header = TemporalRecord::ConsequenceTraceHeader {
            schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            origin: trace.origin,
            from_generation: trace.from_generation,
            until_generation: trace.until_generation,
        };
        let maximum_footer = TemporalRecord::ConsequenceTraceFooter {
            schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            states: u64::MAX,
            hops: u64::MAX,
            scanned_incidences: u64::MAX,
            truncated: true,
        };
        let header_bytes = serialized_line_size(&header, max_result_bytes)?.ok_or(
            QueryError::OutputBudgetTooSmall {
                requested_bytes: max_result_bytes,
            },
        )?;
        let footer_bytes = serialized_line_size(&maximum_footer, max_result_bytes)?.ok_or(
            QueryError::OutputBudgetTooSmall {
                requested_bytes: max_result_bytes,
            },
        )?;
        if header_bytes.saturating_add(footer_bytes) > max_result_bytes {
            return Err(QueryError::OutputBudgetTooSmall {
                requested_bytes: max_result_bytes,
            });
        }
        let item_budget = max_result_bytes
            .saturating_sub(header_bytes)
            .saturating_sub(footer_bytes);
        let mut records = Vec::with_capacity(
            trace
                .states
                .len()
                .saturating_add(trace.hops.len())
                .saturating_add(2),
        );
        records.push(header);
        let mut serialized_item_bytes = 0_usize;
        let mut state_count = 0_u64;
        let mut hop_count = 0_u64;
        let mut truncated = trace.truncated;
        'items: for state in &trace.states {
            let record = TemporalRecord::ConsequenceTraceState {
                schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                endpoint: state.endpoint,
                reached_generation: state.reached_generation,
                depth: u64::try_from(state.depth).unwrap_or(u64::MAX),
            };
            let remaining = item_budget.saturating_sub(serialized_item_bytes);
            let Some(line_bytes) = serialized_line_size(&record, remaining)? else {
                truncated = true;
                break 'items;
            };
            serialized_item_bytes = serialized_item_bytes.saturating_add(line_bytes);
            state_count = state_count.saturating_add(1);
            records.push(record);
        }
        if !truncated || state_count == u64::try_from(trace.states.len()).unwrap_or(u64::MAX) {
            for hop in &trace.hops {
                let record = TemporalRecord::ConsequenceTraceHop {
                    schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
                    query_mode: TemporalQueryMode::HistoricalConclusion,
                    depth: u64::try_from(hop.depth).unwrap_or(u64::MAX),
                    source_reached_generation: hop.source_reached_generation,
                    reached_generation: hop.reached_generation,
                    version: hop.version.clone(),
                };
                let remaining = item_budget.saturating_sub(serialized_item_bytes);
                let Some(line_bytes) = serialized_line_size(&record, remaining)? else {
                    truncated = true;
                    break;
                };
                serialized_item_bytes = serialized_item_bytes.saturating_add(line_bytes);
                hop_count = hop_count.saturating_add(1);
                records.push(record);
            }
        }
        records.push(TemporalRecord::ConsequenceTraceFooter {
            schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            states: state_count,
            hops: hop_count,
            scanned_incidences: u64::try_from(trace.scanned_incidences).unwrap_or(u64::MAX),
            truncated,
        });
        Ok(records)
    }

    /// Read the ChangeSet declaration valid at this query's pinned generation.
    /// Durable stores use the declaration-validity index rather than replaying
    /// prior generation deltas.
    ///
    /// # Errors
    /// Returns a store error when the generation is stale or the declaration
    /// index is unavailable.
    pub fn change_set_at(
        &self,
        change_set: ChangeSetId,
    ) -> Result<Option<TemporalRecord>, QueryError> {
        Ok(self
            .store
            .change_set_at(change_set, self.generation)?
            .map(|version| TemporalRecord::ChangeSetVersion {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                change_set: version.change_set,
                valid_from: version.valid_from,
                valid_until: version.valid_until,
            }))
    }

    /// Find later events that directly changed the same canonical fact as
    /// this query's source event. The relation is historical correlation, not
    /// a causal claim. Durable stores use their fact/event index.
    ///
    /// `after` must be a continuation cursor returned for this query's source
    /// event. Each page is bounded by `limit` and ordered by generation.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidCorrelationAnchor`] if this generation has
    /// no change event, the fact was not changed by it, or the cursor belongs
    /// to another source event, precedes its source, or does not identify an
    /// event that directly changed the selected fact.
    pub fn change_event_correlations(
        &self,
        fact: FactRef,
        after: Option<ChangeEventCorrelationCursor>,
        limit: usize,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        let source = self
            .store
            .change_event(self.generation)?
            .ok_or(QueryError::InvalidCorrelationAnchor)?;
        if !source
            .changed_facts
            .iter()
            .any(|changed| changed.fact == fact)
        {
            return Err(QueryError::InvalidCorrelationAnchor);
        }
        let after_generation = if let Some(cursor) = after {
            if cursor.source_event != source.id
                || self.store.generation_sequence(cursor.after_generation)?
                    < self.store.generation_sequence(self.generation)?
            {
                return Err(QueryError::InvalidCorrelationAnchor);
            }
            let cursor_event = self
                .store
                .change_event(cursor.after_generation)?
                .ok_or(QueryError::InvalidCorrelationAnchor)?;
            if !cursor_event
                .changed_facts
                .iter()
                .any(|changed| changed.fact == fact)
            {
                return Err(QueryError::InvalidCorrelationAnchor);
            }
            cursor.after_generation
        } else {
            self.generation
        };
        let page = self.store.change_events_for_fact(
            fact,
            Some(ChangeEventCursor {
                generation: after_generation,
            }),
            limit,
        )?;
        let returned = u64::try_from(page.items.len()).map_err(|error| {
            QueryError::Store(StoreError::Backend(format!(
                "change-event correlation page length is not representable: {error}"
            )))
        })?;
        let has_more = page.next_cursor.is_some();
        let mut records = page
            .items
            .into_iter()
            .map(|event| TemporalRecord::ChangeEventCorrelation {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                correlation: ChangeEventCorrelation {
                    kind: syntaxmesh_core::ChangeEventRelationKind::HistoricallyCorrelated,
                    source_event: source.id,
                    source_generation: source.generation_after,
                    target_event: event.id,
                    target_generation: event.generation_after,
                    shared_fact: fact,
                },
            })
            .collect::<Vec<_>>();
        records.push(TemporalRecord::ChangeEventCorrelationFooter {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            returned,
            has_more,
            next_cursor: page.next_cursor.map(|cursor| ChangeEventCorrelationCursor {
                source_event: source.id,
                after_generation: cursor.generation,
            }),
        });
        Ok(records)
    }

    /// Return the atomic change event for this query's pinned generation.
    ///
    /// This also retrieves transitions that changed no canonical fact; a
    /// generation without an event (for example, a history anchor) returns
    /// `None`.
    ///
    /// # Errors
    /// Returns a store error if the generation is unavailable or its event is
    /// malformed.
    pub fn change_event(&self) -> Result<Option<TemporalRecord>, QueryError> {
        Ok(self
            .store
            .change_event(self.generation)?
            .map(|event| TemporalRecord::ChangeEvent {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                event,
            }))
    }

    /// Return the complete graph at one retained generation.
    ///
    /// This is an output-sized read; durable stores select active temporal
    /// fact versions directly and verify the result against its manifest.
    ///
    /// # Errors
    /// Returns a store error when the requested generation is unavailable or
    /// its checkpoint/history cannot be validated.
    pub fn graph_at(&self, generation: GenerationId) -> Result<GraphSnapshot, QueryError> {
        Ok(self.store.historical_snapshot(generation)?)
    }

    /// Export one retained historical graph as deterministic NDJSON records.
    ///
    /// # Errors
    /// Returns a store error when the generation or its historical snapshot is
    /// unavailable or fails integrity validation.
    pub fn export_graph_at(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<GraphRecord>, QueryError> {
        let manifest = self.store.manifest(generation)?;
        let snapshot = self.store.historical_snapshot(generation)?;
        let mut records = Vec::with_capacity(
            snapshot
                .provenance
                .len()
                .saturating_add(snapshot.nodes.len())
                .saturating_add(snapshot.edges.len())
                .saturating_add(2),
        );
        records.push(GraphRecord::Header {
            schema_version: GRAPH_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            repository: manifest.repository,
            generation: manifest.generation,
            accepted_by: None,
            accepted_through: None,
        });
        records.extend(
            snapshot
                .provenance
                .into_iter()
                .map(|provenance| GraphRecord::Provenance {
                    query_mode: TemporalQueryMode::HistoricalConclusion,
                    provenance,
                }),
        );
        records.extend(snapshot.nodes.into_iter().map(|node| GraphRecord::Node {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            node,
        }));
        records.extend(snapshot.edges.into_iter().map(|edge| GraphRecord::Edge {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            edge,
        }));
        records.push(GraphRecord::Footer {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            nodes: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Node { .. }))
                .count() as u64,
            edges: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Edge { .. }))
                .count() as u64,
            provenance_records: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Provenance { .. }))
                .count() as u64,
            truncated: false,
            blake3: None,
        });
        Ok(records)
    }

    /// Export the exact graph generation only when its complete retained
    /// ancestry has known engine-acceptance times no later than the cutoff.
    /// Durable stores perform an exact generation-prefix lookup.
    ///
    /// # Errors
    /// Returns a store error for unavailable generations or
    /// [`QueryError::UnknownAcceptanceHistory`] / [`QueryError::GenerationNotKnownBy`]
    /// when the requested acceptance qualification cannot be established.
    pub fn export_graph_at_known_by(
        &self,
        generation: GenerationId,
        accepted_by: AcceptanceTime,
    ) -> Result<Vec<GraphRecord>, QueryError> {
        let accepted_through = self
            .store
            .accepted_through(generation)?
            .ok_or(QueryError::UnknownAcceptanceHistory(generation))?;
        if accepted_through > accepted_by {
            return Err(QueryError::GenerationNotKnownBy {
                generation,
                accepted_by,
                accepted_through,
            });
        }
        let mut records = self.export_graph_at(generation)?;
        if let Some(GraphRecord::Header {
            accepted_by: field,
            accepted_through: prefix,
            ..
        }) = records.first_mut()
        {
            *field = Some(accepted_by);
            *prefix = Some(accepted_through);
        }
        Ok(records)
    }

    /// Return accepted graph transitions after `from` through `to` inclusive.
    ///
    /// # Errors
    /// Returns a store error when either endpoint is unavailable or the range
    /// is not one contiguous, forward generation chain.
    pub fn changed_between(
        &self,
        from: GenerationId,
        to: GenerationId,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        Ok(self
            .store
            .changes_between(from, to)?
            .into_iter()
            .map(|change: GenerationChange| TemporalRecord::Change {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                manifest: change.manifest,
                delta: change.delta,
            })
            .collect())
    }

    /// List known generation acceptances within `[from_inclusive, until_exclusive)`.
    /// Results are ordered by acceptance timestamp then generation ID and
    /// bounded by `limit`; this lists history but does not select a graph state.
    /// Pass the previous page's continuation cursor to continue through ties
    /// and avoid skipping records when a range exceeds the result limit.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for a zero limit or an empty/reversed
    /// interval, and a store error if indexed acceptance metadata is invalid.
    pub fn accepted_between(
        &self,
        from_inclusive: AcceptanceTime,
        until_exclusive: AcceptanceTime,
        after: Option<AcceptedGenerationCursor>,
        limit: usize,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        if limit == 0 || from_inclusive >= until_exclusive {
            return Err(QueryError::InvalidLimit);
        }
        let page = self.store.accepted_generations_between(
            from_inclusive,
            until_exclusive,
            after,
            limit,
        )?;
        let returned = u64::try_from(page.items.len())
            .map_err(|_conversion_error| QueryError::InvalidLimit)?;
        let has_more = page.next_cursor.is_some();
        let mut records = page
            .items
            .into_iter()
            .map(|accepted| TemporalRecord::AcceptedGeneration {
                schema_version: ACCEPTANCE_TIMELINE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::AcceptanceTimeline,
                accepted_at: accepted.accepted_at,
                manifest: accepted.manifest,
            })
            .collect::<Vec<_>>();
        records.push(TemporalRecord::AcceptanceTimelineFooter {
            schema_version: ACCEPTANCE_TIMELINE_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::AcceptanceTimeline,
            returned,
            has_more,
            next_cursor: page.next_cursor,
        });
        Ok(records)
    }

    /// List fact versions by producer occurrence time in `[from, until)`.
    /// This does not select a graph state at that calendar time.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for an empty/reversed range or zero limit.
    pub fn observed_between(
        &self,
        from_inclusive: ObservationTime,
        until_exclusive: ObservationTime,
        after: Option<ObservedFactCursor>,
        limit: usize,
    ) -> Result<Vec<TemporalRecord>, QueryError> {
        if limit == 0 || from_inclusive >= until_exclusive {
            return Err(QueryError::InvalidLimit);
        }
        let page =
            self.store
                .observed_facts_between(from_inclusive, until_exclusive, after, limit)?;
        let returned = u64::try_from(page.items.len())
            .map_err(|_conversion_error| QueryError::InvalidLimit)?;
        let has_more = page.next_cursor.is_some();
        let mut records = Vec::with_capacity(page.items.len().saturating_add(1));
        for item in page.items {
            let Some(observed_at) = item.version.observed_at else {
                return Err(QueryError::Store(StoreError::Integrity(
                    "observation-time index returned a fact without an observation time".to_owned(),
                )));
            };
            records.push(TemporalRecord::ObservedFact {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::ObservationTimeline,
                fact: item.fact,
                valid_from: item.version.valid_from,
                valid_until: item.version.valid_until,
                observed_at,
                accepted_at: item.version.accepted_at,
                payload: item.version.payload,
            });
        }
        records.push(TemporalRecord::ObservedTimelineFooter {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::ObservationTimeline,
            returned,
            has_more,
            next_cursor: page.next_cursor,
        });
        Ok(records)
    }

    /// Export a deterministic, weakly connected neighborhood as bounded NDJSON records.
    ///
    /// Traversal includes incoming and outgoing edges. The node cap may omit
    /// eligible neighbors and the edge cap may omit eligible relationships;
    /// either case is signaled in the footer's `truncated` field. Depth-bounded
    /// frontier omission is intentional and does not itself set `truncated`.
    ///
    /// # Errors
    /// Returns [`QueryError::InvalidLimit`] for empty seeds, zero limits, or
    /// more unique seeds than the node cap; returns [`QueryError::UnknownSeed`]
    /// when any seed is absent from the pinned generation; returns a store
    /// error when the generation is stale or unavailable.
    pub fn export_subgraph(
        &self,
        seeds: &[NodeId],
        max_hops: usize,
        max_nodes: usize,
        max_edges: usize,
    ) -> Result<Vec<GraphRecord>, QueryError> {
        if seeds.is_empty() || max_hops == 0 || max_nodes == 0 || max_edges == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let unique_seeds = seeds.iter().copied().collect::<BTreeSet<_>>();
        if unique_seeds.len() > max_nodes {
            return Err(QueryError::InvalidLimit);
        }
        let mut selected = BTreeSet::new();
        let mut frontier = VecDeque::new();
        for seed in unique_seeds {
            if self.store.node(self.generation, seed)?.is_none() {
                return Err(QueryError::UnknownSeed(seed));
            }
            selected.insert(seed);
            frontier.push_back((seed, 0_usize));
        }
        let mut truncated = false;
        while let Some((current, depth)) = frontier.pop_front() {
            if depth >= max_hops {
                continue;
            }
            let mut incident = self.store.outgoing(self.generation, current)?;
            incident.extend(self.store.incoming(self.generation, current)?);
            incident.sort_by_key(|edge| edge.id);
            for edge in incident {
                let neighbor = if edge.source == current {
                    edge.target
                } else {
                    edge.source
                };
                if selected.contains(&neighbor) {
                    continue;
                }
                if selected.len() == max_nodes {
                    truncated = true;
                    continue;
                }
                selected.insert(neighbor);
                frontier.push_back((neighbor, depth.saturating_add(1)));
            }
        }

        let mut nodes = Vec::new();
        for id in &selected {
            if let Some(node) = self.store.node(self.generation, *id)? {
                nodes.push(node);
            }
        }
        let mut eligible_edges = BTreeMap::new();
        for id in &selected {
            for edge in self.store.outgoing(self.generation, *id)? {
                if selected.contains(&edge.source) && selected.contains(&edge.target) {
                    eligible_edges.entry(edge.id).or_insert(edge);
                }
            }
        }
        if eligible_edges.len() > max_edges {
            truncated = true;
        }
        let edges = eligible_edges
            .into_values()
            .take(max_edges)
            .collect::<Vec<_>>();
        let provenance_ids = nodes
            .iter()
            .map(|node| node.provenance)
            .chain(edges.iter().map(|edge| edge.provenance))
            .collect::<BTreeSet<_>>();
        let provenance = self
            .store
            .provenance(self.generation)?
            .into_iter()
            .filter(|item| provenance_ids.contains(&item.id))
            .collect::<Vec<_>>();
        let manifest = self.store.manifest(self.generation)?;
        let mut records = Vec::with_capacity(
            provenance
                .len()
                .saturating_add(nodes.len())
                .saturating_add(edges.len())
                .saturating_add(2),
        );
        records.push(GraphRecord::Header {
            schema_version: GRAPH_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            repository: manifest.repository,
            generation: manifest.generation,
            accepted_by: None,
            accepted_through: None,
        });
        records.extend(
            provenance
                .into_iter()
                .map(|provenance| GraphRecord::Provenance {
                    query_mode: TemporalQueryMode::HistoricalConclusion,
                    provenance,
                }),
        );
        records.extend(nodes.into_iter().map(|node| GraphRecord::Node {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            node,
        }));
        records.extend(edges.into_iter().map(|edge| GraphRecord::Edge {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            edge,
        }));
        records.push(GraphRecord::Footer {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            nodes: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Node { .. }))
                .count() as u64,
            edges: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Edge { .. }))
                .count() as u64,
            provenance_records: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Provenance { .. }))
                .count() as u64,
            truncated,
            blake3: None,
        });
        Ok(records)
    }

    /// Case-insensitive substring search over canonical node names.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn search(&self, text: &str, limit: usize) -> Result<Vec<Node>, QueryError> {
        if limit == 0 {
            return Err(QueryError::InvalidLimit);
        }
        Ok(self.store.search_nodes(self.generation, text, limit)?)
    }

    /// Return outgoing edges and their target nodes for one node.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn neighbors(&self, id: NodeId) -> Result<Vec<(Edge, Node)>, QueryError> {
        let edges = self.store.outgoing(self.generation, id)?;
        let mut result = Vec::new();
        for edge in edges {
            if let Some(node) = self.store.node(self.generation, edge.target)? {
                result.push((edge, node));
            }
        }
        Ok(result)
    }

    /// Find a bounded directed path using breadth-first traversal.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable, or an
    /// invalid-limit error when `max_hops` is zero.
    pub fn path(
        &self,
        start: NodeId,
        target: NodeId,
        max_hops: usize,
    ) -> Result<Option<Vec<NodeId>>, QueryError> {
        if max_hops == 0 {
            return Err(QueryError::InvalidLimit);
        }
        if self.store.node(self.generation, start)?.is_none()
            || self.store.node(self.generation, target)?.is_none()
        {
            return Ok(None);
        }
        if start == target {
            return Ok(Some(vec![start]));
        }
        let mut frontier = VecDeque::from([(start, vec![start])]);
        let mut visited = BTreeSet::from([start]);
        while let Some((current, path)) = frontier.pop_front() {
            let hops = path.len().saturating_sub(1);
            if hops >= max_hops {
                continue;
            }
            for edge in self.store.outgoing(self.generation, current)? {
                if visited.insert(edge.target) {
                    let mut next = path.clone();
                    next.push(edge.target);
                    if edge.target == target {
                        return Ok(Some(next));
                    }
                    frontier.push_back((edge.target, next));
                }
            }
        }
        Ok(None)
    }

    /// Return nodes that can reach `target` through incoming edges, bounded by
    /// `max_hops`; the target itself is excluded.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable, or an
    /// invalid-limit error when `max_hops` is zero.
    pub fn impact(&self, target: NodeId, max_hops: usize) -> Result<Vec<NodeId>, QueryError> {
        if max_hops == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let mut frontier = VecDeque::from([(target, 0_usize)]);
        let mut visited = BTreeSet::from([target]);
        let mut result = Vec::new();
        while let Some((current, depth)) = frontier.pop_front() {
            if depth >= max_hops {
                continue;
            }
            for edge in self.store.incoming(self.generation, current)? {
                if visited.insert(edge.source) {
                    result.push(edge.source);
                    frontier.push_back((edge.source, depth.saturating_add(1)));
                }
            }
        }
        Ok(result)
    }

    /// Export one complete generation in deterministic NDJSON record order.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn export_records(&self) -> Result<Vec<GraphRecord>, QueryError> {
        let manifest = self.store.manifest(self.generation)?;
        let provenance = self.store.provenance(self.generation)?;
        let nodes = self.store.nodes(self.generation)?;
        let edges = self.store.edges(self.generation)?;
        let mut records = Vec::with_capacity(
            provenance
                .len()
                .saturating_add(nodes.len())
                .saturating_add(edges.len())
                .saturating_add(2),
        );
        records.push(GraphRecord::Header {
            schema_version: GRAPH_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            repository: manifest.repository,
            generation: manifest.generation,
            accepted_by: None,
            accepted_through: None,
        });
        records.extend(
            provenance
                .into_iter()
                .map(|provenance| GraphRecord::Provenance {
                    query_mode: TemporalQueryMode::HistoricalConclusion,
                    provenance,
                }),
        );
        records.extend(nodes.into_iter().map(|node| GraphRecord::Node {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            node,
        }));
        records.extend(edges.into_iter().map(|edge| GraphRecord::Edge {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            edge,
        }));
        records.push(GraphRecord::Footer {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            nodes: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Node { .. }))
                .count() as u64,
            edges: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Edge { .. }))
                .count() as u64,
            provenance_records: records
                .iter()
                .filter(|record| matches!(record, GraphRecord::Provenance { .. }))
                .count() as u64,
            truncated: false,
            blake3: None,
        });
        Ok(records)
    }
}

#[cfg(test)]
mod tests;
