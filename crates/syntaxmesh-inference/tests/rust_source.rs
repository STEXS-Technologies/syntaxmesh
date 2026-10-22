//! Structural source facts and explicit declarations compose without AI.
use std::collections::BTreeMap;
use std::error::Error;
use syntaxmesh_core::{
    Edge, EdgeId, FileId, FileVersion, GenerationId, NodeId, ProvenanceId, RelationKind,
    RepositoryId, StableId, WorktreeId,
};
use syntaxmesh_inference::{Limits, Premise, Rule, Selection, evaluate};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::{LanguageExtractor, SourceFile};

#[test]
fn extracted_rust_call_path_joins_explicit_domain_premises() -> Result<(), Box<dyn Error>> {
    let content = "fn checkout() { payment(); } fn payment() { writer(); } fn writer() {}";
    let source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"lib.rs"]),
            normalized_path: "lib.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content: content.to_owned(),
    };
    let extraction = RustExtractor.extract(&source)?;
    let resolved =
        syntaxmesh_resolver::resolve_references(&extraction.nodes, &extraction.references);
    let find = |name: &str| {
        extraction
            .nodes
            .iter()
            .find(|node| node.name == name)
            .map(|node| node.id)
            .ok_or_else(|| format!("missing Rust definition {name}"))
    };
    let checkout = find("checkout")?;
    let writer = find("writer")?;
    let store = NodeId::derive(&[b"ledger"]);
    let concept = NodeId::derive(&[b"financial-ledger"]);
    let generation = GenerationId::derive(&[b"source-generation"]);
    let external = |relation: &str| RelationKind::External {
        namespace: "example.finance".to_owned(),
        relation: relation.to_owned(),
    };
    let mut nodes = extraction
        .nodes
        .iter()
        .chain(&resolved.reference_nodes)
        .map(|node| node.id)
        .collect::<std::collections::BTreeSet<_>>();
    nodes.extend([store, concept]);
    let mut edges = extraction.edges.clone();
    edges.extend(resolved.edges);
    for (source_id, target, relation) in [
        (writer, store, external("writes")),
        (store, concept, external("instance_of")),
    ] {
        edges.push(Edge {
            id: EdgeId::derive(&[format!("{relation:?}").as_bytes()]),
            source: source_id,
            target,
            relation,
            provenance: ProvenanceId::derive(&[b"explicit-domain-declaration"]),
            extension_payload: None,
        });
    }
    let mut premises = edges
        .into_iter()
        .map(|edge| Premise {
            generation,
            valid_from: generation,
            edge,
        })
        .collect::<Vec<_>>();
    let rule = Rule {
        id: "example.finance:write_path".to_owned(),
        version: "1".to_owned(),
        ontology_versions: BTreeMap::from([("example.finance".to_owned(), "1".to_owned())]),
        configuration_fingerprint: StableId::derive("config", &[b"fixture"]),
        path_relation: RelationKind::Calls,
        join_relation: external("writes"),
        declaration_relation: external("instance_of"),
        concept,
        output_relation: external("participates_in_financial_write"),
    };
    let limits = Limits {
        input_bytes: 65_536,
        facts: 100,
        edge_visits: 1000,
        depth: 4,
        claims: 100,
    };
    let run = |facts: &[Premise]| {
        evaluate(
            &Selection {
                repository: RepositoryId::derive(&[b"repository"]),
                worktree: WorktreeId::derive(&[b"worktree"]),
                generation,
                nodes: &nodes,
                premises: facts,
            },
            &rule,
            &limits,
        )
    };
    let before = run(&premises)?;
    let claim = before
        .iter()
        .find(|claim| claim.source == checkout && claim.target == store)
        .ok_or("Rust call path did not produce its declared domain claim")?;
    if claim.premises.len() != 4 {
        return Err("claim lost exact transitive premises".into());
    }
    let source_calls = premises
        .iter()
        .filter(|premise| premise.edge.relation == RelationKind::Calls)
        .count();
    premises.retain(|premise| premise.edge.relation != rule.declaration_relation);
    if !run(&premises)?.is_empty()
        || premises
            .iter()
            .filter(|premise| premise.edge.relation == RelationKind::Calls)
            .count()
            != source_calls
    {
        return Err(
            "premise removal did not invalidate claims independently of source facts".into(),
        );
    }
    Ok(())
}
