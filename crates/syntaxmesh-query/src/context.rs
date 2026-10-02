//! Deterministic evidence selection and exact tokenizer-budget packing.

use std::collections::{BTreeMap, BTreeSet};

use syntaxmesh_api_model::{
    CONTEXT_PACK_SCHEMA_VERSION, ContextItem, ContextItemKind, ContextPack, ContextRequest,
    ContextWarning,
};
use syntaxmesh_core::{Edge, EdgeId, FileVersion, Node, NodeId, NodeKind, SourceLocation};

use crate::{Query, QueryError, identifier_terms};

mod excerpts;
mod graph;
mod omissions;
mod packing;
mod ranking;
mod source_cache;

use graph::{ContextGraph, ReadMode};
use ranking::{SourceGranularity, candidate_order};

#[derive(Clone, Copy)]
pub(crate) enum SeedMode {
    LexicalAndExplicit,
    ExplicitOnly,
    RankedExplicit,
    RankedPlan,
}

/// Source-free, bounded selection state before token packing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSelectionReport {
    pub generation: syntaxmesh_core::GenerationId,
    /// Stable-ID ordered IDs, graph distance and relevance/caller priority.
    pub nodes: Vec<(NodeId, u8, u32)>,
    pub edges: usize,
    pub warnings: Vec<ContextWarning>,
}

pub(crate) fn selection_report<S: syntaxmesh_store::GraphStore + ?Sized>(
    query: &Query<'_, S>,
    request: &ContextRequest,
    historical: bool,
    seed_mode: SeedMode,
) -> Result<ContextSelectionReport, QueryError> {
    validate_request(request, seed_mode)?;
    query.manifest()?;
    let graph = ContextGraph::new(
        query,
        if historical {
            ReadMode::Historical
        } else {
            ReadMode::Current
        },
    );
    let (mut nodes, warnings) = select_nodes(&graph, request, seed_mode)?;
    let mut edges = BTreeMap::new();
    expand_neighborhood(
        &graph,
        &mut nodes,
        &mut edges,
        request.max_hops,
        usize::from(request.max_candidates),
        seed_mode,
    )?;
    Ok(ContextSelectionReport {
        generation: query.generation(),
        nodes: nodes
            .into_iter()
            .map(|(id, (_, depth, relevance))| (id, depth, relevance))
            .collect(),
        edges: edges.len(),
        warnings,
    })
}

/// Host-supplied source access. Returned bytes are checked against the accepted file hash before
/// any text is included. Implementations must scope paths to the intended repository/worktree.
pub trait ContextSourceProvider {
    /// # Errors
    /// Return an error when source access fails for a reason the host wants surfaced.
    fn read_source(&self, file: &FileVersion) -> Result<Option<Vec<u8>>, String>;
}

/// Host/model-specific exact token counting for the serialized context pack.
pub trait ContextTokenCounter {
    /// Stable tokenizer and configuration identity included in the pack.
    fn tokenizer_id(&self) -> &str;

    /// Count the exact tokenization of the supplied serialized payload.
    ///
    /// # Errors
    /// Return an error when the configured tokenizer cannot count this payload.
    fn count_tokens(&self, serialized_pack: &str) -> Result<u64, String>;
}

#[derive(Debug, Clone)]
struct Candidate {
    kind: ContextItemKind,
    granularity: SourceGranularity,
    relevance: u32,
    distance: u8,
    key: String,
    item: ContextItem,
}

/// Compile a generation-pinned evidence pack. All source and tokenizer-specific behavior is
/// injected; graph reads stay on the ordinary Query path.
pub fn compile_context<S>(
    query: &Query<'_, S>,
    request: &ContextRequest,
    source_provider: &impl ContextSourceProvider,
    token_counter: &impl ContextTokenCounter,
) -> Result<ContextPack, QueryError>
where
    S: syntaxmesh_store::GraphStore + ?Sized,
{
    compile(
        query,
        request,
        source_provider,
        token_counter,
        ReadMode::Current,
        SeedMode::LexicalAndExplicit,
    )
}

