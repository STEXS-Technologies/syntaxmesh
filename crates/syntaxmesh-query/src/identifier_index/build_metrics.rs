use std::time::{Duration, Instant};

/// Wall-clock work regions for a successfully completed identifier-index build.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IdentifierIndexBuildMetrics {
    pub node_pages: usize,
    /// Logical visitor calls, not physical tree-page reads or fallback pages.
    pub node_scans: usize,
    pub nodes: usize,
    pub postings: usize,
    /// Logical charged payload, not allocator memory or resident-set size.
    pub payload_bytes: usize,
    pub elapsed: Duration,
    /// Store scan time including validation/hydration, excluding callbacks.
    pub page_read_elapsed: Duration,
    /// Includes callback validation and term/posting construction.
    pub processing_elapsed: Duration,
}

pub(super) struct BuildProfiler {
    started: Instant,
    metrics: IdentifierIndexBuildMetrics,
}

impl BuildProfiler {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            metrics: IdentifierIndexBuildMetrics::default(),
        }
    }

    pub(super) fn scan_read(&mut self, started: Instant) {
        self.metrics.node_scans = self.metrics.node_scans.saturating_add(1);
        self.metrics.page_read_elapsed = started
            .elapsed()
            .saturating_sub(self.metrics.processing_elapsed);
    }

    pub(super) fn processed(&mut self, started: Instant) {
        self.metrics.processing_elapsed = self
            .metrics
            .processing_elapsed
            .saturating_add(started.elapsed());
    }

    pub(super) fn finish(
        mut self,
        nodes: usize,
        postings: usize,
        payload_bytes: usize,
    ) -> IdentifierIndexBuildMetrics {
        self.metrics.nodes = nodes;
        self.metrics.postings = postings;
        self.metrics.payload_bytes = payload_bytes;
        self.metrics.elapsed = self.started.elapsed();
        self.metrics
    }
}
