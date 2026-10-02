use std::collections::BTreeSet;
use syntaxmesh_core::{NodeId, NodeKind};

pub(super) fn classify(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::File
        | NodeKind::Module
        | NodeKind::Function
        | NodeKind::Struct
        | NodeKind::Enum
        | NodeKind::Trait
        | NodeKind::Test
        | NodeKind::Class
        | NodeKind::Script => "code",
        NodeKind::Document | NodeKind::Section | NodeKind::DocumentChunk => "documentation",
        NodeKind::Reference { .. }
        | NodeKind::UnresolvedReference { .. }
        | NodeKind::AmbiguousReference { .. }
        | NodeKind::Import { .. }
        | NodeKind::Export { .. } => "reference",
        NodeKind::Repository
        | NodeKind::External { .. }
        | NodeKind::RuntimeObservation
        | NodeKind::ModuleResolutionDiagnostic { .. } => "other",
    }
}

/// Change diagnostic priority only; preserve membership and each group's order.
pub(super) fn primary_first(ids: &mut [NodeId], primary: &BTreeSet<NodeId>) {
    ids.sort_by_key(|id| !primary.contains(id));
}

pub(super) fn balance_discovery(plan: &[NodeId], original: &BTreeSet<NodeId>) -> Vec<NodeId> {
    let lexical = plan
        .iter()
        .filter(|id| original.contains(id))
        .copied()
        .collect();
    let discovered = plan
        .iter()
        .filter(|id| !original.contains(id))
        .copied()
        .collect();
    refill_plan(&[lexical, discovered], plan.len())
}

/// Diagnostic composition only. Inputs remain relevance-ordered; duplicates
/// consume no slot and exhausted channels do not prevent the others refilling.
pub(super) fn refill(channels: &[Vec<NodeId>], limit: usize) -> Vec<NodeId> {
    refill_plan(channels, limit.min(32))
}

pub(super) fn refill_plan(channels: &[Vec<NodeId>], limit: usize) -> Vec<NodeId> {
    let mut cursors = channels
        .iter()
        .map(|channel| channel.iter())
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    while selected.len() < limit.min(256) {
        let mut advanced = false;
        for cursor in &mut cursors {
            for id in cursor.by_ref() {
                advanced = true;
                if seen.insert(*id) {
                    selected.push(*id);
                    break;
                }
            }
            if selected.len() == limit.min(256) {
                return selected;
            }
        }
        if !advanced {
            break;
        }
    }
    selected
}

#[cfg(test)]
#[path = "families/tests.rs"]
mod tests;
