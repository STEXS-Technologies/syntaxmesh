use super::tests::{node, reference};
use super::*;

#[test]
fn implementation_references_bind_only_traits_without_making_traits_callable() -> Result<(), String>
{
    let mut contract = node(b"contract-trait", "api::Contract");
    contract.kind = NodeKind::Trait;
    let function = node(b"contract-function", "api::Contract");
    let definitions = [contract.clone(), function.clone()];
    for spelling in ["api::Contract", "Contract"] {
        let mut implementation = reference(spelling);
        implementation.relation = RelationKind::Implements;
        let result = resolve_references(&definitions, &[implementation]);
        if !result
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Implements && edge.target == contract.id)
            || result.edges.iter().any(|edge| edge.target == function.id)
        {
            return Err("implementation did not select only the trait".to_owned());
        }
    }
    let call = resolve_references(
        std::slice::from_ref(&contract),
        &[reference("api::Contract")],
    );
    if call.edges.iter().any(|edge| {
        matches!(
            edge.relation,
            RelationKind::Calls | RelationKind::ResolvesTo
        )
    }) {
        return Err("trait declaration became a callable target".to_owned());
    }
    let mut absent_trait = reference("api::Contract");
    absent_trait.relation = RelationKind::Implements;
    let unresolved = resolve_references(&[function], &[absent_trait]);
    if !matches!(
        unresolved.reference_nodes.first().map(|node| &node.kind),
        Some(NodeKind::UnresolvedReference {
            relation: RelationKind::Implements
        })
    ) {
        return Err("non-trait target did not remain unresolved".to_owned());
    }
    Ok(())
}

#[test]
fn implementation_ambiguity_and_qualified_misses_remain_explicit() -> Result<(), String> {
    let mut first = node(b"first-contract", "api::Contract");
    first.kind = NodeKind::Trait;
    let mut second = node(b"second-contract", "api::Contract");
    second.kind = NodeKind::Trait;
    for (spelling, ambiguous) in [("api::Contract", true), ("wrong::Contract", false)] {
        let mut implementation = reference(spelling);
        implementation.relation = RelationKind::Implements;
        let result = resolve_references(&[first.clone(), second.clone()], &[implementation]);
        let expected = if ambiguous {
            NodeKind::AmbiguousReference {
                relation: RelationKind::Implements,
            }
        } else {
            NodeKind::UnresolvedReference {
                relation: RelationKind::Implements,
            }
        };
        if result.reference_nodes.first().map(|node| &node.kind) != Some(&expected)
            || result.edges.iter().any(|edge| {
                matches!(
                    edge.relation,
                    RelationKind::Implements | RelationKind::ResolvesTo
                )
            })
        {
            return Err("trait ambiguity or qualified miss was guessed".to_owned());
        }
    }
    Ok(())
}
