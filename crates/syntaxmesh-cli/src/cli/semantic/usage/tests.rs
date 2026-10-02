use serde_json::json;

use super::UsageCounters;

#[test]
fn parallel_reports_aggregate_without_lost_counts() {
    let counters = UsageCounters::default();
    let usage = json!({"prompt_tokens":2,"completion_tokens":3,"total_tokens":5});
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..100 {
                    counters.record(Some(&usage));
                }
            });
        }
    });
    let summary = counters.summary();
    for expected in [
        "reports=400",
        "missing=0",
        "invalid=0",
        "reported_prompt_tokens=800",
        "reported_completion_tokens=1200",
        "reported_total_tokens=2000",
        "overflow=false",
    ] {
        assert!(summary.contains(expected));
    }
}

#[test]
fn absent_null_and_malformed_reports_remain_explicit() {
    let counters = UsageCounters::default();
    counters.record(None);
    counters.record(Some(&json!(null)));
    counters.record(Some(
        &json!({"prompt_tokens":-1,"completion_tokens":1,"total_tokens":0}),
    ));
    counters.record(Some(&json!({"prompt_tokens":1})));
    let summary = counters.summary();
    assert!(summary.contains("reports=0 missing=2 invalid=2"));
}

#[test]
fn token_overflow_marks_totals_unknown_instead_of_wrapping() {
    let counters = UsageCounters::default();
    counters.record(Some(
        &json!({"prompt_tokens":u64::MAX,"completion_tokens":0,"total_tokens":u64::MAX}),
    ));
    counters.record(Some(
        &json!({"prompt_tokens":1,"completion_tokens":0,"total_tokens":1}),
    ));
    let summary = counters.summary();
    assert!(summary.contains("reports=2"));
    assert!(summary.contains("reported_total_tokens=unknown"));
    assert!(summary.contains("overflow=true"));
}
