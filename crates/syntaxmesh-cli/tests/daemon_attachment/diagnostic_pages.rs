use super::transport::{Guard, exercise_guarded_pages, exercise_pages};
use axum::http::StatusCode;
use serde_json::json;
use std::error::Error;

fn page(generation: &str, repository: &str, scanned: usize, next: Option<&str>) -> String {
    json!({"schema_version":1,"generation":generation,"data":{
        "repository":repository,"items":[],"scanned_nodes":scanned,
        "has_more":next.is_some(),"next_after":next,
    }})
    .to_string()
}

#[test]
fn diagnostic_pages_reject_invalid_scan_counts_and_continuations() -> Result<(), Box<dyn Error>> {
    let generation = "a".repeat(64);
    let repository = "b".repeat(64);
    let cursor = "c".repeat(64);
    for body in [
        page(&generation, &repository, 101, None),
        page(&generation, &repository, 0, Some(&cursor)),
        page(&generation, &repository, 99, Some(&cursor)),
    ] {
        exercise_pages(
            StatusCode::OK,
            Guard::Matching,
            vec![body],
            Some("inconsistent diagnostic page"),
            "resolution-diagnostics-turso",
            &[],
        )?;
    }
    Ok(())
}

#[test]
fn empty_diagnostic_pages_advance_and_reject_changed_scope_or_nonadvancing_cursor()
-> Result<(), Box<dyn Error>> {
    let generation = "a".repeat(64);
    let repository = "b".repeat(64);
    let cursor = "c".repeat(64);
    let first = page(&generation, &repository, 100, Some(&cursor));
    for second in [
        page(&"d".repeat(64), &repository, 0, None),
        page(&generation, &"d".repeat(64), 0, None),
        page(&generation, &repository, 100, Some(&cursor)),
    ] {
        exercise_pages(
            StatusCode::OK,
            Guard::Matching,
            vec![first.clone(), second],
            Some("inconsistent diagnostic page"),
            "resolution-diagnostics-turso",
            &[],
        )?;
    }
    for guard in [Guard::Missing, Guard::Wrong, Guard::Duplicate] {
        exercise_guarded_pages(
            StatusCode::OK,
            vec![
                (Guard::Matching, first.clone()),
                (guard, page(&generation, &repository, 0, None)),
            ],
            Some("daemon attachment failed"),
            "resolution-diagnostics-turso",
            &[],
        )?;
    }
    Ok(())
}
