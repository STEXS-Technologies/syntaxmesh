use super::{GenerationIdentifierIndex, charged_payload};
use crate::{QueryError, RankedCandidate, identifier_terms};
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{GenerationId, Node, NodeId, NodeKind};
use syntaxmesh_store::{GraphStore, StoreError};

/// Token channel; stemming is identifier morphology, not semantic inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalChannel {
    Exact,
    Stemmed,
}

/// Packing/navigation family; unrelated to evidence confidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalFamily {
    Code,
    Documentation,
    Reference,
    Other,
}

impl LexicalFamily {
    #[must_use]
    pub const fn of(kind: &NodeKind) -> Self {
        match kind {
            NodeKind::File
            | NodeKind::Module
            | NodeKind::Function
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::Test
            | NodeKind::Class
            | NodeKind::Script => Self::Code,
            NodeKind::Document | NodeKind::Section | NodeKind::DocumentChunk => Self::Documentation,
            NodeKind::Reference { .. }
            | NodeKind::UnresolvedReference { .. }
            | NodeKind::AmbiguousReference { .. }
            | NodeKind::Import { .. }
            | NodeKind::Export { .. } => Self::Reference,
            NodeKind::Repository
            | NodeKind::External { .. }
            | NodeKind::RuntimeObservation
            | NodeKind::ModuleResolutionDiagnostic { .. } => Self::Other,
        }
    }
}

/// Explicit query work bounds. Eligible IDs restrict ranking, not global rarity.
pub struct LexicalCandidateRequest<'query> {
    pub query: &'query str,
    pub channel: LexicalChannel,
    pub family: Option<LexicalFamily>,
    pub eligible: Option<&'query BTreeSet<NodeId>>,
    pub posting_budget: usize,
    pub limit: usize,
}

#[derive(Default)]
pub(super) struct PlannerData {
    pub(super) families: BTreeMap<NodeId, LexicalFamily>,
    pub(super) test_intent: BTreeSet<NodeId>,
    stemmed: BTreeMap<String, Vec<NodeId>>,
}

fn stem_terms(terms: &BTreeSet<String>) -> BTreeSet<String> {
    let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
    terms
        .iter()
        .map(|term| stemmer.stem(term).into_owned())
        .collect()
}

impl PlannerData {
    pub(super) fn remove_candidates(&mut self, excluded: &BTreeSet<NodeId>) {
        self.families.retain(|id, _| !excluded.contains(id));
        self.test_intent.retain(|id| !excluded.contains(id));
        for postings in self.stemmed.values_mut() {
            postings.retain(|id| !excluded.contains(id));
        }
        self.stemmed.retain(|_, postings| !postings.is_empty());
    }

    pub(super) fn insert(
        &mut self,
        node: &Node,
        terms: &BTreeSet<String>,
        count: &mut usize,
        bytes: &mut usize,
        posting_limit: usize,
        payload_limit: usize,
    ) -> Result<(), QueryError> {
        *bytes = bytes
            .checked_add(33)
            .filter(|value| *value <= payload_limit)
            .ok_or_else(|| QueryError::Context("planner metadata budget exhausted".to_owned()))?;
        self.families.insert(node.id, LexicalFamily::of(&node.kind));
        let role =
            syntaxmesh_language_sdk::SourceRole::from_payload(node.extension_payload.as_ref())
                .map_err(|error| QueryError::Context(error.to_string()))?;
        if node.kind == NodeKind::Test || role == syntaxmesh_language_sdk::SourceRole::TestIntent {
            *bytes = charged_payload(*bytes, 0, payload_limit)?;
            self.test_intent.insert(node.id);
        }
        for term in stem_terms(terms) {
            if *count >= posting_limit {
                return Err(QueryError::Context(
                    "planner posting budget exhausted".to_owned(),
                ));
            }
            let term_bytes = if self.stemmed.contains_key(&term) {
                0
            } else {
                term.len()
            };
            *bytes = charged_payload(*bytes, term_bytes, payload_limit)?;
            self.stemmed.entry(term).or_default().push(node.id);
            *count = count.saturating_add(1);
        }
        Ok(())
    }
}

