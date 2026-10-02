//! Conservative name resolution and explicit unresolved-reference projection.

use std::collections::{BTreeMap, BTreeSet};

use syntaxmesh_core::{Edge, EdgeId, Node, NodeId, NodeKind, RelationKind};
use syntaxmesh_language_sdk::{Reference, is_reference_target_kind, is_source_anchor};

/// Static reference-resolution policy revision for host no-op planning.
pub const REFERENCE_RESOLVER_REVISION: &[u8] = b"syntaxmesh-reference-resolution-v4";

#[cfg(test)]
mod qualification_tests;

#[cfg(test)]
mod trait_tests;

/// Canonical graph facts produced by resolving a batch of extracted references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// Occurrence nodes retain target spelling and source evidence even when
    /// the reference resolves successfully.
    pub reference_nodes: Vec<Node>,
    /// Source-to-occurrence, occurrence-to-target, and direct semantic edges.
    pub edges: Vec<Edge>,
}

/// Resolve unique exact matches or unqualified terminal-name matches. Qualified
/// misses never fall back to unrelated terminal names; other references remain
/// explicit unresolved or ambiguous nodes.
#[must_use]
pub fn resolve_references(definitions: &[Node], references: &[Reference]) -> Resolution {
    let symbols = symbol_index(definitions);
    let mut result = Resolution {
        reference_nodes: Vec::with_capacity(references.len()),
        edges: Vec::with_capacity(references.len().saturating_mul(3)),
    };
    let mut edge_ids = BTreeSet::new();

    for reference in references {
        // Module specifiers are not symbol names. Until a module-aware strategy
        // is available, keep Imports explicit and unresolved instead of
        // accidentally binding `./utils` to a function named `utils`.
        let matches = if reference.relation == RelationKind::Imports
            || reference.resolution == syntaxmesh_language_sdk::ReferenceResolution::Unresolved
        {
            &[]
        } else {
            matching_symbols(&symbols, &reference.target, &reference.relation)
        };
        let resolved = (matches.len() == 1)
            .then(|| matches.first().copied())
            .flatten();
        let kind = match (matches.is_empty(), resolved.is_some()) {
            (true, _) => NodeKind::UnresolvedReference {
                relation: reference.relation.clone(),
            },
            (false, true) => NodeKind::Reference {
                relation: reference.relation.clone(),
            },
            (false, false) => NodeKind::AmbiguousReference {
                relation: reference.relation.clone(),
            },
        };
        result.reference_nodes.push(Node {
            id: reference.id,
            kind,
            name: reference.target.clone(),
            owner_file: Some(reference.source_location.file_id),
            source: Some(reference.source_location.clone()),
            provenance: reference.provenance,
            extension_payload: reference.resolution.payload(),
        });
        push_edge(
            &mut result.edges,
            &mut edge_ids,
            reference.source,
            reference.id,
            RelationKind::References,
            reference.provenance,
            b"reference-occurrence",
        );
        if let Some(target) = resolved {
            push_edge(
                &mut result.edges,
                &mut edge_ids,
                reference.id,
                target,
                RelationKind::ResolvesTo,
                reference.provenance,
                b"reference-resolution",
            );
            push_edge(
                &mut result.edges,
                &mut edge_ids,
                reference.source,
                target,
                reference.relation.clone(),
                reference.provenance,
                b"resolved-reference",
            );
        }
    }
    result
}

#[derive(Default)]
struct SymbolIndex {
    symbols: BTreeMap<String, Vec<NodeId>>,
    traits: BTreeMap<String, Vec<NodeId>>,
    anchors: BTreeMap<String, Vec<NodeId>>,
}

fn symbol_index(definitions: &[Node]) -> SymbolIndex {
    let mut index = SymbolIndex::default();
    for definition in definitions {
        if !is_reference_target_kind(&definition.kind) {
            continue;
        }
        if is_source_anchor(&definition.kind) {
            index
                .anchors
                .entry(definition.name.clone())
                .or_default()
                .push(definition.id);
            continue;
        }
        let canonical = canonical_symbol(&definition.name);
        let symbols = if definition.kind == NodeKind::Trait {
            &mut index.traits
        } else {
            &mut index.symbols
        };
        symbols
            .entry(canonical.clone())
            .or_default()
            .push(definition.id);
        if let Some(terminal) = canonical.rsplit("::").next()
            && terminal != canonical
        {
            symbols
                .entry(terminal.to_owned())
                .or_default()
                .push(definition.id);
        }
    }
    for ids in index
        .symbols
        .values_mut()
        .chain(index.traits.values_mut())
        .chain(index.anchors.values_mut())
    {
        ids.sort_unstable();
        ids.dedup();
    }
    index
}

fn matching_symbols<'symbols>(
    index: &'symbols SymbolIndex,
    target: &str,
    relation: &RelationKind,
) -> &'symbols [NodeId] {
    if relation != &RelationKind::Implements
        && let Some(exact) = index.anchors.get(target)
        && !exact.is_empty()
    {
        return exact;
    }
    let symbols = if relation == &RelationKind::Implements {
        &index.traits
    } else {
        &index.symbols
    };
    let canonical = canonical_symbol(target);
    if let Some(exact) = symbols.get(&canonical)
        && !exact.is_empty()
    {
        return exact;
    }
    if canonical.contains("::") {
        return &[];
    }
    canonical
        .rsplit("::")
        .next()
        .and_then(|terminal| symbols.get(terminal))
        .map_or(&[], Vec::as_slice)
}

/// Normalize the token spacing and generic arguments produced by Rust's
/// token-stream formatter so `Type < T >::method` can match the source path
/// `Type::method`. Generic arguments do not identify a distinct definition in
/// this syntax-only resolver; any resulting overload ambiguity stays explicit.
fn canonical_symbol(symbol: &str) -> String {
    let mut canonical = String::with_capacity(symbol.len());
    let mut generic_depth = 0_usize;
    for character in symbol.chars() {
        match character {
            '<' => generic_depth = generic_depth.saturating_add(1),
            '>' => generic_depth = generic_depth.saturating_sub(1),
            _ if generic_depth > 0 || character.is_whitespace() => {}
            _ => canonical.push(character),
        }
    }
    canonical
}

fn push_edge(
    edges: &mut Vec<Edge>,
    seen: &mut BTreeSet<EdgeId>,
    source: NodeId,
    target: NodeId,
    relation: RelationKind,
    provenance: syntaxmesh_core::ProvenanceId,
    identity_tag: &[u8],
) {
    let relation_name = format!("{relation:?}");
    let id = EdgeId::derive(&[
        &source.0.0,
        &target.0.0,
        identity_tag,
        relation_name.as_bytes(),
    ]);
    if seen.insert(id) {
        edges.push(Edge {
            id,
            source,
            target,
            relation,
            provenance,
            extension_payload: None,
        });
    }
}

#[cfg(test)]
mod tests;
