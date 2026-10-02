use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Default)]
pub(super) struct Profile {
    calls: AtomicU64,
    micros: AtomicU64,
}

impl Profile {
    pub(super) fn record(&self, started: Instant) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.micros.fetch_add(
            u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    pub(super) fn snapshot(&self) -> (u64, u64) {
        (
            self.calls.load(Ordering::Relaxed),
            self.micros.load(Ordering::Relaxed),
        )
    }
}
