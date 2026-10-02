use syntaxmesh_api_model::ContextPack;
use tiktoken_rs::CoreBPE;

const MAX_MEMO_BYTES: usize = 256 * 1024;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct Memo {
    entries: BTreeMap<String, usize>,
    retained_bytes: usize,
}

impl Memo {
    pub(super) fn count(&mut self, encoding: &CoreBPE, payload: &str) -> usize {
        let Some(ranges) = numeric_ranges(payload) else {
            return encoding.encode_ordinary(payload).len();
        };
        let mut cursor = 0;
        let mut count = 0_usize;
        for (start, end) in ranges {
            for range in [cursor..start, start..end] {
                let Some(piece) = payload.get(range) else {
                    return encoding.encode_ordinary(payload).len();
                };
                let Some(sum) = count.checked_add(self.piece_count(encoding, piece)) else {
                    return encoding.encode_ordinary(payload).len();
                };
                count = sum;
            }
            cursor = end;
        }
        payload
            .get(cursor..)
            .and_then(|piece| count.checked_add(self.piece_count(encoding, piece)))
            .unwrap_or_else(|| encoding.encode_ordinary(payload).len())
    }

    fn piece_count(&mut self, encoding: &CoreBPE, piece: &str) -> usize {
        if let Some(count) = self.entries.get(piece) {
            return *count;
        }
        let count = encoding.encode_ordinary(piece).len();
        if self.entries.len() < 1024
            && piece.len() <= MAX_MEMO_BYTES.saturating_sub(self.retained_bytes)
        {
            self.retained_bytes = self.retained_bytes.saturating_add(piece.len());
            self.entries.insert(piece.to_owned(), count);
        }
        count
    }
}

fn numeric_ranges(payload: &str) -> Option<Vec<(usize, usize)>> {
    if payload.len() > MAX_MEMO_BYTES {
        return None;
    }
    let pack: ContextPack = serde_json::from_str(payload).ok()?;
    if serde_json::to_string(&pack).ok()?.as_str() != payload {
        return None;
    }
    let mut ranges = Vec::new();
    for marker in [
        ",\"token_budget\":",
        ",\"token_count\":",
        "{\"rank\":",
        ",\"line_start\":",
        ",\"line_end\":",
    ] {
        for (position, _) in payload.match_indices(marker) {
            let start = position.checked_add(marker.len())?;
            let rest = payload.get(start..)?;
            let length = rest.bytes().take_while(u8::is_ascii_digit).count();
            if length == 0 {
                continue;
            }
            let end = start.checked_add(length)?;
            if !matches!(payload.as_bytes().get(end), Some(b',' | b'}')) {
                return None;
            }
            ranges.push((start, end));
        }
    }
    ranges.sort_unstable();
    if ranges.is_empty() {
        return None;
    }
    Some(ranges)
}
use std::collections::BTreeMap;
