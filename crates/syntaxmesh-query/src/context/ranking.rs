use std::cmp::Ordering;
use std::collections::BTreeMap;

use syntaxmesh_api_model::ContextItemKind;
use syntaxmesh_core::{Edge, EdgeId, Node, NodeId, NodeKind, RelationKind};

use super::Candidate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SourceGranularity {
    Local,
    Occurrence,
    Container,
}

impl SourceGranularity {
    pub(super) const fn for_node(kind: &NodeKind) -> Self {
        match kind {
            NodeKind::Reference { .. }
            | NodeKind::UnresolvedReference { .. }
            | NodeKind::AmbiguousReference { .. }
            | NodeKind::Import { .. }
            | NodeKind::Export { .. }
            | NodeKind::ModuleResolutionDiagnostic { .. } => Self::Occurrence,
            NodeKind::File | NodeKind::Module | NodeKind::Script | NodeKind::Document => {
                Self::Container
            }
            NodeKind::Repository
            | NodeKind::Function
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::Test
            | NodeKind::External { .. }
            | NodeKind::RuntimeObservation
            | NodeKind::Class
            | NodeKind::Section
            | NodeKind::DocumentChunk => Self::Local,
        }
    }
}

pub(super) fn candidate_order(left: &Candidate, right: &Candidate) -> Ordering {
    item_kind_order(left.kind)
        .cmp(&item_kind_order(right.kind))
        .then_with(|| right.relevance.cmp(&left.relevance))
        .then_with(|| left.distance.cmp(&right.distance))
        .then_with(|| left.granularity.cmp(&right.granularity))
        .then_with(|| left.key.cmp(&right.key))
}

/// Keep a selected documentation heading with selected prose that makes it
/// relevant, without transferring relevance between unrelated graph nodes.
pub(super) fn document_section_relevance(
    selected_nodes: &BTreeMap<NodeId, (Node, u8, u32)>,
    selected_edges: &BTreeMap<EdgeId, Edge>,
) -> BTreeMap<NodeId, u32> {
    let mut relevance = selected_nodes
        .iter()
        .map(|(id, (_, _, score))| (*id, *score))
        .collect::<BTreeMap<_, _>>();
    for edge in selected_edges.values() {
        apply_document_section_edge(edge, selected_nodes, &mut relevance);
    }
    relevance
}

fn apply_document_section_edge(
    edge: &Edge,
    selected_nodes: &BTreeMap<NodeId, (Node, u8, u32)>,
    relevance: &mut BTreeMap<NodeId, u32>,
) {
    if edge.relation != RelationKind::Contains {
        return;
    }
    let (Some((section, _, _)), Some((chunk, _, chunk_score))) = (
        selected_nodes.get(&edge.source),
        selected_nodes.get(&edge.target),
    ) else {
        return;
    };
    if section.kind == NodeKind::Section
        && chunk.kind == NodeKind::DocumentChunk
        && let Some(section_score) = relevance.get_mut(&section.id)
    {
        *section_score = (*section_score).max(*chunk_score);
    }
}

const fn item_kind_order(kind: ContextItemKind) -> u8 {
    match kind {
        ContextItemKind::SourceEvidence => 0,
        ContextItemKind::Signature => 1,
        ContextItemKind::GraphPath => 2,
        ContextItemKind::Summary => 3,
    }
}
