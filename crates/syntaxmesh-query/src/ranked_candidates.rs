use crate::{QueryError, generation_term_statistics, identifier_terms};
use syntaxmesh_core::{GenerationId, Node};
use syntaxmesh_store::{GraphStore, StoreError};

/// One reference candidate with deterministic integer lexical relevance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedCandidate {
    pub node: Node,
    pub score: u64,
}

pub(crate) fn rarity_weight(population: u64, frequency: u64) -> u64 {
    let ratio = population
        .checked_div(frequency.saturating_add(1))
        .unwrap_or(0)
        .saturating_add(1);
    u64::from(ratio.ilog2()).saturating_add(1)
}

/// Reference two-pass ranked retrieval for validating derived indexes.
/// Does not modify canonical search or invoke host services. Stores may incur
/// additional internal costs; this is not a constant-time query implementation.
///
/// # Errors
/// Rejects invalid limits, incomplete statistics, unavailable generations and
/// malformed/nonadvancing pages. At most 16 normalized query terms are accepted.
pub fn reference_ranked_candidates<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
    query: &str,
    scan_budget: usize,
    limit: usize,
) -> Result<Vec<RankedCandidate>, QueryError> {
    if limit == 0 || limit > 256 {
        return Err(QueryError::InvalidLimit);
    }
    let stats = generation_term_statistics(store, generation, query, scan_budget)?;
    if !stats.complete {
        return Err(QueryError::Context(
            "ranked candidates require complete generation statistics".to_owned(),
        ));
    }
    let population = u64::try_from(stats.scanned_nodes)
        .map_err(|error| QueryError::Context(error.to_string()))?;
    let weights = stats
        .frequencies
        .iter()
        .map(|(term, frequency)| {
            let frequency = u64::try_from(*frequency)
                .map_err(|error| QueryError::Context(error.to_string()))?;
            Ok((term.clone(), rarity_weight(population, frequency)))
        })
        .collect::<Result<Vec<_>, QueryError>>()?;
    let mut selected = Vec::<RankedCandidate>::new();
    let mut after = None;
    let mut scanned = 0_usize;
    loop {
        let page = store.historical_nodes_page(generation, after, 1000)?;
        if page.items.len() > 1000 || (page.has_more && page.items.is_empty()) {
            return Err(StoreError::Integrity("invalid ranked-candidate page".to_owned()).into());
        }
        for node in page.items {
            if after.is_some_and(|previous| node.id <= previous) || scanned >= scan_budget {
                return Err(StoreError::Integrity(
                    "ranked-candidate scan changed or did not advance".to_owned(),
                )
                .into());
            }
            after = Some(node.id);
            scanned = scanned.saturating_add(1);
            let terms = identifier_terms(&node.name);
            let mut coverage = 0_u64;
            let mut relevance = 0_u64;
            for (term, weight) in &weights {
                if terms.contains(term) {
                    coverage = coverage.saturating_add(1);
                    relevance = relevance.saturating_add(*weight);
                }
            }
            if coverage != 0 {
                selected.push(RankedCandidate {
                    node,
                    score: relevance.saturating_mul(coverage),
                });
                selected.sort_by(|left, right| {
                    right
                        .score
                        .cmp(&left.score)
                        .then_with(|| left.node.id.cmp(&right.node.id))
                });
                selected.truncate(limit);
            }
        }
        if !page.has_more {
            break;
        }
    }
    if scanned != stats.scanned_nodes {
        return Err(StoreError::Integrity("ranked-candidate population changed".to_owned()).into());
    }
    Ok(selected)
}
