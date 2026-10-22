use super::{invoke, text};
use std::path::Path;
use syntaxmesh_core::GenerationId;
use syntaxmesh_inference::Claim;
use syntaxmesh_inference_sqlite::{StreamOrigin, StreamPage};

pub(super) fn exercise(
    database: &Path,
    receipt: &Path,
    origin: StreamOrigin,
    closed: GenerationId,
    expected: &[Claim],
) -> Result<(), Box<dyn std::error::Error>> {
    if expected.len() < 2 {
        return Err("pagination fixture needs multiple claims".into());
    }
    let generation = origin.baseline.selection.generation.0.to_hex();
    let cursor_path = database.with_extension("cursor.json");
    let mut continuation = false;
    let mut claims = Vec::new();
    for _attempt in 0..expected.len().saturating_add(1) {
        let mut arguments = vec![
            "inference-claims-page",
            text(database)?,
            &generation,
            text(receipt)?,
            "1",
            "16384",
            "1000",
            "4096",
        ];
        if continuation {
            arguments.push(text(&cursor_path)?);
        }
        let output = invoke(&arguments)?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        let page: StreamPage = serde_json::from_slice(&output.stdout)?;
        if page.checkpoint.selection != origin.baseline.selection || page.claims.len() > 1 {
            return Err("page changed historical selection or admission".into());
        }
        claims.extend(page.claims);
        if let Some(cursor) = page.next {
            let mut substituted = cursor;
            substituted.root = None;
            std::fs::write(&cursor_path, serde_json::to_vec(&substituted)?)?;
            let mut forged_arguments = vec![
                "inference-claims-page",
                text(database)?,
                &generation,
                text(receipt)?,
                "1",
                "16384",
                "1000",
                "4096",
            ];
            forged_arguments.push(text(&cursor_path)?);
            let rejected = invoke(&forged_arguments)?;
            if rejected.status.success() || !rejected.stdout.is_empty() {
                return Err("substituted cursor emitted a historical page".into());
            }
            std::fs::write(&cursor_path, serde_json::to_vec(&cursor)?)?;
            continuation = true;
        } else {
            continuation = false;
            break;
        }
    }
    if continuation || claims != expected {
        return Err("pages did not drain exact historical claims".into());
    }
    let closed_generation = closed.0.to_hex();
    let closed_output = invoke(&[
        "inference-claims-page",
        text(database)?,
        &closed_generation,
        text(receipt)?,
        "1",
        "16384",
        "1000",
        "4096",
    ])?;
    if !closed_output.status.success() {
        return Err(String::from_utf8_lossy(&closed_output.stderr)
            .into_owned()
            .into());
    }
    let closed_page: StreamPage = serde_json::from_slice(&closed_output.stdout)?;
    if !closed_page.claims.is_empty()
        || closed_page.next.is_some()
        || closed_page.checkpoint.selection.generation != closed
    {
        return Err("retracted page exposed inactive claims".into());
    }
    Ok(())
}