pub(crate) fn compile_historical_context<S: syntaxmesh_store::GraphStore + ?Sized>(
    query: &Query<'_, S>,
    request: &ContextRequest,
    source_provider: &impl ContextSourceProvider,
    token_counter: &impl ContextTokenCounter,
) -> Result<ContextPack, QueryError> {
    compile(
        query,
        request,
        source_provider,
        token_counter,
        ReadMode::Historical,
        SeedMode::LexicalAndExplicit,
    )
}

pub(crate) fn compile_seeded_context<S: syntaxmesh_store::GraphStore + ?Sized>(
    query: &Query<'_, S>,
    request: &ContextRequest,
    source_provider: &impl ContextSourceProvider,
    token_counter: &impl ContextTokenCounter,
    historical: bool,
    seed_mode: SeedMode,
) -> Result<ContextPack, QueryError> {
    compile(
        query,
        request,
        source_provider,
        token_counter,
        if historical {
            ReadMode::Historical
        } else {
            ReadMode::Current
        },
        seed_mode,
    )
}

fn compile<S: syntaxmesh_store::GraphStore + ?Sized>(
    query: &Query<'_, S>,
    request: &ContextRequest,
    source_provider: &impl ContextSourceProvider,
    token_counter: &impl ContextTokenCounter,
    mode: ReadMode,
    seed_mode: SeedMode,
) -> Result<ContextPack, QueryError> {
    validate_request(request, seed_mode)?;
    if token_counter.tokenizer_id().trim().is_empty() {
        return Err(QueryError::Context(
            "tokenizer identity is empty".to_owned(),
        ));
    }

    let generation = query.generation();
    let manifest = query.manifest()?;
    let mut file_versions = if matches!(mode, ReadMode::Current) {
        query.files()?
    } else {
        Vec::new()
    }
    .into_iter()
    .map(|file| (file.file_id, file))
    .collect::<BTreeMap<_, _>>();
    let graph = ContextGraph::new(query, mode);
    let (mut selected_nodes, mut warnings) = select_nodes(&graph, request, seed_mode)?;
    let mut selected_edges = BTreeMap::<EdgeId, Edge>::new();
    expand_neighborhood(
        &graph,
        &mut selected_nodes,
        &mut selected_edges,
        request.max_hops,
        usize::from(request.max_candidates),
        seed_mode,
    )?;

    if matches!(mode, ReadMode::Historical) {
        let ids = selected_nodes
            .values()
            .flat_map(|(node, _, _)| {
                [
                    node.source.as_ref().map(|source| source.file_id),
                    node.owner_file,
                ]
            })
            .flatten()
            .collect::<BTreeSet<_>>();
        for id in ids {
            if let Some(file) = query.historical_file(id)? {
                file_versions.insert(id, file);
            }
        }
    }

    let mut candidates = Vec::new();
    let mut source_cache = source_cache::SourceCache::new(source_provider);
    for (node, distance, relevance) in selected_nodes.values() {
        if let Some(location) = &node.source
            && let Some(candidate) = source_candidate(
                node,
                location,
                *distance,
                *relevance,
                &file_versions,
                &mut source_cache,
                &mut warnings,
            )
        {
            candidates.push(candidate);
        }
        let source_file_id = node
            .source
            .as_ref()
            .map(|source| source.file_id)
            .or(node.owner_file);
        candidates.push(signature_candidate(
            node,
            *distance,
            *relevance,
            source_file_id.and_then(|file_id| file_versions.get(&file_id)),
        ));
    }

    for edge in selected_edges.values() {
        if let (Some(source), Some(target)) = (
            selected_nodes.get(&edge.source),
            selected_nodes.get(&edge.target),
        ) {
            let text = format!(
                "{} --{:?}--> {}",
                source.0.name, edge.relation, target.0.name
            );
            let distance = source.1.max(target.1);
            candidates.push(Candidate {
                kind: ContextItemKind::GraphPath,
                granularity: SourceGranularity::Local,
                relevance: source.2.saturating_add(target.2),
                distance,
                key: format!("edge:{}", edge.id.0.to_hex()),
                item: ContextItem {
                    rank: 0,
                    evidence_class: None,
                    kind: ContextItemKind::GraphPath,
                    text,
                    node_ids: vec![edge.source, edge.target],
                    edge_ids: vec![edge.id],
                    source_path: None,
                    line_start: None,
                    line_end: None,
                },
            });
        }
    }

    for (node, distance, relevance) in selected_nodes.values() {
        candidates.push(Candidate {
            kind: ContextItemKind::Summary,
            granularity: SourceGranularity::Local,
            relevance: *relevance,
            distance: *distance,
            key: format!("summary:{}", node.id.0.to_hex()),
            item: ContextItem {
                rank: 0,
                evidence_class: None,
                kind: ContextItemKind::Summary,
                text: format!(
                    "{} ({:?}) is {} graph hop(s) from the selected query seeds.",
                    node.name, node.kind, distance
                ),
                node_ids: vec![node.id],
                edge_ids: Vec::new(),
                source_path: None,
                line_start: None,
                line_end: None,
            },
        });
    }

    let origins = candidates
        .iter()
        .map(|candidate| {
            candidate
                .item
                .edge_ids
                .first()
                .and_then(|id| selected_edges.get(id))
                .map(|edge| edge.provenance)
                .or_else(|| {
                    candidate
                        .item
                        .node_ids
                        .first()
                        .and_then(|id| selected_nodes.get(id))
                        .map(|entry| entry.0.provenance)
                })
                .ok_or_else(|| {
                    QueryError::Context("context candidate lacks originating provenance".to_owned())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ids = origins
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let classes = graph
        .context_provenance(&ids)?
        .into_iter()
        .map(|record| (record.id, record.evidence_class))
        .collect::<BTreeMap<_, _>>();
    for (candidate, origin) in candidates.iter_mut().zip(origins) {
        candidate.item.evidence_class = Some(*classes.get(&origin).ok_or_else(|| {
            QueryError::Context("context fact references missing provenance".to_owned())
        })?);
    }

    candidates.sort_by(candidate_order);
    deduplicate_candidates(&mut candidates);
    excerpts::coalesce(&mut candidates);
    let mut omitted = omissions::Counts::from_candidates(&candidates);

    let mut pack = ContextPack {
        schema_version: CONTEXT_PACK_SCHEMA_VERSION,
        query: request.query.clone(),
        repository: manifest.repository,
        worktree: manifest.worktree,
        generation,
        tokenizer: token_counter.tokenizer_id().to_owned(),
        token_budget: request.token_budget,
        token_count: 0,
        items: Vec::new(),
        warnings: warnings.drain(..).take(8).collect(),
        omitted: omitted.summary(),
    };
    finalize_count(&mut pack, token_counter)?;
    if pack.token_count > request.token_budget {
        return Err(QueryError::ContextBudgetTooSmall {
            minimum_tokens: pack.token_count,
            requested_tokens: request.token_budget,
        });
    }

    for candidate in &candidates {
        let trial = omitted.without(candidate.kind);
        if packing::try_item(
            &mut pack,
            candidate.item.clone(),
            trial.summary(),
            token_counter,
        )? {
            omitted = trial;
        }
    }
    pack.omitted = omitted.summary();
    finalize_count(&mut pack, token_counter)?;
    if pack.token_count > request.token_budget {
        return Err(QueryError::ContextBudgetTooSmall {
            minimum_tokens: pack.token_count,
            requested_tokens: request.token_budget,
        });
    }
    Ok(pack)
}

const fn validate_request(request: &ContextRequest, seed_mode: SeedMode) -> Result<(), QueryError> {
    let plan = matches!(seed_mode, SeedMode::RankedPlan);
    if request.query.len() > 4096
        || request.seed_nodes.len()
            > if plan {
                request.max_candidates as usize
            } else {
                32
            }
        || (plan && request.max_hops != 0)
        || request.token_budget == 0
        || request.max_hops > 8
        || request.max_candidates == 0
        || request.max_candidates > 256
    {
        return Err(QueryError::InvalidContextRequest);
    }
    Ok(())
}

type SelectedNodes = BTreeMap<NodeId, (Node, u8, u32)>;

fn select_nodes<S: syntaxmesh_store::GraphStore + ?Sized>(
    query: &ContextGraph<'_, '_, S>,
    request: &ContextRequest,
    seed_mode: SeedMode,
) -> Result<(SelectedNodes, Vec<ContextWarning>), QueryError> {
    if matches!(
        seed_mode,
        SeedMode::ExplicitOnly | SeedMode::RankedExplicit | SeedMode::RankedPlan
    ) && ((request.seed_nodes.is_empty() && !matches!(seed_mode, SeedMode::RankedPlan))
        || request.seed_nodes.iter().collect::<BTreeSet<_>>().len()
            > usize::from(request.max_candidates))
    {
        return Err(QueryError::InvalidContextRequest);
    }
    let mut selected = BTreeMap::new();
    for id in &request.seed_nodes {
        let node = query.node(*id)?.ok_or(QueryError::UnknownSeed(*id))?;
        let priority = if matches!(seed_mode, SeedMode::RankedExplicit | SeedMode::RankedPlan) {
            u32::MAX.saturating_sub(u32::try_from(selected.len()).unwrap_or(u32::MAX))
        } else {
            u32::MAX
        };
        selected.entry(*id).or_insert((node, 0, priority));
    }

    if matches!(
        seed_mode,
        SeedMode::ExplicitOnly | SeedMode::RankedExplicit | SeedMode::RankedPlan
    ) {
        return Ok((selected, Vec::new()));
    }

    let terms = request
        .query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect::<BTreeSet<_>>();
    let max_candidates = usize::from(request.max_candidates);
    let search_limit = candidate_search_limit(max_candidates);
    let mut lookup_limit_reached = false;
    for term in terms.iter().take(16) {
        let matches = query.search(term, search_limit)?;
        lookup_limit_reached |= matches.len() >= search_limit;
        for node in matches {
            let score = lexical_relevance(&node, &terms);
            selected
                .entry(node.id)
                .and_modify(|entry: &mut (Node, u8, u32)| entry.2 = entry.2.max(score))
                .or_insert((node, 0, score));
        }
    }

    let mut warnings = Vec::new();
    if terms.len() > 16 {
        warnings.push(ContextWarning {
            code: "lexical_query_terms_truncated".to_owned(),
            message: format!(
                "{} distinct query terms were reduced to the first 16 in lexical order for candidate lookup",
                terms.len()
            ),
            node_ids: Vec::new(),
        });
    }
    if lookup_limit_reached {
        warnings.push(ContextWarning {
            code: "lexical_lookup_limit_reached".to_owned(),
            message: format!("At least one term lookup reached {search_limit} results; lexical candidate completeness is unknown"),
            node_ids: Vec::new(),
        });
    }
    if request.seed_nodes.is_empty() && !selected.is_empty() {
        let max_score = selected.values().map(|entry| entry.2).max().unwrap_or(0);
        let tied = selected
            .values()
            .filter(|entry| entry.2 == max_score)
            .map(|entry| entry.0.id)
            .collect::<Vec<_>>();
        if tied.len() > 1 {
            warnings.push(ContextWarning {
                code: "ambiguous_lexical_seeds".to_owned(),
                message: format!(
                    "{} equally ranked lexical seed candidates were found",
                    tied.len()
                ),
                node_ids: tied.into_iter().take(8).collect(),
            });
        }
    } else if selected.is_empty() {
        warnings.push(ContextWarning {
            code: "no_lexical_matches".to_owned(),
            message: "No node names matched the query terms or explicit seeds".to_owned(),
            node_ids: Vec::new(),
        });
    }

    if selected.len() > max_candidates {
        warnings.push(ContextWarning {
            code: "lexical_candidates_truncated".to_owned(),
            message: format!(
                "{} merged seed candidates were reduced to the requested {max_candidates} limit",
                selected.len()
            ),
            node_ids: Vec::new(),
        });
        let mut ordered = selected.into_values().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            right
                .2
                .cmp(&left.2)
                .then_with(|| left.0.id.cmp(&right.0.id))
        });
        selected = ordered
            .into_iter()
            .take(max_candidates)
            .map(|entry| (entry.0.id, entry))
            .collect();
    }
    Ok((selected, warnings))
}

fn candidate_search_limit(max_candidates: usize) -> usize {
    max_candidates.saturating_mul(4).min(1024)
}

fn lexical_relevance(node: &Node, terms: &BTreeSet<String>) -> u32 {
    let name_terms = identifier_terms(&node.name);
    let matched_terms = name_terms
        .intersection(terms)
        .cloned()
        .collect::<BTreeSet<_>>();
    let complete_match = !matched_terms.is_empty() && matched_terms.len() == terms.len();
    let score = u32::try_from(matched_terms.len()).unwrap_or(u32::MAX - 1);
    let exact_symbol_term = name_terms.len() == 1 && name_terms.is_subset(terms);
    if exact_symbol_term {
        return score
            .saturating_add(u32::try_from(terms.len()).unwrap_or(u32::MAX - 1))
            .saturating_add(1);
    }
    if complete_match && node.kind == NodeKind::Function {
        score.saturating_add(1)
    } else {
        score
    }
}

fn expand_neighborhood<S: syntaxmesh_store::GraphStore + ?Sized>(
    query: &ContextGraph<'_, '_, S>,
    nodes: &mut BTreeMap<NodeId, (Node, u8, u32)>,
    edges: &mut BTreeMap<EdgeId, Edge>,
    max_hops: u8,
    max_nodes: usize,
    seed_mode: SeedMode,
) -> Result<(), QueryError> {
    let mut frontier = nodes.keys().copied().collect::<Vec<_>>();
    for depth in 1..=max_hops {
        if matches!(seed_mode, SeedMode::RankedExplicit) {
            frontier.sort_by(|left, right| {
                nodes
                    .get(right)
                    .map(|entry| entry.2)
                    .cmp(&nodes.get(left).map(|entry| entry.2))
                    .then_with(|| left.cmp(right))
            });
        }
        let mut next = BTreeSet::new();
        for id in frontier {
            query.visit_adjacent(id, |edge| {
                let other = if edge.source == id {
                    edge.target
                } else {
                    edge.source
                };
                if !nodes.contains_key(&other)
                    && nodes.len() < max_nodes
                    && let Some(node) = query.node(other)?
                {
                    nodes.insert(other, (node, depth, 0));
                    next.insert(other);
                }
                // Admission is monotonic: a refused endpoint cannot become
                // selected later after capacity is exhausted in this query.
                if nodes.contains_key(&edge.source) && nodes.contains_key(&edge.target) {
                    edges.insert(edge.id, edge);
                }
                Ok(())
            })?;
        }
        frontier = next.into_iter().collect();
        if frontier.is_empty() {
            break;
        }
    }
    Ok(())
}

fn source_candidate(
    node: &Node,
    location: &SourceLocation,
    distance: u8,
    relevance: u32,
    files: &BTreeMap<syntaxmesh_core::FileId, FileVersion>,
    cache: &mut source_cache::SourceCache<'_, impl ContextSourceProvider>,
    warnings: &mut Vec<ContextWarning>,
) -> Option<Candidate> {
    let Some(file) = files.get(&location.file_id) else {
        warnings.push(source_warning(
            "missing_file_fact",
            node,
            "source file is absent from the pinned generation",
        ));
        return None;
    };
    if location.content_hash != file.content_hash {
        warnings.push(source_warning(
            "stale_source_span",
            node,
            "source location hash differs from the pinned file version",
        ));
        return None;
    }
    let text = match cache.read(file) {
        Ok(text) => text,
        Err(failure) => {
            warnings.push(source_warning(failure.code, node, &failure.message));
            return None;
        }
    };
    let Some((snippet, line_start, line_end)) =
        line_span(&text, location.span.start_byte, location.span.end_byte)
    else {
        warnings.push(source_warning(
            "invalid_source_span",
            node,
            "source span is outside the file or not on UTF-8 boundaries",
        ));
        return None;
    };
    Some(Candidate {
        kind: ContextItemKind::SourceEvidence,
        granularity: SourceGranularity::for_node(&node.kind),
        relevance,
        distance,
        key: format!("source:{}:{}", node.id.0.to_hex(), location.span.start_byte),
        item: ContextItem {
            rank: 0,
            evidence_class: None,
            kind: ContextItemKind::SourceEvidence,
            text: format!(
                "{}:{}-{}\n{}",
                file.normalized_path, line_start, line_end, snippet
            ),
            node_ids: vec![node.id],
            edge_ids: Vec::new(),
            source_path: Some(file.normalized_path.clone()),
            line_start: Some(line_start),
            line_end: Some(line_end),
        },
    })
}

fn signature_candidate(
    node: &Node,
    distance: u8,
    relevance: u32,
    file: Option<&FileVersion>,
) -> Candidate {
    Candidate {
        kind: ContextItemKind::Signature,
        granularity: SourceGranularity::Local,
        relevance,
        distance,
        key: format!("signature:{}", node.id.0.to_hex()),
        item: ContextItem {
            rank: 0,
            evidence_class: None,
            kind: ContextItemKind::Signature,
            text: format!("{:?} {}", node.kind, node.name),
            node_ids: vec![node.id],
            edge_ids: Vec::new(),
            source_path: file.map(|file| file.normalized_path.clone()),
            line_start: None,
            line_end: None,
        },
    }
}

fn source_warning(code: &str, node: &Node, message: &str) -> ContextWarning {
    ContextWarning {
        code: code.to_owned(),
        message: message.chars().take(240).collect(),
        node_ids: vec![node.id],
    }
}

fn line_span(text: &str, start: u64, end: u64) -> Option<(String, u32, u32)> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    if start > end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return None;
    }
    let line_start = text[..start]
        .rfind('\n')
        .map_or(0, |index| index.saturating_add(1));
    let line_end = text[end..]
        .find('\n')
        .map_or(text.len(), |offset| end.saturating_add(offset));
    let first_line = u32::try_from(
        text[..line_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            .saturating_add(1),
    )
    .ok()?;
    let line_count = u32::try_from(
        text[line_start..line_end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count(),
    )
    .ok()?;
    let last_line = first_line.saturating_add(line_count);
    Some((text[line_start..line_end].to_owned(), first_line, last_line))
}

fn deduplicate_candidates(candidates: &mut Vec<Candidate>) {
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| seen.insert(candidate.key.clone()));
}

fn finalize_count(
    pack: &mut ContextPack,
    counter: &impl ContextTokenCounter,
) -> Result<(), QueryError> {
    for _ in 0..8 {
        let serialized = serde_json::to_string(pack)
            .map_err(|error| QueryError::Context(format!("serialize context pack: {error}")))?;
        let counted = counter
            .count_tokens(&serialized)
            .map_err(|error| QueryError::Context(format!("count context tokens: {error}")))?;
        if counted == pack.token_count {
            return Ok(());
        }
        pack.token_count = counted;
    }
    Err(QueryError::Context(
        "token count did not reach a stable serialized value".to_owned(),
    ))
}

#[cfg(test)]
mod tests;
