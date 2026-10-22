use crate::identifier_terms;
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

/// Deterministic query-relevant line windows attempted only after the complete
/// source item fails exact token-budget admission.
pub(super) fn query_windows(
    source: &str,
    first_line: u32,
    query: &str,
    remaining_excerpt_bytes: &mut usize,
) -> Vec<(String, u32, u32)> {
    const WINDOW_WIDTHS: [usize; 5] = [24, 12, 6, 3, 1];
    const MAX_WINDOW_BYTES: usize = 16 * 1024;
    let query_terms = identifier_terms(query)
        .into_iter()
        .take(16)
        .collect::<Vec<_>>();
    if query_terms.is_empty() || *remaining_excerpt_bytes == 0 {
        return Vec::new();
    }
    #[derive(Clone, Copy, Default)]
    struct Line {
        mask: u16,
        start: usize,
    }
    #[derive(Clone, Copy, Default)]
    struct Window {
        score: u32,
        start_line: usize,
        start_byte: usize,
        end_byte: usize,
        present: bool,
    }

    let mut ring = [Line::default(); 24];
    let mut counts = [[0_u8; 16]; 5];
    let mut masks = [0_u16; 5];
    let mut best = [Window::default(); 5];
    let mut offset = 0_usize;
    let mut line_index = 0_usize;
    for line_with_ending in source.split_inclusive('\n') {
        let source_line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let terms = identifier_terms(source_line);
        let line_mask = query_terms
            .iter()
            .enumerate()
            .filter(|(_, term)| terms.contains(*term))
            .fold(0_u16, |mask, (index, _)| mask | (1_u16 << index));
        let content_end = offset.saturating_add(source_line.len());
        for (window_index, width) in WINDOW_WIDTHS.into_iter().enumerate() {
            if line_index >= width {
                let Some(outgoing_index) = line_index.checked_sub(width) else {
                    continue;
                };
                let Some(outgoing_slot) = outgoing_index.checked_rem(24) else {
                    continue;
                };
                let Some(outgoing_line) = ring.get(outgoing_slot).copied() else {
                    continue;
                };
                let outgoing = outgoing_line.mask;
                let mut bits = outgoing;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    if let Some(window_counts) = counts.get_mut(window_index)
                        && let Some(count) = window_counts.get_mut(bit)
                    {
                        *count = count.saturating_sub(1);
                        if *count == 0
                            && let Some(window_mask) = masks.get_mut(window_index)
                        {
                            *window_mask &= !(1_u16 << bit);
                        }
                    }
                    bits &= bits.saturating_sub(1);
                }
            }
            let mut bits = line_mask;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                if let Some(window_counts) = counts.get_mut(window_index)
                    && let Some(count) = window_counts.get_mut(bit)
                {
                    *count = count.saturating_add(1);
                    if let Some(window_mask) = masks.get_mut(window_index) {
                        *window_mask |= 1_u16 << bit;
                    }
                }
                bits &= bits.saturating_sub(1);
            }
            if line_index.saturating_add(1) < width {
                continue;
            }
            let Some(start_line) = line_index.saturating_add(1).checked_sub(width) else {
                continue;
            };
            let Some(start_slot) = start_line.checked_rem(24) else {
                continue;
            };
            let Some(start) = ring.get(start_slot).map(|entry| entry.start) else {
                continue;
            };
            if content_end.saturating_sub(start) > MAX_WINDOW_BYTES {
                continue;
            }
            let score = masks
                .get(window_index)
                .copied()
                .unwrap_or_default()
                .count_ones();
            if let Some(best_window) = best.get_mut(window_index)
                && score > best_window.score
            {
                *best_window = Window {
                    score,
                    start_line,
                    start_byte: start,
                    end_byte: content_end,
                    present: true,
                };
            }
        }
        let ring_index = line_index.checked_rem(24).unwrap_or_default();
        if let Some(slot) = ring.get_mut(ring_index) {
            *slot = Line {
                mask: line_mask,
                start: offset,
            };
        }
        offset = offset.saturating_add(line_with_ending.len());
        line_index = line_index.saturating_add(1);
    }
    if line_index <= 1 {
        return Vec::new();
    }
    let mut excerpts = Vec::new();
    for (window_index, width) in WINDOW_WIDTHS.into_iter().enumerate() {
        let Some(window) = best.get(window_index).copied() else {
            continue;
        };
        if !window.present || width >= line_index {
            continue;
        }
        let (Ok(start_offset), Ok(end_offset)) = (
            u32::try_from(window.start_line),
            u32::try_from(window.start_line.saturating_add(width).saturating_sub(1)),
        ) else {
            continue;
        };
        let (Some(line_start), Some(line_end)) = (
            first_line.checked_add(start_offset),
            first_line.checked_add(end_offset),
        ) else {
            continue;
        };
        let Some(content) = source.get(window.start_byte..window.end_byte) else {
            continue;
        };
        let mut excerpt = String::new();
        if window.start_line > 0 {
            excerpt.push_str("[…]\n");
        }
        excerpt.push_str(content);
        if window.start_line.saturating_add(width) < line_index {
            excerpt.push_str("\n[…]");
        }
        let excerpt_bytes = excerpt.len();
        if *remaining_excerpt_bytes < excerpt_bytes {
            continue;
        }
        if excerpts
            .last()
            .is_none_or(|(previous, _, _): &(String, u32, u32)| previous != &excerpt)
        {
            *remaining_excerpt_bytes = remaining_excerpt_bytes.saturating_sub(excerpt_bytes);
            excerpts.push((excerpt, line_start, line_end));
        }
    }
    excerpts
}

#[cfg(test)]
mod tests;
