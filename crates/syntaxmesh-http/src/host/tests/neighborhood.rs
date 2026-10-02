use std::error::Error;

use reqwest::blocking::{Client, Response};
use serde_json::Value;
use syntaxmesh_core::{Edge, GenerationId, Node, NodeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store_turso::TursoGraphStore;

use super::require_status;

fn bounded_response(
    response: Response,
    limit: usize,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    if !response.status().is_success() {
        return Err(format!("neighborhood failed: {}", response.status()).into());
    }
    let bytes = response.bytes()?;
    if bytes.len() > limit {
        return Err("HTTP JSON body exceeded requested cap".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn verify(
    client: &Client,
    base: &str,
    engine: &SyntaxMeshEngine<TursoGraphStore, RustExtractor>,
    seed: NodeId,
    generations: [GenerationId; 2],
    removed: NodeId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = |id: NodeId| format!("{base}/api/v1/nodes/{}/neighborhood", id.0.to_hex());
    for generation in generations {
        for hops in [1usize, 2, 3] {
            let expected = engine.query(generation).historical_neighborhood(
                &[seed],
                hops,
                64,
                128,
                1024,
                262_144,
            )?;
            let response = bounded_response(
                client
                    .get(path(seed))
                    .query(&[
                        ("generation", generation.0.to_hex()),
                        ("max_hops", hops.to_string()),
                    ])
                    .send()?,
                262_144,
            )?;
            let data = response.get("data").ok_or("missing neighborhood data")?;
            let nodes = data
                .get("nodes")
                .and_then(Value::as_array)
                .ok_or("missing nodes")?;
            let actual_nodes = nodes
                .iter()
                .map(
                    |item| -> Result<(usize, Node), Box<dyn Error + Send + Sync>> {
                        let depth: usize = serde_json::from_value(
                            item.get("depth").cloned().ok_or("missing depth")?,
                        )?;
                        let node: Node = serde_json::from_value(
                            item.get("node").cloned().ok_or("missing node")?,
                        )?;
                        Ok((depth, node))
                    },
                )
                .collect::<Result<Vec<_>, _>>()?;
            let expected_nodes = expected
                .nodes
                .iter()
                .map(
                    |item| -> Result<(usize, Node), Box<dyn Error + Send + Sync>> {
                        Ok((
                            item.depth,
                            engine
                                .query(generation)
                                .historical_node(item.id)?
                                .ok_or("missing retained node")?,
                        ))
                    },
                )
                .collect::<Result<Vec<_>, _>>()?;
            let edges = data
                .get("edges")
                .and_then(Value::as_array)
                .ok_or("missing edges")?;
            let actual_edges = edges
                .iter()
                .map(
                    |item| -> Result<(usize, Edge), Box<dyn Error + Send + Sync>> {
                        Ok((
                            serde_json::from_value(
                                item.get("depth").cloned().ok_or("missing edge depth")?,
                            )?,
                            serde_json::from_value(
                                item.get("edge").cloned().ok_or("missing edge")?,
                            )?,
                        ))
                    },
                )
                .collect::<Result<Vec<_>, _>>()?;
            let expected_edges = expected
                .edges
                .iter()
                .map(|item| (item.depth, item.edge.clone()))
                .collect::<Vec<_>>();
            if actual_nodes != expected_nodes
                || actual_edges != expected_edges
                || data.get("truncated").and_then(Value::as_bool) != Some(expected.truncated)
                || data
                    .get("scanned_incidence_entries")
                    .and_then(Value::as_u64)
                    != u64::try_from(expected.scanned_incidence_entries).ok()
                || response.get("generation").and_then(Value::as_str)
                    != Some(generation.0.to_hex().as_str())
            {
                return Err(
                    "HTTP multi-hop result differs from independent export-path traversal".into(),
                );
            }
        }
    }
    for (key, field) in [
        ("max_nodes", "nodes"),
        ("max_edges", "edges"),
        ("max_scanned_edges", "scanned_incidence_entries"),
    ] {
        let response =
            bounded_response(client.get(path(seed)).query(&[(key, "1")]).send()?, 262_144)?;
        let data = response.get("data").ok_or("missing truncated data")?;
        if data.get("truncated").and_then(Value::as_bool) != Some(true) {
            return Err("work cap failed to report truncation".into());
        }
        if field == "scanned_incidence_entries" {
            if data
                .get(field)
                .and_then(Value::as_u64)
                .is_none_or(|count| count > 1)
            {
                return Err("scan cap exceeded".into());
            }
        } else if data
            .get(field)
            .and_then(Value::as_array)
            .is_none_or(|items| items.len() > 1)
        {
            return Err("item cap exceeded".into());
        }
    }
    let mut byte_truncated = false;
    for cap in [512usize, 1024, 2048, 4096, 8192] {
        let response = client
            .get(path(seed))
            .query(&[("max_result_bytes", cap.to_string())])
            .send()?;
        if response.status().as_u16() == 413 {
            require_status(response, 413)?;
            continue;
        }
        let result = bounded_response(response, cap)?;
        byte_truncated |= result
            .get("data")
            .and_then(|data| data.get("truncated"))
            .and_then(Value::as_bool)
            == Some(true);
    }
    if !byte_truncated {
        return Err("fixture did not exercise successful byte-limited truncation".into());
    }
    for (key, value) in [
        ("max_hops", "0"),
        ("max_hops", "9"),
        ("max_nodes", "0"),
        ("max_nodes", "257"),
        ("max_edges", "0"),
        ("max_edges", "257"),
        ("max_scanned_edges", "0"),
        ("max_scanned_edges", "4097"),
        ("max_result_bytes", "0"),
        ("max_result_bytes", "1048577"),
        ("generation", "bad"),
        ("cursor", "unsupported"),
    ] {
        require_status(client.get(path(seed)).query(&[(key, value)]).send()?, 400)?;
    }
    require_status(
        client
            .get(path(seed))
            .query(&[("max_result_bytes", "1")])
            .send()?,
        413,
    )?;
    require_status(
        client
            .get(path(NodeId::derive(&[b"missing-neighborhood-seed"])))
            .send()?,
        404,
    )?;
    require_status(
        client
            .get(path(seed))
            .query(&[(
                "generation",
                GenerationId::derive(&[b"missing-neighborhood-generation"])
                    .0
                    .to_hex(),
            )])
            .send()?,
        404,
    )?;
    require_status(client.get(path(removed)).send()?, 404)?;
    let old = generations.first().ok_or("missing old generation")?;
    bounded_response(
        client
            .get(path(removed))
            .query(&[("generation", old.0.to_hex())])
            .send()?,
        262_144,
    )?;
    Ok(())
}
