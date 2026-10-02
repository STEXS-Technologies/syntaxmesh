use reqwest::blocking::Client;
use serde_json::Value;
use std::error::Error;
use syntaxmesh_core::{FileVersion, GenerationId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store_turso::TursoGraphStore;

pub(super) fn verify(
    client: &Client,
    base: &str,
    engine: &SyntaxMeshEngine<TursoGraphStore, RustExtractor>,
    generations: [GenerationId; 2],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let url = format!("{base}/api/v1/files");
    for generation in generations {
        let manifest = engine.query(generation).manifest()?;
        let mut after = None;
        let mut requests = 0usize;
        loop {
            requests = requests.saturating_add(1);
            if requests > 100 {
                return Err("file continuation did not terminate".into());
            }
            let expected = engine.query(generation).historical_files_page(after, 1)?;
            let mut request = client.get(&url).query(&[
                ("generation", generation.0.to_hex()),
                ("limit", "1".to_owned()),
            ]);
            if let Some(file) = after {
                request = request.query(&[("after_file", file.0.to_hex())]);
            }
            let response = super::require_status(request.send()?, 200)?;
            let data = response.get("data").ok_or("file page data missing")?;
            let files: Vec<FileVersion> =
                serde_json::from_value(data.get("items").cloned().ok_or("file items missing")?)?;
            let next = if expected.has_more {
                expected.items.last().map(|file| file.file_id.0.to_hex())
            } else {
                None
            };
            if files != expected.items
                || data.get("has_more").and_then(Value::as_bool) != Some(expected.has_more)
                || data.get("next_after").and_then(Value::as_str) != next.as_deref()
                || data.get("repository").and_then(Value::as_str)
                    != Some(manifest.repository.0.to_hex().as_str())
                || data.get("worktree").and_then(Value::as_str)
                    != Some(manifest.worktree.0.to_hex().as_str())
                || response.get("generation").and_then(Value::as_str)
                    != Some(generation.0.to_hex().as_str())
            {
                return Err("HTTP file inventory differs from retained Engine query".into());
            }
            after = expected.items.last().map(|file| file.file_id);
            if !expected.has_more {
                break;
            }
        }
        if Some(&generation) == generations.first() && requests < 2 {
            return Err("retained inventory fixture did not page".into());
        }
    }
    for (key, value) in [
        ("limit", "0"),
        ("limit", "101"),
        ("limit", "bad"),
        ("generation", "bad"),
        ("after_file", "bad"),
        ("unknown", "x"),
    ] {
        super::require_status(client.get(&url).query(&[(key, value)]).send()?, 400)?;
    }
    super::require_status(
        client
            .get(&url)
            .query(&[("after_file", "a".repeat(64))])
            .send()?,
        400,
    )?;
    super::require_status(
        client
            .get(&url)
            .query(&[("generation", "f".repeat(64))])
            .send()?,
        404,
    )?;
    Ok(())
}
