#[test]
fn json_counter_is_bounded_without_allocating_output() -> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::json!({"text": "λ\"\n", "value": 42});
    let exact = serde_json::to_vec(&value)?.len();
    if super::json_size(&value, exact)? != Some(exact)
        || super::json_size(&value, exact.saturating_sub(1))?.is_some()
        || super::item_size(&value, exact.saturating_add(1))? != Some(exact.saturating_add(1))
    {
        return Err("JSON counter violated its byte bound or separator accounting".into());
    }
    Ok(())
}
#[test]
fn bounded_json_bytes_accepts_exact_size_and_rejects_one_byte_less()
-> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::json!({"text": "λ\"\n", "items": [1, 2, 3]});
    let expected = serde_json::to_vec(&value)?;
    let bytes = super::bounded_json_bytes(&value, expected.len())
        .map_err(|status| format!("exact byte limit failed: {status}"))?;
    if bytes != expected
        || super::bounded_json_bytes(&value, expected.len().saturating_sub(1))
            != Err(axum::http::StatusCode::PAYLOAD_TOO_LARGE)
    {
        return Err("bounded serializer violated exact output limit".into());
    }
    Ok(())
}
