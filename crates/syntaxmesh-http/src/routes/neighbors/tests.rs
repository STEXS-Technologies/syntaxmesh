use axum::http::StatusCode;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use syntaxmesh_api_model::HistoricalNeighborCursor;
use syntaxmesh_core::{EdgeDirection, EdgeId, GenerationId, NodeId};

#[test]
fn cursor_round_trip_and_rejection_are_bounded_and_versioned()
-> Result<(), Box<dyn std::error::Error>> {
    let cursor = HistoricalNeighborCursor {
        generation: GenerationId::derive(&[b"cursor-generation"]),
        endpoint: NodeId::derive(&[b"cursor-node"]),
        direction: EdgeDirection::Incoming,
        after_edge: EdgeId::derive(&[b"cursor-edge"]),
    };
    let token = super::encode_cursor(cursor).map_err(|status| status.to_string())?;
    if super::decode_cursor(&token) != Ok(cursor) {
        return Err("cursor round trip changed typed coordinates".into());
    }
    let wrong_version = URL_SAFE_NO_PAD.encode(serde_json::to_vec(
        &serde_json::json!({"version": 2, "cursor": cursor}),
    )?);
    for invalid in [
        String::new(),
        "!invalid-base64".to_owned(),
        "a".repeat(2049),
        URL_SAFE_NO_PAD.encode(b"not JSON"),
        wrong_version,
        format!("{token}="),
    ] {
        if super::decode_cursor(&invalid) != Err(StatusCode::BAD_REQUEST) {
            return Err("invalid cursor accepted".into());
        }
    }
    Ok(())
}
