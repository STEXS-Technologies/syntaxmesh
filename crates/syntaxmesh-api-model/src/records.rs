//! Versioned interchange records shared by future CLI, HTTP, MCP, and exports.

use serde::{Deserialize, Serialize};
use syntaxmesh_core::{
    AcceptanceTime, AcceptedGenerationCursor, ChangeEventCorrelation, ChangeEventCorrelationCursor,
    ChangeSet, ChangeSetEvent, ConsequenceEdgeVersion, Edge, EdgeDirection, EdgeId,
    EventsForChangeSetCursor, FactPayload, FactVersionRef, GenerationId, GenerationManifest,
    LineageEndpoint, Node, NodeId, ObservationTime, ObservedFactCursor, Provenance, RepositoryId,
};

pub const GRAPH_EXPORT_SCHEMA_VERSION: u32 = 7;
pub const TEMPORAL_EXPORT_SCHEMA_VERSION: u32 = 11;
pub const ACCEPTANCE_TIMELINE_SCHEMA_VERSION: u32 = 2;
pub const HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION: u32 = 1;
pub const CONSEQUENCE_TRACE_SCHEMA_VERSION: u32 = 1;
pub const CONTEXT_PACK_SCHEMA_VERSION: u32 = 2;

/// Input to the deterministic, generation-pinned context compiler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRequest {
    pub query: String,
    pub seed_nodes: Vec<syntaxmesh_core::NodeId>,
    pub token_budget: u64,
    pub max_hops: u8,
    pub max_candidates: u16,
}

/// Evidence item class; declaration order is not a relevance score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemKind {
    SourceEvidence,
    Signature,
    GraphPath,
    Summary,
}

/// One complete evidence item selected for an agent context pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItem {
    pub rank: u32,
    /// Class of the selecting node or path edge, not proof of quote entailment.
    /// Legacy context payloads lack this field and retain unknown classification.
    #[serde(default)]
    pub evidence_class: Option<syntaxmesh_core::EvidenceClass>,
    pub kind: ContextItemKind,
    pub text: String,
    pub node_ids: Vec<syntaxmesh_core::NodeId>,
    pub edge_ids: Vec<syntaxmesh_core::EdgeId>,
    pub source_path: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
}

/// A bounded explanation of source or query ambiguity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextWarning {
    pub code: String,
    pub message: String,
    pub node_ids: Vec<syntaxmesh_core::NodeId>,
}

/// Counts of omitted complete candidates by class, never a truncated snippet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmittedContextSummary {
    pub source_evidence: u32,
    pub signatures: u32,
    pub graph_paths: u32,
    pub summaries: u32,
}

impl OmittedContextSummary {
    #[must_use]
    pub const fn total(self) -> u32 {
        self.source_evidence
            .saturating_add(self.signatures)
            .saturating_add(self.graph_paths)
            .saturating_add(self.summaries)
    }
}

/// Versioned, generation-pinned evidence context with tokenizer-specific exact accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPack {
    pub schema_version: u32,
    pub query: String,
    pub repository: syntaxmesh_core::RepositoryId,
    pub worktree: syntaxmesh_core::WorktreeId,
    pub generation: syntaxmesh_core::GenerationId,
    pub tokenizer: String,
    pub token_budget: u64,
    pub token_count: u64,
    pub items: Vec<ContextItem>,
    pub warnings: Vec<ContextWarning>,
    pub omitted: OmittedContextSummary,
}

/// Continuation point tied to one exact historical endpoint query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalNeighborCursor {
    pub generation: GenerationId,
    pub endpoint: NodeId,
    pub direction: EdgeDirection,
    pub after_edge: EdgeId,
}

/// Describes what temporal semantics produced a serialized query record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalQueryMode {
    /// Facts as recorded for the selected accepted generation, without
    /// re-evaluation under newer semantics.
    HistoricalConclusion,
    /// Facts selected by producer-reported event time; this is evidence
    /// listing, not a graph-state conclusion.
    ObservationTimeline,
    /// Generations selected by engine acceptance time; this is a metadata
    /// listing, not a graph-state conclusion.
    AcceptanceTimeline,
    /// Reserved for an explicit future query that evaluates old state under
    /// current semantics. No current query emits this value.
    CurrentSemanticsRetrospective,
}

