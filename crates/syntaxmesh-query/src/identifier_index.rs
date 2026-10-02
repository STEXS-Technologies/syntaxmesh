use crate::{QueryError, RankedCandidate, identifier_terms};
use std::collections::BTreeMap;
use syntaxmesh_core::{FileId, GenerationId, GenerationManifest, NodeId};
use syntaxmesh_store::{GraphStore, StoreError};

pub(crate) mod lexical;
mod path_inventory;
mod planner;
mod source_content;
pub use planner::{
    DiscoveryPackingPreference, LexicalPlanRequest, SourceRolePackingRequest, SourceRolePreference,
};

#[cfg(feature = "benchmark-instrumentation")]
mod build_metrics;
#[cfg(feature = "benchmark-instrumentation")]
pub use build_metrics::IdentifierIndexBuildMetrics;

#[cfg(test)]
mod tests;

/// Immutable, explicitly budgeted derived index for one retained generation.
/// Stores IDs only; canonical payloads remain authoritative in the store.
pub struct GenerationIdentifierIndex {
    generation: GenerationId,
    manifest: GenerationManifest,
    population: usize,
    postings: BTreeMap<String, Vec<NodeId>>,
    planner: Option<lexical::PlannerData>,
    #[cfg(feature = "benchmark-instrumentation")]
    build_metrics: IdentifierIndexBuildMetrics,
}

