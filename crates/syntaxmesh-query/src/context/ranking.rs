use std::cmp::Ordering;

use syntaxmesh_api_model::ContextItemKind;
use syntaxmesh_core::NodeKind;

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

const fn item_kind_order(kind: ContextItemKind) -> u8 {
    match kind {
        ContextItemKind::SourceEvidence => 0,
        ContextItemKind::Signature => 1,
        ContextItemKind::GraphPath => 2,
        ContextItemKind::Summary => 3,
    }
}