/// A stream starts with Header, carries facts, and ends with Footer. This
/// enum is an interchange format, never the canonical store representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GraphRecord {
    Header {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        repository: RepositoryId,
        generation: GenerationId,
        /// Optional engine-acceptance cutoff for a qualified historical graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accepted_by: Option<AcceptanceTime>,
        /// Maximum acceptance time on the path from retained anchor to generation.
        /// Missing means the ancestry includes unknown legacy acceptance metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accepted_through: Option<AcceptanceTime>,
    },
    Node {
        query_mode: TemporalQueryMode,
        node: Node,
    },
    Edge {
        query_mode: TemporalQueryMode,
        edge: Edge,
    },
    Provenance {
        query_mode: TemporalQueryMode,
        provenance: Provenance,
    },
    Footer {
        query_mode: TemporalQueryMode,
        nodes: u64,
        edges: u64,
        provenance_records: u64,
        /// True when node or edge limits omitted eligible subgraph records.
        truncated: bool,
        /// Hash of the deterministic record stream, absent in fast streaming mode.
        blake3: Option<[u8; 32]>,
    },
}

/// Versioned NDJSON records for generation-based timeline queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemporalRecord {
    /// One selected node and its shortest discovered depth in an exact-generation traversal.
    HistoricalNeighborhoodNode {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        generation: GenerationId,
        depth: usize,
        node: Node,
    },
    /// One selected edge and its shortest discovered depth in an exact-generation traversal.
    HistoricalNeighborhoodEdge {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        generation: GenerationId,
        depth: usize,
        edge: Edge,
    },
    /// Bounded traversal totals; this stream is complete for its declared budget, not pageable.
    HistoricalNeighborhoodFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        generation: GenerationId,
        nodes: u64,
        edges: u64,
        scanned_incidence_entries: u64,
        serialized_item_bytes: u64,
        truncated: bool,
    },
    /// One historical edge and its opposite endpoint at the pinned generation.
    HistoricalNeighbor {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        generation: GenerationId,
        endpoint: NodeId,
        direction: EdgeDirection,
        edge: Edge,
        neighbor: Node,
    },
    /// Continuation state for one bounded historical-neighbor page.
    HistoricalNeighborFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        generation: GenerationId,
        endpoint: NodeId,
        direction: EdgeDirection,
        returned: u64,
        has_more: bool,
        next_cursor: Option<HistoricalNeighborCursor>,
    },
    /// One generation accepted during a requested calendar-time interval.
    AcceptedGeneration {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        accepted_at: AcceptanceTime,
        manifest: GenerationManifest,
    },
    /// Pagination status for the preceding acceptance-time page.
    AcceptanceTimelineFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        returned: u64,
        has_more: bool,
        next_cursor: Option<AcceptedGenerationCursor>,
    },
    /// One historical payload interval for a graph node.
    NodeVersion {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        valid_from: GenerationId,
        valid_until: Option<GenerationId>,
        node: Node,
    },
    /// One accepted delta in a requested generation range.
    Change {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        manifest: syntaxmesh_core::GenerationManifest,
        delta: syntaxmesh_core::GraphDelta,
    },
    /// One version of a canonical file, provenance, node, or edge fact.
    FactVersion {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        valid_from: GenerationId,
        valid_until: Option<GenerationId>,
        observed_at: Option<ObservationTime>,
        accepted_at: Option<AcceptanceTime>,
        payload: FactPayload,
    },
    /// A payload version directly superseded by the next contiguous version
    /// of the same stable fact identity. This does not imply cross-fact cause.
    FactSupersedes {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        prior: FactVersionRef,
        current: FactVersionRef,
        accepted_at: Option<AcceptanceTime>,
    },
    /// One accepted transition directly associated with a canonical fact.
    ChangeEvent {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        event: syntaxmesh_core::ChangeEvent,
    },
    /// Pagination status for a fact-linked change-event page.
    ChangeEventFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        returned: u64,
        has_more: bool,
        next_cursor: Option<syntaxmesh_core::ChangeEventCursor>,
    },
    /// A later event that directly changed the same canonical fact identity
    /// as the source event. This records historical correlation, not cause.
    ChangeEventCorrelation {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        correlation: ChangeEventCorrelation,
    },
    /// Pagination status for a source-event/fact correlation page.
    ChangeEventCorrelationFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        returned: u64,
        has_more: bool,
        next_cursor: Option<ChangeEventCorrelationCursor>,
    },
    /// An explicitly assigned event and the provenance supporting membership.
    ChangeSetEvent {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        item: ChangeSetEvent,
    },
    /// Pagination status for events belonging to one ChangeSet at one snapshot.
    ChangeSetEventsFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        returned: u64,
        has_more: bool,
        next_cursor: Option<EventsForChangeSetCursor>,
    },
    /// One ChangeSet declaration version selected at its valid generation.
    ChangeSetVersion {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        change_set: ChangeSet,
        valid_from: GenerationId,
        valid_until: Option<GenerationId>,
    },
    /// One evidence-backed consequence edge in a bounded neighborhood.
    ConsequenceEdge {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        generation: GenerationId,
        depth: u64,
        edge: syntaxmesh_core::ConsequenceEdge,
    },
    /// Completion and explicit truncation status for a consequence traversal.
    ConsequenceNeighborhoodFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        generation: GenerationId,
        endpoints: u64,
        edges: u64,
        scanned_incidences: u64,
        truncated: bool,
    },
    /// One canonical fact version selected by producer occurrence time.
    ObservedFact {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        fact: syntaxmesh_core::FactRef,
        valid_from: GenerationId,
        valid_until: Option<GenerationId>,
        observed_at: ObservationTime,
        accepted_at: Option<AcceptanceTime>,
        payload: FactPayload,
    },
    /// Pagination status for an observation-time range query.
    ObservedTimelineFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        returned: u64,
        has_more: bool,
        next_cursor: Option<ObservedFactCursor>,
    },
    /// Header binding a temporal consequence trace stream to its origin and range.
    ConsequenceTraceHeader {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        origin: LineageEndpoint,
        from_generation: GenerationId,
        until_generation: GenerationId,
    },
    /// One vertex in the temporal DAG, distinct by endpoint, time, and depth.
    ConsequenceTraceState {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        endpoint: LineageEndpoint,
        reached_generation: GenerationId,
        depth: u64,
    },
    /// One temporally assigned hop with the original assertion validity and evidence.
    ConsequenceTraceHop {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        depth: u64,
        source_reached_generation: GenerationId,
        reached_generation: GenerationId,
        version: ConsequenceEdgeVersion,
    },
    /// Completion and explicit truncation status for a temporal trace stream.
    ConsequenceTraceFooter {
        schema_version: u32,
        query_mode: TemporalQueryMode,
        states: u64,
        hops: u64,
        scanned_incidences: u64,
        truncated: bool,
    },
}

