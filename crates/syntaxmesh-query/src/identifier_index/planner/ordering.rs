//! Reuse the diagnostic family-channel refill algorithm (ADR-0284/0293).
use std::collections::BTreeSet;
use syntaxmesh_core::NodeId;

pub(super) fn refill(channels: &[Vec<NodeId>], limit: usize) -> Vec<NodeId> {
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
