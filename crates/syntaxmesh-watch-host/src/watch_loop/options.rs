use std::time::Duration;

/// Timing and notification policy for the reusable synchronous watch loop.
#[derive(Debug, Clone)]
pub struct WatchLoopOptions {
    pub quiet: Duration,
    pub maximum: Duration,
    pub reconcile: Duration,
    pub maximum_wait: Duration,
    pub duration: Option<Duration>,
    pub poll_only: bool,
}

impl Default for WatchLoopOptions {
    fn default() -> Self {
        Self {
            quiet: Duration::from_millis(100),
            maximum: Duration::from_secs(1),
            reconcile: Duration::from_secs(30),
            maximum_wait: Duration::from_millis(50),
            duration: None,
            poll_only: false,
        }
    }
}
