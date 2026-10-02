use super::tests::{node, reference};
use super::*;

#[test]
fn unmatched_qualified_calls_never_bind_an_unrelated_terminal_name() -> Result<(), String> {
    let definition = node(b"qualified-other", "actual_module::helper");
    for target in [
        "wrong_module::helper",
        "wrong_module < T >::helper",
        "missing::nested::helper",
    ] {
        let resolved = resolve_references(std::slice::from_ref(&definition), &[reference(target)]);
        if resolved.reference_nodes.first().map(|node| &node.kind)
            != Some(&NodeKind::UnresolvedReference {
                relation: RelationKind::Calls,
            })
            || resolved.edges.iter().any(|edge| {
                matches!(
                    edge.relation,
                    RelationKind::Calls | RelationKind::ResolvesTo
                )
            })
        {
            return Err(format!(
                "qualified miss guessed an unrelated definition: {target}"
            ));
        }
    }
    for target in ["actual_module::helper", "helper"] {
        let resolved = resolve_references(std::slice::from_ref(&definition), &[reference(target)]);
        if !resolved
            .edges
            .iter()
            .any(|edge| edge.target == definition.id && edge.relation == RelationKind::Calls)
        {
            return Err(format!(
                "existing exact/unqualified lookup changed: {target}"
            ));
        }
    }
    Ok(())
}
