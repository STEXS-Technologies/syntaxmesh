use super::*;
use std::collections::BTreeMap;
use syntaxmesh_core::{
    Edge, EdgeId, GenerationId, ProvenanceId, RepositoryId, StableId, WorktreeId,
};

pub(crate) fn fixture() -> (BTreeSet<NodeId>, Vec<Premise>, Rule, Limits) {
    let ids = [
        b"entry".as_slice(),
        b"middle",
        b"writer",
        b"store",
        b"concept",
    ]
    .map(|name| NodeId::derive(&[name]));
    let [entry, middle, writer, store, concept] = ids;
    let external = |relation: &str| RelationKind::External {
        namespace: "example".to_owned(),
        relation: relation.to_owned(),
    };
    let premises = [
        (entry, middle, RelationKind::Calls),
        (middle, writer, RelationKind::Calls),
        (writer, store, external("writes")),
        (store, concept, external("instance_of")),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (source, target, relation))| Premise {
        generation: GenerationId::derive(&[b"generation"]),
        valid_from: GenerationId::derive(&[b"generation"]),
        edge: Edge {
            id: EdgeId::derive(&[index.to_string().as_bytes()]),
            source,
            target,
            relation,
            provenance: ProvenanceId::derive(&[b"declared"]),
            extension_payload: None,
        },
    })
    .collect();
    let rule = Rule {
        id: "example:write_path".to_owned(),
        version: "1".to_owned(),
        ontology_versions: BTreeMap::from([("example".to_owned(), "1".to_owned())]),
        configuration_fingerprint: StableId::derive("config", &[b"1"]),
        path_relation: RelationKind::Calls,
        join_relation: external("writes"),
        declaration_relation: external("instance_of"),
        concept,
        output_relation: external("participates_in_write"),
    };
    (
        ids.into_iter().collect(),
        premises,
        rule,
        Limits {
            input_bytes: 16_384,
            facts: 9,
            edge_visits: 100,
            depth: 2,
            claims: 2,
        },
    )
}

pub(crate) fn selection<'facts>(
    nodes: &'facts BTreeSet<NodeId>,
    premises: &'facts [Premise],
) -> Selection<'facts> {
    Selection {
        repository: RepositoryId::derive(&[b"repo"]),
        worktree: WorktreeId::derive(&[b"worktree"]),
        generation: GenerationId::derive(&[b"generation"]),
        nodes,
        premises,
    }
}

#[test]
fn path_join_retains_exact_witness_and_is_input_order_invariant() -> Result<(), InferenceError> {
    let (nodes, mut premises, rule, limits) = fixture();
    let original = evaluate(&selection(&nodes, &premises), &rule, &limits)?;
    if original.len() != 2
        || original
            .iter()
            .any(|claim| claim.derivation_class != DerivationClass::DeterministicRule)
        || !original.iter().any(|claim| claim.premises.len() == 4)
    {
        return Err(InferenceError::InvalidInput);
    }
    premises.reverse();
    if evaluate(&selection(&nodes, &premises), &rule, &limits)? != original {
        return Err(InferenceError::InvalidInput);
    }
    premises.retain(|premise| premise.edge.relation != rule.declaration_relation);
    if !evaluate(&selection(&nodes, &premises), &rule, &limits)?.is_empty() {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn mixed_generation_and_exhausted_limits_never_return_partial_claims() -> Result<(), InferenceError>
{
    let (nodes, mut premises, rule, mut limits) = fixture();
    for change in 0..3 {
        match change {
            0 => limits.edge_visits = 1,
            1 => {
                limits.edge_visits = 100;
                limits.depth = 1;
            }
            _ => {
                limits.depth = 2;
                limits.claims = 1;
            }
        }
        if evaluate(&selection(&nodes, &premises), &rule, &limits).is_ok() {
            return Err(InferenceError::InvalidInput);
        }
    }
    limits.claims = 2;
    if let Some(premise) = premises.first_mut() {
        premise.generation = GenerationId::derive(&[b"foreign"]);
    }
    if evaluate(&selection(&nodes, &premises), &rule, &limits) != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn rule_versions_ontology_and_configuration_change_claim_identity() -> Result<(), InferenceError> {
    let (nodes, premises, rule, limits) = fixture();
    let original = evaluate(&selection(&nodes, &premises), &rule, &limits)?;
    for change in 0..3 {
        let mut changed = rule.clone();
        match change {
            0 => changed.version = "2".to_owned(),
            1 => {
                changed
                    .ontology_versions
                    .insert("example".to_owned(), "2".to_owned());
            }
            _ => changed.configuration_fingerprint = StableId::derive("config", &[b"2"]),
        }
        let next = evaluate(&selection(&nodes, &premises), &changed, &limits)?;
        if next
            .iter()
            .any(|claim| original.iter().any(|previous| previous.id == claim.id))
        {
            return Err(InferenceError::InvalidInput);
        }
    }
    Ok(())
}

#[test]
fn cycle_detection_preserves_finite_nonrepeating_witnesses() -> Result<(), InferenceError> {
    let (nodes, mut premises, rule, mut limits) = fixture();
    let original = evaluate(&selection(&nodes, &premises), &rule, &limits)?;
    let mut cycle = premises
        .first()
        .cloned()
        .ok_or(InferenceError::InvalidInput)?;
    cycle.edge.id = EdgeId::derive(&[b"cycle"]);
    cycle.edge.source = NodeId::derive(&[b"writer"]);
    cycle.edge.target = NodeId::derive(&[b"entry"]);
    premises.push(cycle);
    limits.facts = 10;
    if evaluate(&selection(&nodes, &premises), &rule, &limits)? != original {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn duplicate_dangling_and_reserved_output_inputs_reject() -> Result<(), InferenceError> {
    let (nodes, mut premises, mut rule, mut limits) = fixture();
    let duplicate = premises
        .first()
        .cloned()
        .ok_or(InferenceError::InvalidInput)?;
    premises.push(duplicate);
    limits.facts = 10;
    if evaluate(&selection(&nodes, &premises), &rule, &limits) != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    premises.pop();
    let first = premises.first_mut().ok_or(InferenceError::InvalidInput)?;
    first.edge.target = NodeId::derive(&[b"missing"]);
    if evaluate(&selection(&nodes, &premises), &rule, &limits) != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    rule.output_relation = RelationKind::Calls;
    if evaluate(&selection(&nodes, &[]), &rule, &limits) != Err(InferenceError::InvalidInput) {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}
