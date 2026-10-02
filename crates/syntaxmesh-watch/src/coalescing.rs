use std::time::Duration;

#[cfg(test)]
mod tests;

/// Bounded notification coalescing; the host supplies elapsed monotonic time.
/// Each due batch requests an inventory rescan, not a list of changed paths.
pub struct WatchCoalescer {
    quiet: Duration,
    maximum: Duration,
    pending: Option<(Duration, Duration)>,
}

impl WatchCoalescer {
    /// Rejects zero intervals and a maximum shorter than the quiet interval.
    ///
    /// # Errors
    /// Returns an error for invalid debounce intervals.
    pub fn new(quiet: Duration, maximum: Duration) -> Result<Self, &'static str> {
        if quiet.is_zero() || maximum < quiet {
            return Err("watch intervals require 0 < quiet <= maximum");
        }
        Ok(Self {
            quiet,
            maximum,
            pending: None,
        })
    }

    /// Records a notification or notification-overflow rescan request.
    /// Out-of-order timestamps cannot move the last event backward.
    pub fn mark(&mut self, now: Duration) {
        self.pending = Some(match self.pending {
            Some((first, last)) => (first, last.max(now)),
            None => (now, now),
        });
    }

    /// Whether a rescan is due, without consuming the pending batch.
    #[must_use]
    pub fn is_due(&self, now: Duration) -> bool {
        self.time_until_due(now)
            .is_some_and(|remaining| remaining.is_zero())
    }

    /// Minimum remaining quiet/max interval; None means no pending batch.
    /// Supply time from the same monotonic origin used by `mark`.
    #[must_use]
    pub fn time_until_due(&self, now: Duration) -> Option<Duration> {
        self.pending.map(|(first, last)| {
            self.quiet
                .saturating_sub(now.saturating_sub(last))
                .min(self.maximum.saturating_sub(now.saturating_sub(first)))
        })
    }

    /// Consumes a due batch. The host must re-mark work if indexing fails.
    pub fn take_due(&mut self, now: Duration) -> bool {
        if !self.is_due(now) {
            return false;
        }
        self.pending = None;
        true
    }
}
