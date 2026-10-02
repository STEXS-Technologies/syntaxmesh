use std::error::Error;

use axum::http::StatusCode;
use serde_json::{Value, json};
use syntaxmesh_core::{Edge, EdgeId, Node, NodeId, NodeKind, ProvenanceId, RelationKind, StableId};

use super::transport::{Guard, exercise_command, exercise_guarded_pages, exercise_pages};

fn item(edge_id: u8, source: u8, target: u8, neighbor_id: u8) -> Value {
    let edge = Edge {
        id: EdgeId(StableId([edge_id; 32])),
        source: NodeId(StableId([source; 32])),
        target: NodeId(StableId([target; 32])),
        relation: RelationKind::Calls,
        provenance: ProvenanceId(StableId([1; 32])),
        extension_payload: None,
    };
    let neighbor = Node {
        id: NodeId(StableId([neighbor_id; 32])),
        kind: NodeKind::Function,
        name: "neighbor".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId(StableId([1; 32])),
        extension_payload: None,
    };
    json!({"edge":edge,"neighbor":neighbor})
}

fn page(
    generation: &str,
    endpoint: &str,
    direction: &str,
    items: Vec<Value>,
    cursor: Option<&str>,
) -> String {
    let items = Value::Array(items);
    json!({"schema_version":1,"generation":generation,"data":{
        "endpoint":endpoint,"direction":direction,"items":items,
        "has_more":cursor.is_some(),"next_cursor":cursor,
    }})
    .to_string()
}

#[test]
fn neighbor_pages_reject_foreign_scope_incidence_order_and_cursor_bounds()
-> Result<(), Box<dyn Error>> {
    let seed = "b".repeat(64);
    let generation = "a".repeat(64);
    let overlong = "x".repeat(2049);
    for body in [
        page(&generation, &"c".repeat(64), "outgoing", vec![], None),
        page(&generation, &seed, "incoming", vec![], None),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(1, 0xcc, 0xdd, 0xdd)],
            None,
        ),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(1, 0xbb, 0xdd, 0xee)],
            None,
        ),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(2, 0xbb, 0xdd, 0xdd), item(1, 0xbb, 0xdd, 0xdd)],
            None,
        ),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(1, 0xbb, 0xdd, 0xdd); 101],
            None,
        ),
        page(&generation, &seed, "outgoing", vec![], Some("cursor")),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(1, 0xbb, 0xdd, 0xdd)],
            Some(""),
        ),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(1, 0xbb, 0xdd, 0xdd)],
            Some(&overlong),
        ),
    ] {
        exercise_command(
            StatusCode::OK,
            Guard::Matching,
            body,
            Some("inconsistent neighbor page"),
            "neighbors-turso",
            &[&seed],
        )?;
    }
    Ok(())
}

#[test]
fn continuations_require_pinned_generation_order_and_nonrepeating_cursor()
-> Result<(), Box<dyn Error>> {
    let seed = "b".repeat(64);
    let generation = "a".repeat(64);
    let first = page(
        &generation,
        &seed,
        "outgoing",
        vec![item(1, 0xbb, 0xdd, 0xdd)],
        Some("cursor-1"),
    );
    for second in [
        page(
            &"c".repeat(64),
            &seed,
            "outgoing",
            vec![item(2, 0xbb, 0xdd, 0xdd)],
            None,
        ),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(1, 0xbb, 0xdd, 0xdd)],
            None,
        ),
        page(
            &generation,
            &seed,
            "outgoing",
            vec![item(2, 0xbb, 0xdd, 0xdd)],
            Some("cursor-1"),
        ),
    ] {
        exercise_pages(
            StatusCode::OK,
            Guard::Matching,
            vec![first.clone(), second],
            Some("inconsistent neighbor page"),
            "neighbors-turso",
            &[&seed],
        )?;
    }
    Ok(())
}

