//! Canonical graph-store ports and reference backends.

mod backend;
mod historical_node_scan;
mod lineage;
mod persistent_fact_tree;
mod persistent_incidence;

pub use lineage::{
    NODE_KIND_CODE_EXPORT, NODE_KIND_CODE_IMPORT, NODE_KIND_CODE_MODULE,
    NODE_KIND_CODE_MODULE_RESOLUTION_DIAGNOSTIC, change_event, change_event_from_prior,
    change_events_from_history, delta_fact_candidates, fact_change_kind_code, fact_storage_key,
    node_kind_storage_code,
};

#[cfg(feature = "benchmark-instrumentation")]
pub use backend::ConsequenceRangeReadMetrics;
#[cfg(feature = "benchmark-instrumentation")]
pub use backend::HistoricalEdgeReadMetrics;
pub use backend::{
    AcceptedGeneration, AcceptedGenerationPage, BackendIntegrityCheck, BackendIntegrityKind,
    BackendIntegrityReport, ChangeEventPage, ChangeSetVersion, ConsequenceEdgePage,
    ConsequenceRangePage, DurableRecordPage, DurableRecordStore, EventsForChangeSetPage,
    FactHistoryCursor, FactHistoryEntry, FactHistoryPage, FactHistoryVersion,
    FactVersionChangeCursor, FactVersionChangePage, FileGraphStore, GenerationChange,
    GenerationChangeCursor, GenerationChangePage, GraphStore, HistoricalEdgePage,
    HistoricalFilePage, HistoricalNodePage, InMemoryGraphStore, MAX_FACT_VERSION_CHANGE_PAGE_SIZE,
    MAX_GENERATION_CHANGE_PAGE_SIZE, MAX_HISTORICAL_EDGE_PAGE_SIZE, MAX_HISTORICAL_FILE_PAGE_SIZE,
    MAX_HISTORICAL_NODE_PAGE_SIZE, NodeHistoryVersion, ObservedFactPage, ObservedFactVersion,
    StoreError, graph_snapshot_root_v2,
};
pub use persistent_fact_tree::{
    PersistentFactKey, PersistentFactMutation, PersistentFactNode, PersistentFactTree,
    PersistentFactTreeApply, PersistentFactTreeCache, PersistentFactTreeError,
    PersistentFactTreeLookup, PersistentFactTreeRangeWalker, PersistentFactTreeWalker,
    generation_root_v2,
};
pub use persistent_incidence::{
    IncidenceMutation, PersistentIncidenceApply, PersistentIncidenceEndpointLookup,
    PersistentIncidenceIndex, PersistentIncidencePageWalker,
};
pub use syntaxmesh_core::{AcceptedGenerationCursor, ObservedFactCursor};
