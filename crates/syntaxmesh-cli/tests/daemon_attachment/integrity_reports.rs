use super::transport::{Guard, exercise_command};
use axum::http::StatusCode;
use serde_json::json;
use std::error::Error;

#[test]
fn backend_integrity_rejects_foreign_kind_and_inconsistent_findings_without_output()
-> Result<(), Box<dyn Error>> {
    for (kind, passed, findings) in [
        ("sqlite_database", true, vec!["ok"]),
        ("turso_database", true, vec![]),
        ("turso_database", false, vec!["ok"]),
        ("turso_database", true, vec!["ok", "failed"]),
    ] {
        let body = json!({"schema_version":1,"generation":"a".repeat(64),"data":{"kind":kind,"passed":passed,"findings":findings}}).to_string();
        exercise_command(
            StatusCode::OK,
            Guard::Matching,
            body,
            Some("inconsistent backend integrity report"),
            "integrity-turso",
            &[],
        )?;
    }
    Ok(())
}
