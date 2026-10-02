//! Deterministic change-event projection helpers shared by store backends.

use std::collections::{BTreeMap, BTreeSet};

use syntaxmesh_core::{
    ChangeEvent, ChangeEventId, ChangedFact, FactChangeKind, FactRef, GenerationHistoryEntry,
    GraphDelta, GraphSnapshot, IndexRunId, NodeKind,
};

/// Storage projection code for a module declaration.
pub const NODE_KIND_CODE_MODULE: i64 = 3;
/// Storage projection code for an import occurrence.
pub const NODE_KIND_CODE_IMPORT: i64 = 15;
/// Storage projection code for an export occurrence.
pub const NODE_KIND_CODE_EXPORT: i64 = 16;
/// Storage projection code for a module-resolution diagnostic.
pub const NODE_KIND_CODE_MODULE_RESOLUTION_DIAGNOSTIC: i64 = 17;
/// Storage projection code for a Bash script.
pub const NODE_KIND_CODE_SCRIPT: i64 = 18;
/// Storage projection code for a documentation root.
pub const NODE_KIND_CODE_DOCUMENT: i64 = 19;
/// Storage projection code for a documentation section.
pub const NODE_KIND_CODE_SECTION: i64 = 20;
/// Storage projection code for a source-grounded documentation text block.
pub const NODE_KIND_CODE_DOCUMENT_CHUNK: i64 = 21;

/// Stable, database-private discriminator used by SQL node-kind projections.
///
/// These codes are independent of Rust enum discriminants and bincode variant
/// order. Zero is reserved for unclassified legacy rows; assign new codes
/// explicitly and never reuse existing values.
#[must_use]
pub const fn node_kind_storage_code(kind: &NodeKind) -> i64 {
    match kind {
        NodeKind::Repository => 1,
        NodeKind::File => 2,
        NodeKind::Module => NODE_KIND_CODE_MODULE,
        NodeKind::Function => 4,
        NodeKind::Struct => 5,
        NodeKind::Enum => 6,
        NodeKind::Trait => 7,
        NodeKind::Test => 8,
        NodeKind::External { .. } => 9,
        NodeKind::Reference { .. } => 10,
        NodeKind::UnresolvedReference { .. } => 11,
        NodeKind::AmbiguousReference { .. } => 12,
        NodeKind::RuntimeObservation => 13,
        NodeKind::Class => 14,
        NodeKind::Import { .. } => NODE_KIND_CODE_IMPORT,
        NodeKind::Export { .. } => NODE_KIND_CODE_EXPORT,
        NodeKind::ModuleResolutionDiagnostic { .. } => NODE_KIND_CODE_MODULE_RESOLUTION_DIAGNOSTIC,
        NodeKind::Script => NODE_KIND_CODE_SCRIPT,
        NodeKind::Document => NODE_KIND_CODE_DOCUMENT,
        NodeKind::Section => NODE_KIND_CODE_SECTION,
        NodeKind::DocumentChunk => NODE_KIND_CODE_DOCUMENT_CHUNK,
    }
}

use crate::{GraphStore, InMemoryGraphStore, StoreError};

/// Stable SQL family key and identity bytes for one canonical fact reference.
#[must_use]
pub fn fact_storage_key(fact: FactRef) -> (i64, Vec<u8>) {
    match fact {
        FactRef::File(id) => (0, id.0.0.to_vec()),
        FactRef::Provenance(id) => (1, id.0.0.to_vec()),
        FactRef::Node(id) => (2, id.0.0.to_vec()),
        FactRef::Edge(id) => (3, id.0.0.to_vec()),
    }
}

/// Stable SQL code for one fact transition classification.
#[must_use]
pub const fn fact_change_kind_code(kind: FactChangeKind) -> i64 {
    match kind {
        FactChangeKind::Added => 0,
        FactChangeKind::Updated => 1,
        FactChangeKind::Removed => 2,
    }
}

/// Build the deterministic change-event projection for retained reference
/// history. Durable backends use this only for migration/backfill.
///
/// # Errors
/// Returns an integrity error if replay diverges from a retained generation,
/// or a serialization/backend error if an event cannot be derived.
pub fn change_events_from_history(
    history: &[GenerationHistoryEntry],
) -> Result<Vec<ChangeEvent>, StoreError> {
    let mut replay = InMemoryGraphStore::new();
    let mut events = Vec::new();
    for entry in history {
        if entry.anchor.is_some() {
            replay.restore_history_anchor(entry)?;
            if entry.delta.is_none() {
                continue;
            }
        }
        if let Some(delta) = &entry.delta {
            let event = replay.change_event_for_delta(delta)?;
            let accepted = replay.apply_delta(delta.clone())?;
            if accepted.generation != entry.manifest.generation
                || accepted.graph_root != entry.manifest.graph_root
            {
                return Err(StoreError::Integrity(
                    "change-event backfill replay differs from its generation manifest".to_owned(),
                ));
            }
            events.push(event);
        } else if entry.anchor.is_none() {
            return Err(StoreError::Integrity(
                "history entry has neither a delta nor an anchor".to_owned(),
            ));
        }
    }
    Ok(events)
}

