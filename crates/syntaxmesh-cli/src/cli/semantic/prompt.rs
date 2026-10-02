use serde_json::json;
use syntaxmesh_semantic::SemanticRequest;

pub(super) const ENVELOPE_BYTES: usize = "{\"requests\":[]}".len();

pub(super) fn encode_request(request: &SemanticRequest) -> Result<(String, usize), String> {
    let chunks = request.prompt_chunks();
    let count = chunks.len();
    let chunks = chunks
        .into_iter()
        .map(|chunk| {
            json!({
                "content_hash": hex::encode(chunk.content_hash),
                "section_context": chunk.context,
                "text": chunk.text,
            })
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_string(&json!({"chunks": chunks}))
        .map_err(|_error| "could not encode semantic request".to_owned())?;
    Ok((encoded, count))
}

pub(super) fn encode(requests: &[SemanticRequest]) -> Result<String, String> {
    let fragments = requests
        .iter()
        .map(|request| encode_request(request).map(|(encoded, _)| encoded))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("{{\"requests\":[{}]}}", fragments.join(",")))
}

#[cfg(test)]
mod tests;
