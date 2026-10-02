use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use serde_json::Value;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct UsageCounters {
    reports: AtomicUsize,
    missing: AtomicUsize,
    invalid: AtomicUsize,
    prompt: AtomicU64,
    completion: AtomicU64,
    total: AtomicU64,
    overflow: AtomicBool,
}

impl UsageCounters {
    pub(super) fn record(&self, usage: Option<&Value>) {
        let Some(usage) = usage.filter(|value| !value.is_null()) else {
            self.missing.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let counts = usage
            .get("prompt_tokens")
            .and_then(Value::as_u64)
            .zip(usage.get("completion_tokens").and_then(Value::as_u64))
            .zip(usage.get("total_tokens").and_then(Value::as_u64));
        let Some(((prompt, completion), total)) = counts else {
            self.invalid.fetch_add(1, Ordering::Relaxed);
            return;
        };
        self.reports.fetch_add(1, Ordering::Relaxed);
        self.add(&self.prompt, prompt);
        self.add(&self.completion, completion);
        self.add(&self.total, total);
    }

    fn add(&self, counter: &AtomicU64, value: u64) {
        if counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(value)
            })
            .is_err()
        {
            self.overflow.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn summary(&self) -> String {
        let overflow = self.overflow.load(Ordering::Relaxed);
        let count = |counter: &AtomicU64| {
            if overflow {
                "unknown".to_owned()
            } else {
                counter.load(Ordering::Relaxed).to_string()
            }
        };
        format!(
            "semantic_usage reports={} missing={} invalid={} reported_prompt_tokens={} reported_completion_tokens={} reported_total_tokens={} overflow={overflow}",
            self.reports.load(Ordering::Relaxed),
            self.missing.load(Ordering::Relaxed),
            self.invalid.load(Ordering::Relaxed),
            count(&self.prompt),
            count(&self.completion),
            count(&self.total)
        )
    }
}
