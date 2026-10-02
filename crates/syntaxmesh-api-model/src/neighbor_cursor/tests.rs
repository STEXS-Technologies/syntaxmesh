use super::*;
use syntaxmesh_core::{EdgeDirection, EdgeId, GenerationId, NodeId};

#[test]
fn existing_cursor_bytes_round_trip_and_invalid_tokens_fail()
-> Result<(), Box<dyn std::error::Error>> {
    let cursor = HistoricalNeighborCursor {
        generation: GenerationId::derive(&[b"cursor-generation"]),
        endpoint: NodeId::derive(&[b"cursor-node"]),
        direction: EdgeDirection::Incoming,
        after_edge: EdgeId::derive(&[b"cursor-edge"]),
    };
    let token = encode_neighbor_cursor(cursor)?;
    let original = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Cursor { version: 1, cursor })?);
    if token != original || decode_neighbor_cursor(&token)? != cursor {
        return Err("cursor wire contract changed".into());
    }
    for invalid in [
        String::new(),
        "!invalid-base64".to_owned(),
        "a".repeat(2049),
        URL_SAFE_NO_PAD.encode(b"not JSON"),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Cursor { version: 2, cursor })?),
        format!("{token}="),
    ] {
        if decode_neighbor_cursor(&invalid).is_ok() {
            return Err("invalid cursor accepted".into());
        }
    }
    Ok(())
}