impl TemporalRecord {
    /// Serialize one independently parseable NDJSON record.
    ///
    /// # Errors
    /// Returns a JSON serialization error if the record cannot be encoded.
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl GraphRecord {
    /// Serialize exactly one JSON Lines record without a trailing newline.
    ///
    /// # Errors
    /// Returns a JSON serialization error if a record cannot be encoded.
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_one_independently_parseable_line() {
        let record = GraphRecord::Header {
            schema_version: GRAPH_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            repository: RepositoryId::derive(&[b"repo"]),
            generation: GenerationId::derive(&[b"generation"]),
            accepted_by: None,
            accepted_through: None,
        };
        let line = record.to_json_line().unwrap_or_default();
        assert!(line.contains("\"schema_version\":7"));
        assert!(line.contains("\"query_mode\":\"historical_conclusion\""));
        assert!(!line.is_empty());
        assert!(!line.contains('\n'));
        assert!(serde_json::from_str::<GraphRecord>(&line).is_ok_and(|parsed| parsed == record));
    }

    #[test]
    fn footer_round_trips_explicit_truncation_status() {
        let record = GraphRecord::Footer {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            nodes: 2,
            edges: 1,
            provenance_records: 1,
            truncated: true,
            blake3: None,
        };
        let line = record.to_json_line().unwrap_or_default();
        assert!(line.contains("\"query_mode\":\"historical_conclusion\""));
        assert!(serde_json::from_str::<GraphRecord>(&line).is_ok_and(|parsed| parsed == record));
    }

    #[test]
    fn temporal_node_version_round_trips_with_its_own_schema_version() {
        let record = TemporalRecord::NodeVersion {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            valid_from: GenerationId::derive(&[b"from"]),
            valid_until: Some(GenerationId::derive(&[b"until"])),
            node: Node {
                id: syntaxmesh_core::NodeId::derive(&[b"node"]),
                kind: syntaxmesh_core::NodeKind::Function,
                name: "f".to_owned(),
                owner_file: None,
                source: None,
                provenance: syntaxmesh_core::ProvenanceId::derive(&[b"provenance"]),
                extension_payload: None,
            },
        };
        let line = record.to_json_line().unwrap_or_default();
        assert!(line.contains("\"schema_version\":11"));
        assert!(line.contains("\"query_mode\":\"historical_conclusion\""));
        assert!(serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record));
    }