#[test]
fn historical_continuations_reject_malformed_or_foreign_typed_coordinates()
-> Result<(), Box<dyn Error>> {
    use syntaxmesh_api_model::{HistoricalNeighborCursor, encode_neighbor_cursor};
    use syntaxmesh_core::{EdgeDirection, GenerationId};
    let generation = "a".repeat(64);
    let seed = "b".repeat(64);
    let cursor = HistoricalNeighborCursor {
        generation: GenerationId(StableId([0xaa; 32])),
        endpoint: NodeId(StableId([0xbb; 32])),
        direction: EdgeDirection::Outgoing,
        after_edge: EdgeId(StableId([100; 32])),
    };
    let mut foreign = cursor;
    foreign.generation = GenerationId(StableId([0xcc; 32]));
    let mut wrong_endpoint = cursor;
    wrong_endpoint.endpoint = NodeId(StableId([0xcc; 32]));
    let mut wrong_direction = cursor;
    wrong_direction.direction = EdgeDirection::Incoming;
    let mut wrong_edge = cursor;
    wrong_edge.after_edge = EdgeId(StableId([99; 32]));
    for token in [
        "not-a-valid-cursor".to_owned(),
        encode_neighbor_cursor(foreign)?,
        encode_neighbor_cursor(wrong_endpoint)?,
        encode_neighbor_cursor(wrong_direction)?,
        encode_neighbor_cursor(wrong_edge)?,
    ] {
        exercise_command(
            StatusCode::OK,
            Guard::Matching,
            page(
                &generation,
                &seed,
                "outgoing",
                (1..=100).map(|id| item(id, 0xbb, 0xdd, 0xdd)).collect(),
                Some(&token),
            ),
            Some("inconsistent neighbor page"),
            "neighbors-at-turso",
            &[&generation, &seed, "outgoing", "101"],
        )?;
    }
    let good = encode_neighbor_cursor(cursor)?;
    exercise_pages(
        StatusCode::OK,
        Guard::Matching,
        vec![
            page(
                &generation,
                &seed,
                "outgoing",
                (1..=100).map(|id| item(id, 0xbb, 0xdd, 0xdd)).collect(),
                Some(&good),
            ),
            page(
                &"c".repeat(64),
                &seed,
                "outgoing",
                vec![item(101, 0xbb, 0xdd, 0xdd)],
                None,
            ),
        ],
        Some("inconsistent neighbor page"),
        "neighbors-at-turso",
        &[&generation, &seed, "outgoing", "200"],
    )?;
    Ok(())
}

#[test]
fn every_continuation_rechecks_owner_identity_before_any_output() -> Result<(), Box<dyn Error>> {
    use syntaxmesh_api_model::{HistoricalNeighborCursor, encode_neighbor_cursor};
    use syntaxmesh_core::{EdgeDirection, GenerationId};
    let generation = "a".repeat(64);
    let seed = "b".repeat(64);
    let cursor = encode_neighbor_cursor(HistoricalNeighborCursor {
        generation: GenerationId(StableId([0xaa; 32])),
        endpoint: NodeId(StableId([0xbb; 32])),
        direction: EdgeDirection::Outgoing,
        after_edge: EdgeId(StableId([100; 32])),
    })?;
    let first = page(
        &generation,
        &seed,
        "outgoing",
        (1..=100).map(|id| item(id, 0xbb, 0xdd, 0xdd)).collect(),
        Some(&cursor),
    );
    let second = page(
        &generation,
        &seed,
        "outgoing",
        vec![item(101, 0xbb, 0xdd, 0xdd)],
        None,
    );
    for guard in [Guard::Missing, Guard::Wrong, Guard::Duplicate] {
        for (command, arguments) in [
            ("neighbors-turso", vec![seed.as_str()]),
            (
                "neighbors-at-turso",
                vec![generation.as_str(), seed.as_str(), "outgoing", "101"],
            ),
        ] {
            exercise_guarded_pages(
                StatusCode::OK,
                vec![(Guard::Matching, first.clone()), (guard, second.clone())],
                Some("owner instance response mismatch"),
                command,
                &arguments,
            )?;
        }
    }
    Ok(())
}