/// Build the event projection for one accepted delta without inferring
/// relationships between different facts.
///
/// # Errors
/// Returns an encoding error if the canonical transition digest cannot be
/// produced.
pub fn change_event(delta: &GraphDelta, before: &GraphSnapshot) -> Result<ChangeEvent, StoreError> {
    let mut prior_facts = BTreeSet::new();
    prior_facts.extend(before.files.iter().map(|item| FactRef::File(item.file_id)));
    prior_facts.extend(
        before
            .provenance
            .iter()
            .map(|item| FactRef::Provenance(item.id)),
    );
    prior_facts.extend(before.nodes.iter().map(|item| FactRef::Node(item.id)));
    prior_facts.extend(before.edges.iter().map(|item| FactRef::Edge(item.id)));
    change_event_from_prior(delta, &prior_facts, &before.edges)
}

/// List directly named fact identities in a delta. Implicit edge cascades are
/// supplied separately from the backend's indexed incident-edge lookup.
#[must_use]
pub fn delta_fact_candidates(delta: &GraphDelta) -> BTreeSet<FactRef> {
    let mut facts = BTreeSet::new();
    facts.extend(
        delta
            .changed_files
            .iter()
            .map(|item| FactRef::File(item.file_id)),
    );
    facts.extend(delta.removed_files.iter().copied().map(FactRef::File));
    facts.extend(
        delta
            .upsert_provenance
            .iter()
            .map(|item| FactRef::Provenance(item.id)),
    );
    facts.extend(delta.upsert_nodes.iter().map(|item| FactRef::Node(item.id)));
    facts.extend(delta.remove_nodes.iter().copied().map(FactRef::Node));
    facts.extend(delta.upsert_edges.iter().map(|item| FactRef::Edge(item.id)));
    facts.extend(delta.remove_edges.iter().copied().map(FactRef::Edge));
    facts
}

/// Build an event from presence facts loaded by targeted identity queries and
/// indexed incident-edge lookup. Ordinary durable writes need not scan the
/// whole prior graph to form this projection.
///
/// # Errors
/// Returns an encoding error if the canonical transition digest cannot be
/// produced.
pub fn change_event_from_prior(
    delta: &GraphDelta,
    prior_facts: &BTreeSet<FactRef>,
    incident_edges: &[syntaxmesh_core::Edge],
) -> Result<ChangeEvent, StoreError> {
    let mut changes = BTreeMap::<FactRef, (bool, bool)>::new();
    let mut record = |fact: FactRef, old: bool, new: bool| {
        changes.insert(fact, (old, new));
    };

    for file in &delta.changed_files {
        let fact = FactRef::File(file.file_id);
        let old = prior_facts.contains(&fact);
        let removed = delta.removed_files.contains(&file.file_id);
        record(fact, old, !removed);
    }
    for file in &delta.removed_files {
        if !delta
            .changed_files
            .iter()
            .any(|changed| changed.file_id == *file)
        {
            let fact = FactRef::File(*file);
            let old = prior_facts.contains(&fact);
            record(fact, old, false);
        }
    }
    for provenance in &delta.upsert_provenance {
        let fact = FactRef::Provenance(provenance.id);
        let old = prior_facts.contains(&fact);
        record(fact, old, true);
    }
    for node in &delta.upsert_nodes {
        let fact = FactRef::Node(node.id);
        let old = prior_facts.contains(&fact);
        record(fact, old, true);
    }
    for node in &delta.remove_nodes {
        if !delta.upsert_nodes.iter().any(|upsert| upsert.id == *node) {
            let fact = FactRef::Node(*node);
            let old = prior_facts.contains(&fact);
            record(fact, old, false);
        }
    }

    let explicitly_upserted_edges = delta
        .upsert_edges
        .iter()
        .map(|edge| edge.id)
        .collect::<std::collections::BTreeSet<_>>();
    for edge in &delta.upsert_edges {
        let fact = FactRef::Edge(edge.id);
        let old = prior_facts.contains(&fact);
        record(fact, old, true);
    }
    for edge in &delta.remove_edges {
        if !explicitly_upserted_edges.contains(edge) {
            let fact = FactRef::Edge(*edge);
            let old = prior_facts.contains(&fact);
            record(fact, old, false);
        }
    }
    for edge in incident_edges {
        let cascaded = delta
            .remove_nodes
            .iter()
            .any(|node| edge.source == *node || edge.target == *node);
        if cascaded && !explicitly_upserted_edges.contains(&edge.id) {
            record(FactRef::Edge(edge.id), true, false);
        }
    }

    let changed_facts = changes
        .into_iter()
        .filter_map(|(fact, (old, new))| {
            let kind = match (old, new) {
                (false, true) => FactChangeKind::Added,
                (true, true) => FactChangeKind::Updated,
                (true, false) => FactChangeKind::Removed,
                (false, false) => return None,
            };
            Some(ChangedFact { fact, kind })
        })
        .collect();
    let id = change_event_id(delta)?;
    let generation_before = delta.expected_base;
    let generation_after = delta.next_generation;
    Ok(ChangeEvent {
        id,
        repository: delta.repository,
        worktree: delta.worktree,
        generation_before,
        generation_after,
        changed_facts,
    })
}