    #[test]
    fn historical_neighbor_records_round_trip_bound_cursor_fields() {
        let generation = GenerationId::derive(&[b"neighbor-generation"]);
        let endpoint = syntaxmesh_core::NodeId::derive(&[b"neighbor-endpoint"]);
        let neighbor = syntaxmesh_core::Node {
            id: syntaxmesh_core::NodeId::derive(&[b"neighbor-target"]),
            kind: syntaxmesh_core::NodeKind::Function,
            name: "target".to_owned(),
            owner_file: None,
            source: None,
            provenance: syntaxmesh_core::ProvenanceId::derive(&[b"neighbor-provenance"]),
            extension_payload: None,
        };
        let edge = Edge {
            id: EdgeId::derive(&[b"neighbor-edge"]),
            source: endpoint,
            target: neighbor.id,
            relation: syntaxmesh_core::RelationKind::Calls,
            provenance: neighbor.provenance,
            extension_payload: None,
        };
        let cursor = HistoricalNeighborCursor {
            generation,
            endpoint,
            direction: EdgeDirection::Outgoing,
            after_edge: edge.id,
        };
        let item = TemporalRecord::HistoricalNeighbor {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            generation,
            endpoint,
            direction: EdgeDirection::Outgoing,
            edge,
            neighbor,
        };
        let footer = TemporalRecord::HistoricalNeighborFooter {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            generation,
            endpoint,
            direction: EdgeDirection::Outgoing,
            returned: 1,
            has_more: true,
            next_cursor: Some(cursor),
        };
        for record in [item, footer] {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"schema_version\":11"));
            assert!(
                serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record)
            );
        }
    }

    #[test]
    fn historical_neighborhood_records_round_trip_depth_and_scan_summary() {
        let generation = GenerationId::derive(&[b"neighborhood-generation"]);
        let provenance = syntaxmesh_core::ProvenanceId::derive(&[b"neighborhood-provenance"]);
        let node = Node {
            id: syntaxmesh_core::NodeId::derive(&[b"neighborhood-node"]),
            kind: syntaxmesh_core::NodeKind::Function,
            name: "selected".to_owned(),
            owner_file: None,
            source: None,
            provenance,
            extension_payload: None,
        };
        let edge = Edge {
            id: EdgeId::derive(&[b"neighborhood-edge"]),
            source: node.id,
            target: syntaxmesh_core::NodeId::derive(&[b"neighborhood-target"]),
            relation: syntaxmesh_core::RelationKind::Calls,
            provenance,
            extension_payload: None,
        };
        let records = [
            TemporalRecord::HistoricalNeighborhoodNode {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                depth: 0,
                node,
            },
            TemporalRecord::HistoricalNeighborhoodEdge {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                depth: 1,
                edge,
            },
            TemporalRecord::HistoricalNeighborhoodFooter {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                nodes: 1,
                edges: 1,
                scanned_incidence_entries: 2,
                serialized_item_bytes: 512,
                truncated: false,
            },
        ];
        for record in records {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"schema_version\":1"));
            assert!(
                serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record)
            );
        }
    }

    #[test]
    fn consequence_neighborhood_records_round_trip_with_schema_version() {
        let generation = GenerationId::derive(&[b"consequence-generation"]);
        let edge = syntaxmesh_core::ConsequenceEdge {
            id: syntaxmesh_core::ConsequenceEdgeId::derive(&[b"consequence-edge"]),
            source: syntaxmesh_core::LineageEndpoint::ChangeEvent(
                syntaxmesh_core::ChangeEventId::derive(&[b"event"]),
            ),
            target: syntaxmesh_core::LineageEndpoint::FactVersion(FactVersionRef {
                fact: syntaxmesh_core::FactRef::Node(syntaxmesh_core::NodeId::derive(&[b"node"])),
                valid_from: generation,
            }),
            kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
            evidence: vec![FactVersionRef {
                fact: syntaxmesh_core::FactRef::Node(syntaxmesh_core::NodeId::derive(&[
                    b"evidence-node",
                ])),
                valid_from: generation,
            }],
            derivation: syntaxmesh_core::ConsequenceDerivation::Explicit,
            provenance: syntaxmesh_core::ProvenanceId::derive(&[b"provenance"]),
        };
        let records = [
            TemporalRecord::ConsequenceEdge {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                depth: 1,
                edge,
            },
            TemporalRecord::ConsequenceNeighborhoodFooter {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                endpoints: 2,
                edges: 1,
                scanned_incidences: 1,
                truncated: false,
            },
        ];
        for record in records {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"schema_version\":11"));
            assert!(serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|p| p == record));
        }
    }

    #[test]
    fn consequence_trace_records_round_trip_with_independent_schema_version() {
        let from = GenerationId::derive(&[b"trace-from"]);
        let until = GenerationId::derive(&[b"trace-until"]);
        let source_id = syntaxmesh_core::NodeId::derive(&[b"trace-source"]);
        let target_id = syntaxmesh_core::NodeId::derive(&[b"trace-target"]);
        let source = syntaxmesh_core::LineageEndpoint::FactVersion(FactVersionRef {
            fact: syntaxmesh_core::FactRef::Node(source_id),
            valid_from: from,
        });
        let target = syntaxmesh_core::LineageEndpoint::FactVersion(FactVersionRef {
            fact: syntaxmesh_core::FactRef::Node(target_id),
            valid_from: from,
        });
        let version = ConsequenceEdgeVersion {
            edge: syntaxmesh_core::ConsequenceEdge {
                id: syntaxmesh_core::ConsequenceEdgeId::derive(&[b"trace-edge"]),
                source,
                target,
                kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
                evidence: vec![FactVersionRef {
                    fact: syntaxmesh_core::FactRef::Node(source_id),
                    valid_from: from,
                }],
                derivation: syntaxmesh_core::ConsequenceDerivation::Explicit,
                provenance: syntaxmesh_core::ProvenanceId::derive(&[b"trace-provenance"]),
            },
            valid_from: from,
            valid_until: Some(until),
        };
        let records = [
            TemporalRecord::ConsequenceTraceHeader {
                schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                origin: source,
                from_generation: from,
                until_generation: until,
            },
            TemporalRecord::ConsequenceTraceState {
                schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                endpoint: target,
                reached_generation: until,
                depth: 1,
            },
            TemporalRecord::ConsequenceTraceHop {
                schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                depth: 1,
                source_reached_generation: from,
                reached_generation: from,
                version,
            },
            TemporalRecord::ConsequenceTraceFooter {
                schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                states: 2,
                hops: 1,
                scanned_incidences: 1,
                truncated: false,
            },
        ];
        for record in records {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"schema_version\":1"));
            assert!(serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|p| p == record));
        }
    }

    #[test]
    fn typed_fact_history_round_trips_edge_payload_and_interval() {
        let edge = Edge {
            id: syntaxmesh_core::EdgeId::derive(&[b"edge"]),
            source: syntaxmesh_core::NodeId::derive(&[b"source"]),
            target: syntaxmesh_core::NodeId::derive(&[b"target"]),
            relation: syntaxmesh_core::RelationKind::Calls,
            provenance: syntaxmesh_core::ProvenanceId::derive(&[b"provenance"]),
            extension_payload: None,
        };
        let record = TemporalRecord::FactVersion {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            valid_from: GenerationId::derive(&[b"from"]),
            valid_until: Some(GenerationId::derive(&[b"until"])),
            observed_at: Some(ObservationTime(123)),
            accepted_at: Some(AcceptanceTime(456)),
            payload: FactPayload::Edge(edge),
        };
        let line = record.to_json_line().unwrap_or_default();
        assert!(line.contains("\"query_mode\":\"historical_conclusion\""));
        assert!(line.contains("\"type\":\"fact_version\""));
        assert!(serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record));
    }

    #[test]
    fn same_identity_lineage_record_round_trips_both_fact_versions() {
        let fact = syntaxmesh_core::FactRef::Node(syntaxmesh_core::NodeId::derive(&[b"node"]));
        let record = TemporalRecord::FactSupersedes {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            prior: FactVersionRef {
                fact,
                valid_from: GenerationId::derive(&[b"generation-one"]),
            },
            current: FactVersionRef {
                fact,
                valid_from: GenerationId::derive(&[b"generation-two"]),
            },
            accepted_at: Some(AcceptanceTime(123)),
        };
        let line = record.to_json_line().unwrap_or_default();
        assert!(line.contains("\"schema_version\":11"));
        assert!(line.contains("\"type\":\"fact_supersedes\""));
        assert!(serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record));
    }

    #[test]
    fn change_event_and_pagination_footer_round_trip() {
        let generation = GenerationId::derive(&[b"event-generation"]);
        let fact = syntaxmesh_core::FactRef::Node(syntaxmesh_core::NodeId::derive(&[b"node"]));
        let event = syntaxmesh_core::ChangeEvent {
            id: syntaxmesh_core::ChangeEventId::derive(&[b"event"]),
            repository: RepositoryId::derive(&[b"repository"]),
            worktree: syntaxmesh_core::WorktreeId::derive(&[b"worktree"]),
            generation_before: None,
            generation_after: generation,
            changed_facts: vec![syntaxmesh_core::ChangedFact {
                fact,
                kind: syntaxmesh_core::FactChangeKind::Added,
            }],
        };
        let records = [
            TemporalRecord::ChangeEvent {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                event,
            },
            TemporalRecord::ChangeEventFooter {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                returned: 1,
                has_more: true,
                next_cursor: Some(syntaxmesh_core::ChangeEventCursor { generation }),
            },
        ];
        for record in records {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"schema_version\":11"));
            assert!(line.contains("\"query_mode\":\"historical_conclusion\""));
            assert!(
                serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record)
            );
        }
    }

    #[test]
    fn explicit_change_set_event_and_snapshot_cursor_round_trip() {
        let generation = GenerationId::derive(&[b"changeset-generation"]);
        let change_set = syntaxmesh_core::ChangeSetId::derive(&[b"change-set"]);
        let event_id = syntaxmesh_core::ChangeEventId::derive(&[b"change-event"]);
        let event = syntaxmesh_core::ChangeEvent {
            id: event_id,
            repository: RepositoryId::derive(&[b"repository"]),
            worktree: syntaxmesh_core::WorktreeId::derive(&[b"worktree"]),
            generation_before: None,
            generation_after: generation,
            changed_facts: Vec::new(),
        };
        let declaration = ChangeSet {
            id: change_set,
            kind: syntaxmesh_core::ChangeSetKind::ManualGroup,
            title: Some("group".to_owned()),
            originating_intent: None,
            parent_changes: Vec::new(),
            git_commits: Vec::new(),
            pull_requests: Vec::new(),
            issues: Vec::new(),
            adrs: Vec::new(),
            repositories: vec![event.repository],
            first_generation: generation,
            last_generation: Some(generation),
            provenance: syntaxmesh_core::ProvenanceId::derive(&[b"assertion"]),
        };
        let records = [
            TemporalRecord::ChangeSetEvent {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                item: ChangeSetEvent {
                    membership: syntaxmesh_core::ChangeSetMembership {
                        change_set,
                        event: event_id,
                        provenance: syntaxmesh_core::ProvenanceId::derive(&[b"assertion"]),
                    },
                    event,
                    membership_valid_from: generation,
                },
            },
            TemporalRecord::ChangeSetEventsFooter {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                returned: 1,
                has_more: true,
                next_cursor: Some(EventsForChangeSetCursor {
                    change_set,
                    as_of_generation: generation,
                    after_event_generation: generation,
                }),
            },
            TemporalRecord::ChangeSetVersion {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                change_set: declaration,
                valid_from: generation,
                valid_until: None,
            },
        ];
        for record in records {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"schema_version\":11"));
            assert!(line.contains("\"query_mode\":\"historical_conclusion\""));
            assert!(
                serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record)
            );
        }
    }

    #[test]
    fn change_event_correlation_round_trips_evidence_and_source_bound_cursor() {
        let fact = syntaxmesh_core::FactRef::Node(syntaxmesh_core::NodeId::derive(&[b"node"]));
        let source_event = syntaxmesh_core::ChangeEventId::derive(&[b"source-event"]);
        let target_event = syntaxmesh_core::ChangeEventId::derive(&[b"target-event"]);
        let generation = GenerationId::derive(&[b"target-generation"]);
        let records = [
            TemporalRecord::ChangeEventCorrelation {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                correlation: ChangeEventCorrelation {
                    kind: syntaxmesh_core::ChangeEventRelationKind::HistoricallyCorrelated,
                    source_event,
                    source_generation: GenerationId::derive(&[b"source-generation"]),
                    target_event,
                    target_generation: generation,
                    shared_fact: fact,
                },
            },
            TemporalRecord::ChangeEventCorrelationFooter {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                returned: 1,
                has_more: true,
                next_cursor: Some(ChangeEventCorrelationCursor {
                    source_event,
                    after_generation: generation,
                }),
            },
        ];
        for record in records {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"schema_version\":11"));
            assert!(line.contains("\"query_mode\":\"historical_conclusion\""));
            assert!(
                serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record)
            );
        }
    }

    #[test]
    fn observed_timeline_page_round_trips_distinct_times_and_cursor() {
        let node_id = syntaxmesh_core::NodeId::derive(&[b"node"]);
        let fact = syntaxmesh_core::FactRef::Node(node_id);
        let cursor = ObservedFactCursor {
            observed_at: ObservationTime(123),
            fact,
            valid_from: GenerationId::derive(&[b"generation"]),
        };
        let records = [
            TemporalRecord::ObservedFact {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::ObservationTimeline,
                fact,
                valid_from: cursor.valid_from,
                valid_until: None,
                observed_at: cursor.observed_at,
                accepted_at: Some(AcceptanceTime(456)),
                payload: FactPayload::Node(Node {
                    id: node_id,
                    kind: syntaxmesh_core::NodeKind::RuntimeObservation,
                    name: "observed".to_owned(),
                    owner_file: None,
                    source: None,
                    provenance: syntaxmesh_core::ProvenanceId::derive(&[b"provenance"]),
                    extension_payload: None,
                }),
            },
            TemporalRecord::ObservedTimelineFooter {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::ObservationTimeline,
                returned: 1,
                has_more: true,
                next_cursor: Some(cursor),
            },
        ];
        for record in records {
            let line = record.to_json_line().unwrap_or_default();
            assert!(line.contains("\"query_mode\":\"observation_timeline\""));
            assert!(
                serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record)
            );
        }
    }

    #[test]
    fn acceptance_timeline_record_round_trips_with_independent_schema_version() {
        let generation = GenerationId::derive(&[b"accepted-generation"]);
        let record = TemporalRecord::AcceptedGeneration {
            schema_version: ACCEPTANCE_TIMELINE_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::AcceptanceTimeline,
            accepted_at: AcceptanceTime(1_725_000_000_000_000_000),
            manifest: GenerationManifest {
                repository: RepositoryId::derive(&[b"accepted-repository"]),
                worktree: syntaxmesh_core::WorktreeId::derive(&[b"accepted-worktree"]),
                generation,
                parent: None,
                graph_root: [7; 32],
                configuration_hash: [0; 32],
                extractor_set_hash: [0; 32],
                schema_version: 1,
                status: syntaxmesh_core::GenerationStatus::Durable,
            },
        };
        let line = record.to_json_line().unwrap_or_default();
        assert!(line.contains("\"schema_version\":2"));
        assert!(line.contains("\"query_mode\":\"acceptance_timeline\""));
        assert!(serde_json::from_str::<TemporalRecord>(&line).is_ok_and(|parsed| parsed == record));
    }
}
