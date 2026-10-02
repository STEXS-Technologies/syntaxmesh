//! Reversible omission counts, independent of the accepted text buffer.

use syntaxmesh_api_model::{ContextItemKind, OmittedContextSummary};

use super::Candidate;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Default)]
pub(super) struct Counts {
    source_evidence: usize,
    signatures: usize,
    graph_paths: usize,
    summaries: usize,
}

impl Counts {
    pub(super) fn from_candidates(candidates: &[Candidate]) -> Self {
        let mut counts = Self::default();
        for candidate in candidates {
            let bucket = counts.bucket(candidate.kind);
            *bucket = bucket.saturating_add(1);
        }
        counts
    }

    const fn bucket(&mut self, kind: ContextItemKind) -> &mut usize {
        match kind {
            ContextItemKind::SourceEvidence => &mut self.source_evidence,
            ContextItemKind::Signature => &mut self.signatures,
            ContextItemKind::GraphPath => &mut self.graph_paths,
            ContextItemKind::Summary => &mut self.summaries,
        }
    }

    pub(super) const fn without(mut self, kind: ContextItemKind) -> Self {
        let bucket = self.bucket(kind);
        *bucket = bucket.saturating_sub(1);
        self
    }

    pub(super) fn summary(self) -> OmittedContextSummary {
        OmittedContextSummary {
            source_evidence: u32::try_from(self.source_evidence).unwrap_or(u32::MAX),
            signatures: u32::try_from(self.signatures).unwrap_or(u32::MAX),
            graph_paths: u32::try_from(self.graph_paths).unwrap_or(u32::MAX),
            summaries: u32::try_from(self.summaries).unwrap_or(u32::MAX),
        }
    }
}