impl GenerationIdentifierIndex {
    /// Build complete postings, rejecting partial or malformed input.
    ///
    /// # Errors
    /// Rejects zero/exhausted budgets and invalid historical scans.
    pub fn build<S: GraphStore + ?Sized>(
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
            false,
        )
    }

    /// Build an opt-in label/path channel from one retained generation.
    /// Inventory bytes charge UTF-8 paths plus one 32-byte file ID per file,
    /// separately from posting payload bytes; neither budget measures RSS.
    ///
    /// # Errors
    /// Rejects exhausted budgets, malformed scans and missing source files.
    pub fn build_with_paths<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
        file_budget: usize,
        inventory_payload_budget: usize,
    ) -> Result<Self, QueryError> {
        if node_budget == 0 || posting_budget == 0 || payload_budget == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let paths = path_inventory::load(store, generation, file_budget, inventory_payload_budget)?;
        Self::build_with_inventory(
            store,
            generation,
            node_budget,
            posting_budget,
            payload_budget,
            Some(&paths),
            false,
        )
    }

    fn build_with_inventory<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
        node_budget: usize,
        posting_budget: usize,
        payload_budget: usize,
        paths: Option<&BTreeMap<FileId, String>>,
        planner: bool,
    ) -> Result<Self, QueryError> {
        if node_budget == 0 || posting_budget == 0 || payload_budget == 0 {
            return Err(QueryError::InvalidLimit);
        }
        #[cfg(feature = "benchmark-instrumentation")]
        let mut profiler = build_metrics::BuildProfiler::new();
        let mut index = Self {
            generation,
            manifest: store.manifest(generation)?,
            population: 0,
            postings: BTreeMap::new(),
            planner: planner.then(lexical::PlannerData::default),
            #[cfg(feature = "benchmark-instrumentation")]
            build_metrics: IdentifierIndexBuildMetrics::default(),
        };
        let mut after = None;
        let mut posting_count = 0_usize;
        let mut payload_bytes = 0_usize;
        let mut callback_error = None;
        #[cfg(feature = "benchmark-instrumentation")]
        let scan_started = std::time::Instant::now();
        let result = store.visit_historical_nodes(generation, node_budget, &mut |node| {
            #[cfg(feature = "benchmark-instrumentation")]
            let processing_started = std::time::Instant::now();
            let insertion = (|| -> Result<(), QueryError> {
                if index.population >= node_budget {
                    return Err(QueryError::Context(
                        "identifier index node budget exhausted".to_owned(),
                    ));
                }
                if after.is_some_and(|previous| node.id <= previous) {
                    return Err(StoreError::Integrity(
                        "nonadvancing identifier index page".to_owned(),
                    )
                    .into());
                }
                after = Some(node.id);
                index.population = index.population.saturating_add(1);
                let mut terms = identifier_terms(&node.name);
                if let Some(data) = &mut index.planner {
                    data.insert(
                        &node,
                        &terms,
                        &mut posting_count,
                        &mut payload_bytes,
                        posting_budget,
                        payload_budget,
                    )?;
                }
                if let (Some(inventory), Some(source)) = (paths, &node.source) {
                    let path = inventory.get(&source.file_id).ok_or_else(|| {
                        StoreError::Integrity("path index source file is missing".to_owned())
                    })?;
                    terms.extend(identifier_terms(path));
                }
                for term in terms {
                    if posting_count >= posting_budget {
                        return Err(QueryError::Context(
                            "identifier index posting budget exhausted".to_owned(),
                        ));
                    }
                    let term_bytes = if index.postings.contains_key(&term) {
                        0
                    } else {
                        term.len()
                    };
                    payload_bytes = charged_payload(payload_bytes, term_bytes, payload_budget)?;
                    index.postings.entry(term).or_default().push(node.id);
                    posting_count = posting_count.saturating_add(1);
                }
                Ok(())
            })();
            #[cfg(feature = "benchmark-instrumentation")]
            profiler.processed(processing_started);
            match insertion {
                Ok(()) => Ok(()),
                Err(error) => {
                    callback_error = Some(error);
                    Err(StoreError::Integrity(
                        "identifier index visitor stopped".to_owned(),
                    ))
                }
            }
        });
        if let Some(error) = callback_error {
            return Err(error);
        }
        let visited = result.map_err(|error| {
            if matches!(error, StoreError::InvalidPageLimit) {
                QueryError::Context("identifier index node budget exhausted".to_owned())
            } else {
                error.into()
            }
        })?;
        if visited != index.population {
            return Err(
                StoreError::Integrity("identifier index scan count differs".to_owned()).into(),
            );
        }
        #[cfg(feature = "benchmark-instrumentation")]
        {
            profiler.scan_read(scan_started);
            index.build_metrics = profiler.finish(index.population, posting_count, payload_bytes);
        }
        Ok(index)
    }

    /// Read completed build work; available only for instrumented builds.
    #[cfg(feature = "benchmark-instrumentation")]
    #[must_use]
    pub const fn build_metrics(&self) -> IdentifierIndexBuildMetrics {
        self.build_metrics
    }

    /// Retrieve exact-token candidates using the reference rarity/coverage rule.
    /// Visits only matching postings and hydrates only the selected nodes.
    ///
    /// # Errors
    /// Rejects generation mismatch, invalid limits, more than 16 terms,
    /// exhausted posting budgets, and missing canonical nodes.
    pub fn candidates<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        query: &str,
        posting_budget: usize,
        limit: usize,
    ) -> Result<Vec<RankedCandidate>, QueryError> {
        let terms = identifier_terms(query);
        if generation != self.generation
            || posting_budget == 0
            || limit == 0
            || limit > 256
            || terms.len() > 16
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
        let population = u64::try_from(self.population)
            .map_err(|error| QueryError::Context(error.to_string()))?;
        let mut visits = 0_usize;
        let mut scores = BTreeMap::<NodeId, (u64, u64)>::new();
        for term in terms {
            let Some(postings) = self.postings.get(&term) else {
                continue;
            };
            let frequency = u64::try_from(postings.len())
                .map_err(|error| QueryError::Context(error.to_string()))?;
            let weight = crate::ranked_candidates::rarity_weight(population, frequency);
            for id in postings {
                if visits >= posting_budget {
                    return Err(QueryError::Context(
                        "identifier candidate posting budget exhausted".to_owned(),
                    ));
                }
                visits = visits.saturating_add(1);
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
        ranked.truncate(limit);
        Self::hydrate_ranked(store, generation, ranked)
    }

    fn hydrate_ranked<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
        ranked: Vec<(NodeId, u64)>,
    ) -> Result<Vec<RankedCandidate>, QueryError> {
        let ids = ranked.iter().map(|entry| entry.0).collect::<Vec<_>>();
        let mut nodes = BTreeMap::new();
        for node in store.historical_nodes_by_ids(generation, &ids)? {
            if !ids.contains(&node.id) || nodes.insert(node.id, node).is_some() {
                return Err(StoreError::Integrity(
                    "identifier batch has foreign or duplicate nodes".to_owned(),
                )
                .into());
            }
        }
        ranked
            .into_iter()
            .map(|(id, score)| {
                let node = nodes.remove(&id).ok_or_else(|| {
                    StoreError::Integrity(
                        "identifier candidate missing from pinned generation".to_owned(),
                    )
                })?;
                Ok(RankedCandidate { node, score })
            })
            .collect()
    }
}

fn charged_payload(current: usize, term_bytes: usize, limit: usize) -> Result<usize, QueryError> {
    current
        .checked_add(term_bytes)
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<NodeId>()))
        .filter(|bytes| *bytes <= limit)
        .ok_or_else(|| QueryError::Context("identifier index payload budget exhausted".to_owned()))
}
