use syntaxmesh_api_model::ContextItemKind;

use super::Candidate;

pub(super) fn coalesce(candidates: &mut Vec<Candidate>) {
    let mut retained: Vec<Candidate> = Vec::with_capacity(candidates.len());
    for candidate in candidates.drain(..) {
        let duplicate = retained.iter_mut().find(|existing| {
            candidate.kind == ContextItemKind::SourceEvidence
                && existing.kind == candidate.kind
                && existing.item.text == candidate.item.text
                && existing.item.source_path == candidate.item.source_path
                && existing.item.line_start == candidate.item.line_start
                && existing.item.line_end == candidate.item.line_end
                && existing.item.evidence_class == candidate.item.evidence_class
                && existing.item.edge_ids == candidate.item.edge_ids
        });
        if let Some(existing) = duplicate {
            existing.item.node_ids.extend(candidate.item.node_ids);
            existing.item.node_ids.sort_unstable();
            existing.item.node_ids.dedup();
        } else {
            retained.push(candidate);
        }
    }
    *candidates = retained;
}

#[cfg(test)]
mod tests;
