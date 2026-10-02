use super::*;
use syntaxmesh_api_model::{TEMPORAL_EXPORT_SCHEMA_VERSION, TemporalQueryMode, TemporalRecord};

pub(super) fn verify(
    client: &reqwest::blocking::Client,
    base: &str,
    engine: &SyntaxMeshEngine<TursoGraphStore, RustExtractor>,
    generations: [GenerationId; 2],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let url = format!("{base}/api/v1/fact-history");
    for generation in generations {
        let query = engine.query(generation);
        let manifest = query.manifest()?;
        let mut after = None;
        let mut parameters = vec![
            ("generation".to_owned(), generation.0.to_hex()),
            ("limit".to_owned(), "1".to_owned()),
        ];
        let mut requests = 0usize;
        loop {
            requests = requests.saturating_add(1);
            if requests > 2000 {
                return Err("history continuation did not terminate".into());
            }
            let expected = query.fact_history_page(after, 1)?;
            let response = require_status(client.get(&url).query(&parameters).send()?, 200)?;
            let data = response.get("data").ok_or("history data missing")?;
            let items = data
                .get("items")
                .and_then(Value::as_array)
                .ok_or("history items missing")?;
            if response.get("generation").and_then(Value::as_str)
                != Some(generation.0.to_hex().as_str())
                || data.get("repository").and_then(Value::as_str)
                    != Some(manifest.repository.0.to_hex().as_str())
                || data.get("worktree").and_then(Value::as_str)
                    != Some(manifest.worktree.0.to_hex().as_str())
                || items.len() != expected.items.len()
            {
                return Err("history page scope or count differs".into());
            }
            for (actual, entry) in items.iter().zip(&expected.items) {
                let version = &entry.version;
                let record: TemporalRecord = serde_json::from_value(
                    actual
                        .get("record")
                        .cloned()
                        .ok_or("history record missing")?,
                )?;
                if record
                    != (TemporalRecord::FactVersion {
                        schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                        query_mode: TemporalQueryMode::HistoricalConclusion,
                        valid_from: version.valid_from,
                        valid_until: version.valid_until,
                        observed_at: version.observed_at,
                        accepted_at: version.accepted_at,
                        payload: version.payload.clone(),
                    })
                    || actual.get("fact") != Some(&serde_json::to_value(entry.fact)?)
                    || actual.get("valid_from_sequence").and_then(Value::as_u64)
                        != Some(entry.valid_from_sequence)
                    || actual.get("valid_until_sequence")
                        != Some(&serde_json::to_value(entry.valid_until_sequence)?)
                {
                    return Err("history page changed retained version".into());
                }
            }
            let next = data.get("next_cursor").ok_or("history cursor missing")?;
            after = expected.next_cursor;
            if let Some(cursor) = after {
                if next.get("generation").and_then(Value::as_str)
                    != Some(cursor.as_of_generation.0.to_hex().as_str())
                    || next.get("after_valid_from").and_then(Value::as_str)
                        != Some(cursor.after_valid_from.0.to_hex().as_str())
                {
                    return Err("history cursor changed generation".into());
                }
                parameters = vec![("limit".to_owned(), "1".to_owned())];
                for key in ["generation", "after_kind", "after_id", "after_valid_from"] {
                    parameters.push((
                        key.to_owned(),
                        next.get(key)
                            .and_then(Value::as_str)
                            .ok_or("history cursor field missing")?
                            .to_owned(),
                    ));
                }
            } else {
                if !next.is_null() {
                    return Err("terminal history page has cursor".into());
                }
                break;
            }
        }
        if requests < 2 {
            return Err("history fixture did not page".into());
        }
    }
    for (key, value) in [
        ("limit", "0"),
        ("limit", "101"),
        ("limit", "bad"),
        ("generation", "bad"),
        ("after_kind", "node"),
        ("after_id", "bad"),
        ("after_valid_from", "bad"),
        ("unknown", "x"),
    ] {
        require_status(client.get(&url).query(&[(key, value)]).send()?, 400)?;
    }
    let id = generations[0].0.to_hex();
    require_status(
        client
            .get(&url)
            .query(&[
                ("after_kind", "node"),
                ("after_id", id.as_str()),
                ("after_valid_from", id.as_str()),
            ])
            .send()?,
        400,
    )?;
    require_status(
        client
            .get(&url)
            .query(&[("generation", "f".repeat(64))])
            .send()?,
        404,
    )?;
    Ok(())
}
