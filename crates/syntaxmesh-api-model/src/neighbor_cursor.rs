use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::HistoricalNeighborCursor;

const MAX_CURSOR_BYTES: usize = 2048;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u32,
    cursor: HistoricalNeighborCursor,
}

/// Invalid or oversized versioned neighbor continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeighborCursorError;

impl std::fmt::Display for NeighborCursorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid neighbor cursor")
    }
}

impl std::error::Error for NeighborCursorError {}

/// Encode the existing version-1 opaque neighbor continuation.
///
/// # Errors
/// Returns an error if serialization fails or the encoded token exceeds 2048 bytes.
pub fn encode_neighbor_cursor(
    cursor: HistoricalNeighborCursor,
) -> Result<String, NeighborCursorError> {
    let bytes =
        serde_json::to_vec(&Cursor { version: 1, cursor }).map_err(|_error| NeighborCursorError)?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    if token.len() > MAX_CURSOR_BYTES {
        return Err(NeighborCursorError);
    }
    Ok(token)
}

/// Decode a bounded version-1 continuation; callers validate query coordinates.
///
/// # Errors
/// Rejects empty, oversized, malformed, or unsupported-version tokens.
pub fn decode_neighbor_cursor(
    token: &str,
) -> Result<HistoricalNeighborCursor, NeighborCursorError> {
    if token.is_empty() || token.len() > MAX_CURSOR_BYTES {
        return Err(NeighborCursorError);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_error| NeighborCursorError)?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_error| NeighborCursorError)?;
    if cursor.version != 1 {
        return Err(NeighborCursorError);
    }
    Ok(cursor.cursor)
}

#[cfg(test)]
mod tests;