/// Derive the stable event identity from a graph delta without reconstructing
/// the graph or calculating its changed-fact projection.
///
/// The identity is bound to the canonicalized delta and generation transition;
/// callers validating event references do not need to replay retained history.
///
/// # Errors
/// Returns an encoding error if the delta cannot be canonically serialized.
pub fn change_event_id(delta: &GraphDelta) -> Result<ChangeEventId, StoreError> {
    let mut canonical_delta = delta.clone();
    canonical_delta.run_id = IndexRunId::derive(&[b"canonical-change-event"]);
    canonical_delta
        .changed_files
        .sort_by_key(|item| item.file_id);
    canonical_delta.removed_files.sort_unstable();
    canonical_delta
        .upsert_provenance
        .sort_by_key(|item| item.id);
    canonical_delta.upsert_nodes.sort_by_key(|item| item.id);
    canonical_delta.upsert_edges.sort_by_key(|item| item.id);
    canonical_delta.remove_nodes.sort_unstable();
    canonical_delta.remove_edges.sort_unstable();
    let digest = bincode::serialize(&canonical_delta)
        .map_err(|error| StoreError::Backend(format!("encode canonical graph delta: {error}")))?;
    let delta_digest = blake3::hash(&digest);
    let before_bytes = delta
        .expected_base
        .map_or([0; 32], |generation| generation.0.0);
    Ok(ChangeEventId::derive(&[
        &delta.repository.0.0,
        &delta.worktree.0.0,
        &before_bytes,
        &delta.next_generation.0.0,
        delta_digest.as_bytes(),
    ]))
}

#[cfg(test)]
mod node_kind_code_tests {
    use super::*;
    use syntaxmesh_core::{ExportKind, ImportKind, ModuleResolutionDiagnosticStatus, NodeId};

    #[test]
    fn module_resolution_kinds_use_fixed_storage_codes() {
        let module = NodeKind::Module;
        let import = NodeKind::Import {
            specifier: "./mod".to_owned(),
            kind: ImportKind::Named,
            imported_name: Some("x".to_owned()),
            local_name: Some("x".to_owned()),
            type_only: false,
        };
        let export = NodeKind::Export {
            source_specifier: None,
            kind: ExportKind::Local,
            exported_name: Some("x".to_owned()),
            local_name: Some("x".to_owned()),
            type_only: false,
        };
        let diagnostic = NodeKind::ModuleResolutionDiagnostic {
            occurrence: NodeId::derive(&[b"module-resolution-occurrence"]),
            status: ModuleResolutionDiagnosticStatus::Unresolved,
            candidate_paths: Vec::new(),
        };
        assert_eq!(node_kind_storage_code(&module), NODE_KIND_CODE_MODULE);
        assert_eq!(node_kind_storage_code(&import), NODE_KIND_CODE_IMPORT);
        assert_eq!(node_kind_storage_code(&export), NODE_KIND_CODE_EXPORT);
        assert_eq!(
            node_kind_storage_code(&diagnostic),
            NODE_KIND_CODE_MODULE_RESOLUTION_DIAGNOSTIC
        );
        assert_eq!(
            node_kind_storage_code(&NodeKind::Script),
            NODE_KIND_CODE_SCRIPT
        );
        assert_eq!(
            node_kind_storage_code(&NodeKind::Document),
            NODE_KIND_CODE_DOCUMENT
        );
        assert_eq!(
            node_kind_storage_code(&NodeKind::Section),
            NODE_KIND_CODE_SECTION
        );
        assert_eq!(
            node_kind_storage_code(&NodeKind::DocumentChunk),
            NODE_KIND_CODE_DOCUMENT_CHUNK
        );
    }
}
