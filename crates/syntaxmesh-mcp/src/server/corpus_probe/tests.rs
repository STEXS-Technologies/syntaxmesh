use std::collections::BTreeSet;
use syntaxmesh_core::{Node, NodeId};

#[path = "families.rs"]
mod families;

/// Complete test-only oracle: canonical payloads with source-location path terms.
pub(super) fn path_candidates(
    nodes: &[Node],
    files: &[syntaxmesh_core::FileVersion],
    query: &str,
) -> Result<Vec<syntaxmesh_query::RankedCandidate>, String> {
    complete_candidates(nodes, Some(files), query)
}

pub(super) fn label_candidates(
    nodes: &[Node],
    query: &str,
) -> Result<Vec<syntaxmesh_query::RankedCandidate>, String> {
    complete_candidates(nodes, None, query)
}

fn complete_candidates(
    nodes: &[Node],
    files: Option<&[syntaxmesh_core::FileVersion]>,
    query: &str,
) -> Result<Vec<syntaxmesh_query::RankedCandidate>, String> {
    let paths = files
        .unwrap_or_default()
        .iter()
        .map(|file| (file.file_id, &file.normalized_path))
        .collect::<std::collections::BTreeMap<_, _>>();
    let documents = nodes
        .iter()
        .map(|node| {
            let mut terms = syntaxmesh_query::identifier_terms(&node.name);
            if let Some(source) = &node.source
                && files.is_some()
            {
                let path = paths
                    .get(&source.file_id)
                    .ok_or("missing oracle source file")?;
                terms.extend(syntaxmesh_query::identifier_terms(path));
            }
            Ok((node, terms))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let population = u64::try_from(nodes.len()).map_err(|error| error.to_string())?;
    let weights = syntaxmesh_query::identifier_terms(query)
        .into_iter()
        .map(|term| {
            let frequency = u64::try_from(
                documents
                    .iter()
                    .filter(|(_, terms)| terms.contains(&term))
                    .count(),
            )
            .map_err(|error| error.to_string())?;
            Ok((
                term,
                u64::from(
                    population
                        .checked_div(frequency.saturating_add(1))
                        .unwrap_or(0)
                        .saturating_add(1)
                        .ilog2(),
                )
                .saturating_add(1),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut ranked = Vec::new();
    for (node, terms) in documents {
        let mut sum = 0_u64;
        let mut coverage = 0_u64;
        for (term, weight) in &weights {
            if terms.contains(term) {
                sum = sum.saturating_add(*weight);
                coverage = coverage.saturating_add(1);
            }
        }
        if coverage != 0 {
            ranked.push(syntaxmesh_query::RankedCandidate {
                node: node.clone(),
                score: sum.saturating_mul(coverage),
            });
        }
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.node.id.cmp(&right.node.id))
    });
    ranked.truncate(256);
    Ok(ranked)
}

/// Test-only ownership field comparison; IDs and canonical nodes stay untouched.
pub(super) fn with_file_paths(nodes: &[Node], files: &[syntaxmesh_core::FileVersion]) -> Vec<Node> {
    let paths = files
        .iter()
        .map(|file| (file.file_id, file.normalized_path.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    nodes
        .iter()
        .map(|node| {
            let mut diagnostic = node.clone();
            if let Some(path) = node
                .owner_file
                .or_else(|| node.source.as_ref().map(|location| location.file_id))
                .and_then(|file| paths.get(&file))
            {
                diagnostic.name.push(' ');
                diagnostic.name.push_str(path);
            }
            diagnostic
        })
        .collect()
}

/// Exact-token Snowball comparison using the already loaded diagnostic corpus.
pub(super) fn stemmed_rank(nodes: &[Node], query: &str, target: NodeId) -> serde_json::Value {
    let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
    let normalize = |text: &str| {
        syntaxmesh_query::identifier_terms(text)
            .iter()
            .map(|term| stemmer.stem(term).into_owned())
            .collect::<BTreeSet<_>>()
    };
    let scores = token_scores(nodes, query, normalize);
    let exact = token_scores(nodes, query, syntaxmesh_query::identifier_terms);
    let definitions = scores
        .iter()
        .copied()
        .filter(|(_, node)| definition_candidate(node))
        .collect::<Vec<_>>();
    let exact_definitions = exact
        .iter()
        .copied()
        .filter(|(_, node)| definition_candidate(node))
        .collect::<Vec<_>>();
    let fused = reciprocal_fusion([&exact, &scores], 60);
    let harmonic = reciprocal_fusion([&exact, &scores], 0);
    let mut family_reports = serde_json::Map::new();
    let mut family_channels = Vec::new();
    for family in ["code", "documentation", "reference", "other"] {
        let population = nodes
            .iter()
            .filter(|node| families::classify(&node.kind) == family)
            .collect::<Vec<_>>();
        let local_stemmed = token_scores_refs(&population, query, normalize);
        let local_exact = token_scores_refs(&population, query, syntaxmesh_query::identifier_terms);
        let exact_members = exact
            .iter()
            .copied()
            .filter(|(_, node)| families::classify(&node.kind) == family)
            .collect::<Vec<_>>();
        let members = scores
            .iter()
            .copied()
            .filter(|(_, node)| families::classify(&node.kind) == family)
            .collect::<Vec<_>>();
        family_channels.push(members.iter().map(|(_, node)| node.id).collect::<Vec<_>>());
        family_reports.insert(
            family.to_owned(),
            serde_json::json!({
                "population": population.len(),
                "statistics_scope": "complete_generation",
                "target_rank":target_rank(&members, target),
                "top":diagnostic_top(&members),
                "exact_target_rank":target_rank(&exact_members, target),
                "exact_top":diagnostic_top(&exact_members),
                "family_local_statistics": {
                    "statistics_scope": "complete_family",
                    "target_rank":target_rank(&local_stemmed, target),
                    "top":diagnostic_top(&local_stemmed),
                    "exact_target_rank":target_rank(&local_exact, target),
                    "exact_top":diagnostic_top(&local_exact)
                }
            }),
        );
    }
    serde_json::json!({"target_rank": target_rank(&scores, target),
        "families":family_reports,
        "family_round_robin_preview":families::refill(&family_channels, 8).iter().map(|id| id.0.to_hex()).collect::<Vec<_>>(),
        "definition_target_rank": target_rank(&definitions, target),
        "definition_top": diagnostic_top(&definitions),
        "exact_definition_target_rank": target_rank(&exact_definitions, target),
        "top": diagnostic_top(&scores),
        "exact_rank": target_rank(&exact, target),
        "reciprocal_fusion_rank": target_rank(&fused, target),
        "reciprocal_fusion_top": diagnostic_top(&fused),
        "reciprocal_fusion_offset": 60,
        "zero_offset_fusion_rank": target_rank(&harmonic, target),
        "zero_offset_fusion_top": diagnostic_top(&harmonic)})
}

fn reciprocal_fusion<'nodes>(
    channels: [&Vec<(u64, &'nodes Node)>; 2],
    offset: u64,
) -> Vec<(u64, &'nodes Node)> {
    let mut fused = std::collections::BTreeMap::new();
    // Diagnostic only: equal channel weights and integer reciprocal ranks.
    // Both complete channels contribute; this is not a bounded production search.
    for channel in channels {
        for (position, (_, node)) in channel.iter().enumerate() {
            let denominator = u64::try_from(position)
                .unwrap_or(u64::MAX)
                .saturating_add(1)
                .saturating_add(offset);
            let contribution = 1_000_000_000_u64.checked_div(denominator).unwrap_or(0);
            let entry = fused.entry(node.id).or_insert((0_u64, *node));
            entry.0 = entry.0.saturating_add(contribution);
        }
    }
    let mut fused = fused.into_values().collect::<Vec<_>>();
    sort_scores(&mut fused);
    fused
}

fn target_rank(scores: &[(u64, &Node)], target: NodeId) -> Option<usize> {
    scores
        .iter()
        .position(|(_, node)| node.id == target)
        .map(|position| position.saturating_add(1))
}

fn diagnostic_top(scores: &[(u64, &Node)]) -> Vec<serde_json::Value> {
    scores.iter().take(5).map(|(score, node)| serde_json::json!({"score":score,"name":node.name.chars().take(240).collect::<String>(), "node_id":node.id.0.to_hex(), "kind":node.kind, "owner_file":node.owner_file, "source":node.source})).collect()
}

fn definition_candidate(node: &Node) -> bool {
    matches!(
        node.kind,
        syntaxmesh_core::NodeKind::Function
            | syntaxmesh_core::NodeKind::Class
            | syntaxmesh_core::NodeKind::Struct
            | syntaxmesh_core::NodeKind::Enum
            | syntaxmesh_core::NodeKind::Trait
            | syntaxmesh_core::NodeKind::Test
            | syntaxmesh_core::NodeKind::Module
            | syntaxmesh_core::NodeKind::File
            | syntaxmesh_core::NodeKind::Script
            | syntaxmesh_core::NodeKind::Document
            | syntaxmesh_core::NodeKind::Section
            | syntaxmesh_core::NodeKind::DocumentChunk
    )
}

fn sort_scores(scores: &mut [(u64, &Node)]) {
    scores.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.id.cmp(&right.1.id))
    });
}

fn token_scores<'nodes>(
    nodes: &'nodes [Node],
    query: &str,
    normalize: impl Fn(&str) -> BTreeSet<String>,
) -> Vec<(u64, &'nodes Node)> {
    token_scores_refs(&nodes.iter().collect::<Vec<_>>(), query, normalize)
}

fn token_scores_refs<'nodes>(
    nodes: &[&'nodes Node],
    query: &str,
    normalize: impl Fn(&str) -> BTreeSet<String>,
) -> Vec<(u64, &'nodes Node)> {
    let terms = normalize(query);
    let labels = nodes
        .iter()
        .map(|node| (*node, normalize(&node.name)))
        .collect::<Vec<_>>();
    let population = u64::try_from(nodes.len()).unwrap_or(u64::MAX);
    let weights = terms
        .iter()
        .map(|term| {
            let frequency = u64::try_from(
                labels
                    .iter()
                    .filter(|(_, label)| label.contains(term))
                    .count(),
            )
            .unwrap_or(u64::MAX);
            let ratio = population
                .checked_div(frequency.saturating_add(1))
                .unwrap_or(0)
                .saturating_add(1);
            (term, u64::from(ratio.ilog2()).saturating_add(1))
        })
        .collect::<Vec<_>>();
    let mut scores = labels
        .iter()
        .filter_map(|(node, label)| {
            let mut score = 0_u64;
            let mut coverage = 0_u64;
            for (term, weight) in &weights {
                if label.contains(*term) {
                    score = score.saturating_add(*weight);
                    coverage = coverage.saturating_add(1);
                }
            }
            (coverage != 0).then_some((score.saturating_mul(coverage), *node))
        })
        .collect::<Vec<_>>();
    sort_scores(&mut scores);
    scores
}

/// Diagnostic only: choose symbol/document seeds from complete exact and stemmed
/// channels. Corpus statistics include occurrences; graph traversal retains them.
pub(super) fn definition_channel_seeds(
    nodes: &[Node],
    query: &str,
    per_channel: usize,
) -> Vec<NodeId> {
    channel_seeds(nodes, query, per_channel, true)
}

pub(super) fn family_plan_within(
    nodes: &[Node],
    query: &str,
    eligible: &BTreeSet<NodeId>,
) -> Result<Vec<NodeId>, String> {
    let known = nodes.iter().map(|node| node.id).collect::<BTreeSet<_>>();
    if eligible.len() > 256 || !eligible.is_subset(&known) {
        return Err("packing plan exceeds capacity or contains foreign IDs".to_owned());
    }
    let mut plan = family_order_within(nodes, query, eligible, eligible.len());
    let ranked = plan.iter().copied().collect::<BTreeSet<_>>();
    plan.extend(eligible.difference(&ranked).copied());
    let primary = nodes
        .iter()
        .filter(|node| matches!(families::classify(&node.kind), "code" | "documentation"))
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    families::primary_first(&mut plan, &primary);
    Ok(plan)
}

pub(super) fn discovery_balanced_plan(
    nodes: &[Node],
    query: &str,
    eligible: &BTreeSet<NodeId>,
    original: &BTreeSet<NodeId>,
) -> Result<Vec<NodeId>, String> {
    let complete = family_plan_within(nodes, query, eligible)?;
    let mut plan = families::balance_discovery(&complete, original);
    let primary = nodes
        .iter()
        .filter(|node| matches!(families::classify(&node.kind), "code" | "documentation"))
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    families::primary_first(&mut plan, &primary);
    Ok(plan)
}

/// Diagnostic only: preserve occurrence candidates in both complete channels.
pub(super) fn all_fact_channel_seeds(
    nodes: &[Node],
    query: &str,
    per_channel: usize,
) -> Vec<NodeId> {
    channel_seeds(nodes, query, per_channel, false)
}

fn channel_seeds(
    nodes: &[Node],
    query: &str,
    per_channel: usize,
    definitions_only: bool,
) -> Vec<NodeId> {
    let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
    let exact = token_scores(nodes, query, syntaxmesh_query::identifier_terms);
    let stemmed = token_scores(nodes, query, |text| {
        syntaxmesh_query::identifier_terms(text)
            .iter()
            .map(|term| stemmer.stem(term).into_owned())
            .collect()
    });
    let mut seeds = BTreeSet::new();
    for channel in [&exact, &stemmed] {
        seeds.extend(
            channel
                .iter()
                .filter(|(_, node)| !definitions_only || definition_candidate(node))
                .take(per_channel.min(16))
                .map(|(_, node)| node.id),
        );
    }
    seeds.into_iter().collect()
}

/// Diagnostic allocation: two all-fact slots, four definition slots, two path
/// slots. Existing exact/stemmed selectors retain occurrences in graph traversal.
pub(super) fn mixed_channel_seeds(nodes: &[Node], query: &str, path_ids: &[NodeId]) -> Vec<NodeId> {
    if syntaxmesh_query::identifier_terms(query).is_empty() {
        return Vec::new();
    }
    let mut seeds = all_fact_channel_seeds(nodes, query, 1)
        .into_iter()
        .collect::<BTreeSet<_>>();
    seeds.extend(definition_channel_seeds(nodes, query, 2));
    seeds.extend(path_ids.iter().take(2).copied());
    seeds.into_iter().collect()
}

/// Diagnostic only: eight complete globally weighted family channels, bounded
/// by the existing explicit-seed request ceiling, without target-aware quotas.
pub(super) fn family_channel_seeds(nodes: &[Node], query: &str) -> Vec<NodeId> {
    family_seeds_within(nodes, query, &nodes.iter().map(|node| node.id).collect())
}

pub(super) fn family_seeds_within(
    nodes: &[Node],
    query: &str,
    eligible: &BTreeSet<NodeId>,
) -> Vec<NodeId> {
    family_order_within(nodes, query, eligible, 32)
}

fn family_order_within(
    nodes: &[Node],
    query: &str,
    eligible: &BTreeSet<NodeId>,
    limit: usize,
) -> Vec<NodeId> {
    let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
    let exact = token_scores(nodes, query, syntaxmesh_query::identifier_terms);
    let stemmed = token_scores(nodes, query, |text| {
        syntaxmesh_query::identifier_terms(text)
            .iter()
            .map(|term| stemmer.stem(term).into_owned())
            .collect()
    });
    let mut channels = Vec::new();
    for family in ["code", "documentation", "reference", "other"] {
        for scores in [&exact, &stemmed] {
            channels.push(
                scores
                    .iter()
                    .filter(|(_, node)| {
                        eligible.contains(&node.id) && families::classify(&node.kind) == family
                    })
                    .map(|(_, node)| node.id)
                    .collect(),
            );
        }
    }
    let mut seeds = families::refill_plan(&channels, limit);
    let primary = nodes
        .iter()
        .filter(|node| matches!(families::classify(&node.kind), "code" | "documentation"))
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    families::primary_first(&mut seeds, &primary);
    seeds
}

#[test]
fn definition_seeds_are_bounded_deterministic_and_preserve_both_channels() {
    let node = |ordinal: u64, name: &str, kind| Node {
        id: NodeId::derive(&[&ordinal.to_le_bytes()]),
        kind,
        name: name.to_owned(),
        owner_file: None,
        source: None,
        provenance: syntaxmesh_core::ProvenanceId::derive(&[b"seed-channel-probe"]),
        extension_payload: None,
    };
    let nodes = vec![
        node(1, "execution", syntaxmesh_core::NodeKind::Function),
        node(2, "execute", syntaxmesh_core::NodeKind::Function),
        node(
            3,
            "execution",
            syntaxmesh_core::NodeKind::Reference {
                relation: syntaxmesh_core::RelationKind::Calls,
            },
        ),
        node(4, "unrelated", syntaxmesh_core::NodeKind::DocumentChunk),
    ];
    let seeds = definition_channel_seeds(&nodes, "execution", 2);
    for entry in nodes.iter().take(3) {
        let report = stemmed_rank(&nodes, "execution", entry.id);
        assert!(
            report
                .get("target_rank")
                .is_some_and(serde_json::Value::is_number)
        );
        assert_eq!(
            report
                .get("definition_target_rank")
                .is_some_and(serde_json::Value::is_number),
            definition_candidate(entry)
        );
        assert!(
            report
                .get("top")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|rows| rows.iter().all(|row| {
                    row.get("kind").is_some()
                        && row.get("node_id").is_some()
                        && row.get("source").is_some()
                }))
        );
    }
    assert_eq!(seeds.len(), 2);
    assert!(nodes.iter().take(2).all(|entry| seeds.contains(&entry.id)));
    assert!(nodes.iter().skip(2).all(|entry| !seeds.contains(&entry.id)));
    assert_eq!(seeds, definition_channel_seeds(&nodes, "execution", 2));
    assert!(definition_channel_seeds(&nodes, "execution", 0).is_empty());
    assert!(definition_channel_seeds(&nodes, "", 2).is_empty());
    let many = (0..100)
        .map(|ordinal| node(ordinal, "execution", syntaxmesh_core::NodeKind::Function))
        .collect::<Vec<_>>();
    assert!(definition_channel_seeds(&many, "execution", usize::MAX).len() <= 32);
    let all_facts = all_fact_channel_seeds(&nodes, "execution", 4);
    assert_eq!(all_facts.len(), 3);
    assert!(
        nodes
            .iter()
            .take(3)
            .all(|entry| all_facts.contains(&entry.id))
    );
    assert_eq!(all_facts, all_fact_channel_seeds(&nodes, "execution", 4));
    assert!(all_fact_channel_seeds(&nodes, "execution", 0).is_empty());
    assert!(all_fact_channel_seeds(&nodes, "", 4).is_empty());
    assert!(all_fact_channel_seeds(&many, "execution", usize::MAX).len() <= 32);
    let paths = nodes.iter().map(|entry| entry.id).collect::<Vec<_>>();
    let mixed = mixed_channel_seeds(&nodes, "execution", &paths);
    let expected = all_fact_channel_seeds(&nodes, "execution", 1)
        .into_iter()
        .chain(definition_channel_seeds(&nodes, "execution", 2))
        .chain(paths.iter().take(2).copied())
        .collect::<BTreeSet<_>>();
    assert_eq!(mixed, expected.into_iter().collect::<Vec<_>>());
    assert!(mixed.len() <= 8);
    assert!(mixed_channel_seeds(&nodes, "", &paths).is_empty());
    let family = family_channel_seeds(&nodes, "execution");
    assert_eq!(family.len(), 3);
    assert!(nodes.iter().take(3).all(|entry| family.contains(&entry.id)));
    assert_eq!(family, family_channel_seeds(&nodes, "execution"));
    assert!(family_channel_seeds(&nodes, "").is_empty());
    assert_eq!(family_channel_seeds(&many, "execution").len(), 32);
    let selected = many
        .iter()
        .take(64)
        .map(|entry| entry.id)
        .collect::<BTreeSet<_>>();
    for question in ["execution", "no_matching_vocabulary", ""] {
        let complete = family_plan_within(&many, question, &selected);
        assert_eq!(
            complete
                .as_ref()
                .map(|plan| plan.iter().copied().collect::<BTreeSet<_>>()),
            Ok(selected.clone())
        );
        assert_eq!(complete.as_ref().map(Vec::len), Ok(64));
        assert_eq!(complete, family_plan_within(&many, question, &selected));
    }
    assert!(
        family_plan_within(
            &many,
            "execution",
            &[NodeId::derive(&[b"foreign-plan-id"])]
                .into_iter()
                .collect()
        )
        .is_err()
    );
    let large = (0..257)
        .map(|ordinal| node(ordinal, "execution", syntaxmesh_core::NodeKind::Function))
        .collect::<Vec<_>>();
    let capacity = large
        .iter()
        .take(256)
        .map(|entry| entry.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        family_plan_within(&large, "", &capacity)
            .as_ref()
            .map(Vec::len),
        Ok(256)
    );
    let excessive = large.iter().map(|entry| entry.id).collect::<BTreeSet<_>>();
    assert!(family_plan_within(&large, "execution", &excessive).is_err());
    let eligible = family.iter().take(2).copied().collect::<BTreeSet<_>>();
    let refined = family_seeds_within(&nodes, "execution", &eligible);
    assert!(refined.iter().all(|id| eligible.contains(id)));
    assert_eq!(refined.len(), 2);
    assert_eq!(refined, family_seeds_within(&nodes, "execution", &eligible));
    assert!(family_seeds_within(&nodes, "execution", &BTreeSet::new()).is_empty());
    let many_paths = (100..200)
        .map(|ordinal| node(ordinal, "execution", syntaxmesh_core::NodeKind::Function).id)
        .collect::<Vec<_>>();
    let bounded = mixed_channel_seeds(&many, "execution", &many_paths);
    assert!(bounded.len() <= 8);
    assert_eq!(
        many_paths.iter().filter(|id| bounded.contains(id)).count(),
        2
    );
    assert_eq!(
        bounded,
        mixed_channel_seeds(&many, "execution", &many_paths)
    );
}

#[test]
fn snowball_characterization_matches_observed_identifier_morphology() {
    let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
    for (left, right) in [
        ("guardrail", "guardrails"),
        ("execute", "execution"),
        ("retry", "retries"),
    ] {
        assert_eq!(stemmer.stem(left), stemmer.stem(right));
    }
    // Stemming is not a substitute for identifier splitting.
    let normalized = syntaxmesh_query::identifier_terms("executeWithRetry");
    let stems = normalized
        .iter()
        .map(|term| stemmer.stem(term).into_owned())
        .collect::<BTreeSet<_>>();
    assert!(stems.contains(stemmer.stem("execution").as_ref()));
    assert!(stems.contains(stemmer.stem("retries").as_ref()));
}

/// Test-only full-corpus oracle, not a bounded production retrieval API.
/// Uses integer logarithmic rarity and query coverage, inspired by Graphify.
pub(super) fn rank(nodes: &[Node], query: &str, target: NodeId) -> serde_json::Value {
    let terms = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect::<BTreeSet<_>>();
    let labels = nodes
        .iter()
        .map(|node| {
            (
                node.id,
                node.name.to_lowercase(),
                matches!(
                    node.kind,
                    syntaxmesh_core::NodeKind::Function
                        | syntaxmesh_core::NodeKind::Class
                        | syntaxmesh_core::NodeKind::Struct
                        | syntaxmesh_core::NodeKind::Enum
                        | syntaxmesh_core::NodeKind::Trait
                        | syntaxmesh_core::NodeKind::Test
                        | syntaxmesh_core::NodeKind::Module
                        | syntaxmesh_core::NodeKind::File
                        | syntaxmesh_core::NodeKind::Script
                        | syntaxmesh_core::NodeKind::Document
                        | syntaxmesh_core::NodeKind::Section
                        | syntaxmesh_core::NodeKind::DocumentChunk
                ),
            )
        })
        .collect::<Vec<_>>();
    let population = u64::try_from(nodes.len()).unwrap_or(u64::MAX);
    let weights = terms
        .iter()
        .map(|term| {
            let frequency = u64::try_from(
                labels
                    .iter()
                    .filter(|(_, label, _)| label.contains(term.as_str()))
                    .count(),
            )
            .unwrap_or(u64::MAX);
            let ratio = population
                .checked_div(frequency.saturating_add(1))
                .unwrap_or(0)
                .saturating_add(1);
            (term, u64::from(ratio.ilog2()).saturating_add(1))
        })
        .collect::<Vec<_>>();
    let mut scored = labels
        .into_iter()
        .filter_map(|(id, label, declaration)| {
            let mut score = 0_u64;
            let mut coverage = 0_u64;
            for (term, weight) in &weights {
                if label.contains(term.as_str()) {
                    score = score.saturating_add(*weight);
                    coverage = coverage.saturating_add(1);
                }
            }
            if coverage == 0 {
                return None;
            }
            Some((score.saturating_mul(coverage), id, label, declaration))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let rank = scored
        .iter()
        .position(|(_, id, _, _)| *id == target)
        .map(|position| position.saturating_add(1));
    let declaration_rank = scored
        .iter()
        .filter(|entry| entry.3)
        .position(|entry| entry.1 == target)
        .map(|position| position.saturating_add(1));
    let summary = |entry: &(u64, NodeId, String, bool)| serde_json::json!({"score":entry.0,"id":entry.1.0.to_hex(),"label":entry.2.chars().take(240).collect::<String>()});
    serde_json::json!({"population": population, "matches": scored.len(), "target_rank": rank,
        "declaration_document_rank": declaration_rank,
        "top": scored.iter().take(5).map(summary).collect::<Vec<_>>(),
        "declaration_document_top": scored.iter().filter(|entry| entry.3).take(5).map(summary).collect::<Vec<_>>()})
}

#[test]
fn corpus_probe_prefers_rare_multi_term_match_over_frequent_single_word() {
    let target = NodeId::derive(&[b"rare-transaction-loader"]);
    let node = |id, name: &str| Node {
        id,
        kind: syntaxmesh_core::NodeKind::Function,
        name: name.to_owned(),
        owner_file: None,
        source: None,
        provenance: syntaxmesh_core::ProvenanceId::derive(&[b"probe"]),
        extension_payload: None,
    };
    let mut nodes = (0_u64..100)
        .map(|ordinal| node(NodeId::derive(&[&ordinal.to_le_bytes()]), "Record"))
        .collect::<Vec<_>>();
    nodes.push(node(target, "loadTransactionLog"));
    let file_id = syntaxmesh_core::FileId::derive(&[b"ownership-probe"]);
    let mut owned_nodes = nodes.clone();
    if let Some(owned) = owned_nodes.iter_mut().find(|entry| entry.id == target) {
        owned.owner_file = Some(file_id);
    }
    let enriched = with_file_paths(
        &owned_nodes,
        &[syntaxmesh_core::FileVersion {
            file_id,
            normalized_path: "src/retry_workflow.rs".to_owned(),
            content_hash: [0; 32],
            size_bytes: 0,
        }],
    );
    assert_eq!(with_file_paths(&owned_nodes, &[]), owned_nodes);
    assert_eq!(
        owned_nodes
            .iter()
            .find(|entry| entry.id == target)
            .map(|entry| entry.name.as_str()),
        Some("loadTransactionLog")
    );
    assert_eq!(
        stemmed_rank(&enriched, "retry workflow", target)
            .get("exact_rank")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let stemmed = stemmed_rank(&nodes, "loads transactions", target);
    assert_eq!(
        stemmed
            .get("zero_offset_fusion_rank")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        stemmed
            .get("reciprocal_fusion_rank")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(stemmed, stemmed_rank(&nodes, "loads transactions", target));
    assert_eq!(
        stemmed
            .get("target_rank")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let result = rank(&nodes, "load transaction record", target);
    assert_eq!(
        result
            .get("target_rank")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    for ordinal in 100_u64..110 {
        let mut reference = node(
            NodeId::derive(&[&ordinal.to_le_bytes()]),
            "load transaction record expression",
        );
        reference.kind = syntaxmesh_core::NodeKind::Reference {
            relation: syntaxmesh_core::RelationKind::Calls,
        };
        nodes.push(reference);
    }
    let mixed = rank(&nodes, "load transaction record", target);
    assert!(
        mixed
            .get("target_rank")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|position| position > 1)
    );
    assert_eq!(
        mixed
            .get("declaration_document_rank")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
}