impl GenerationIdentifierIndex {
    /// Build both label channels and family metadata in one bounded page walk.
    ///
    /// # Errors
    /// Rejects exhausted budgets and malformed canonical pages.
    pub fn build_for_planner<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
    ) -> Result<Self, QueryError> {
        Self::build_with_inventory(
            store,
            generation,
            node_budget,
            posting_budget,
            payload_budget,
            None,
            true,
        )
    }

    /// Rank a complete channel before family/eligible filtering and truncation.
    /// Only returned IDs are hydrated; zero-score IDs are not lexical matches.
    ///
    /// # Errors
    /// Rejects missing capability, stale manifests, invalid limits/eligible IDs,
    /// exhausted visits, and malformed canonical hydration.
    pub fn lexical_candidates<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalCandidateRequest<'_>,
    ) -> Result<Vec<RankedCandidate>, QueryError> {
        let (ranked, _) = self.lexical_ranked_ids(store, generation, request)?;
        Self::hydrate_ranked(store, generation, ranked)
    }

    pub(super) fn lexical_ranked_ids<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalCandidateRequest<'_>,
    ) -> Result<(Vec<(NodeId, u64)>, usize), QueryError> {
        let (mut ranked, visits) = self.lexical_channel_scores(store, generation, request)?;
        ranked.truncate(request.limit);
        Ok((ranked, visits))
    }

    // Complete ranking for internal family partitioning: truncate only after
    // filtering, otherwise a crowded family can hide another family's matches.
    pub(super) fn lexical_channel_scores<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalCandidateRequest<'_>,
    ) -> Result<(Vec<(NodeId, u64)>, usize), QueryError> {
        let data = self.planner.as_ref().ok_or(QueryError::InvalidLimit)?;
        let exact = identifier_terms(request.query);
        if generation != self.generation
            || request.posting_budget == 0
            || request.limit == 0
            || request.limit > 256
            || exact.len() > 16
            || request.eligible.is_some_and(|ids| {
                ids.len() > 256 || ids.iter().any(|id| !data.families.contains_key(id))
            })
        {
            return Err(QueryError::InvalidLimit);
        }
        let manifest = store.manifest(generation)?;
        if manifest.repository != self.manifest.repository
            || manifest.worktree != self.manifest.worktree
            || manifest.graph_root != self.manifest.graph_root
        {
            return Err(StoreError::Integrity(
                "identifier index snapshot identity mismatch".to_owned(),
            )
            .into());
        }
        let (terms, postings) = match request.channel {
            LexicalChannel::Exact => (exact, &self.postings),
            LexicalChannel::Stemmed => (stem_terms(&exact), &data.stemmed),
        };
        let population = u64::try_from(self.population)
            .map_err(|error| QueryError::Context(error.to_string()))?;
        let mut visits = 0_usize;
        let mut scores = BTreeMap::<NodeId, (u64, u64)>::new();
        for term in terms {
            let Some(ids) = postings.get(&term) else {
                continue;
            };
            let frequency =
                u64::try_from(ids.len()).map_err(|error| QueryError::Context(error.to_string()))?;
            let weight = crate::ranked_candidates::rarity_weight(population, frequency);
            for id in ids {
                if visits >= request.posting_budget {
                    return Err(QueryError::Context(
                        "lexical candidate posting budget exhausted".to_owned(),
                    ));
                }
                visits = visits.saturating_add(1);
                if request
                    .family
                    .is_some_and(|family| data.families.get(id) != Some(&family))
                    || request
                        .eligible
                        .is_some_and(|eligible| !eligible.contains(id))
                {
                    continue;
                }
                let score = scores.entry(*id).or_default();
                score.0 = score.0.saturating_add(weight);
                score.1 = score.1.saturating_add(1);
            }
        }
        let mut ranked = scores
            .into_iter()
            .map(|(id, (weight, coverage))| (id, weight.saturating_mul(coverage)))
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        Ok((ranked, visits))
    }
}

#[cfg(test)]
pub(super) mod tests;
