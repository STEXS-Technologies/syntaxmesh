use super::*;
use syntaxmesh_core::{FileId, ProvenanceId, SourceLocation, SourceSpan};

#[test]
fn constrained_reference_does_not_bind_a_matching_definition() -> Result<(), String> {
    let definition = node(b"definition", "perform");
    let mut occurrence = reference("perform");
    occurrence.resolution = syntaxmesh_language_sdk::ReferenceResolution::Unresolved;
    let result = resolve_references(&[definition], &[occurrence.clone()]);
    let persisted = result.reference_nodes.first().ok_or("missing occurrence")?;
    if !matches!(persisted.kind, NodeKind::UnresolvedReference { .. })
        || persisted.name != occurrence.target
        || persisted.source.as_ref() != Some(&occurrence.source_location)
        || syntaxmesh_language_sdk::ReferenceResolution::from_payload(
            persisted.extension_payload.as_ref(),
        )
        .map_err(|error| error.to_string())?
            != occurrence.resolution
        || result.edges.len() != 1
        || result
            .edges
            .first()
            .is_none_or(|edge| edge.relation != RelationKind::References)
    {
        return Err("constraint or evidence was lost during resolution".to_owned());
    }
    Ok(())
}

pub(super) fn node(id: &[u8], name: &str) -> Node {
    Node {
        id: NodeId::derive(&[id]),
        kind: NodeKind::Function,
        name: name.to_owned(),
        owner_file: Some(FileId::derive(&[b"src/definition.rs"])),
        source: None,
        provenance: ProvenanceId::derive(&[b"definition-provenance"]),
        extension_payload: None,
    }
}

pub(super) fn reference(target: &str) -> Reference {
    Reference {
        resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
        id: NodeId::derive(&[b"reference"]),
        source: NodeId::derive(&[b"caller"]),
        target: target.to_owned(),
        relation: RelationKind::Calls,
        source_location: SourceLocation {
            file_id: FileId::derive(&[b"src/caller.rs"]),
            content_hash: [7; 32],
            span: SourceSpan {
                start_byte: 3,
                end_byte: 8,
            },
        },
        provenance: ProvenanceId::derive(&[b"reference-provenance"]),
    }
}

#[test]
fn resolves_a_unique_cross_module_terminal_name() -> Result<(), String> {
    let definition = node(b"definition", "library::perform");
    let result = resolve_references(std::slice::from_ref(&definition), &[reference("perform")]);
    if result.reference_nodes.first().is_none_or(|item| {
        item.kind
            != (NodeKind::Reference {
                relation: RelationKind::Calls,
            })
    }) || !result.edges.iter().any(|edge| {
        edge.source == NodeId::derive(&[b"caller"])
            && edge.target == definition.id
            && edge.relation == RelationKind::Calls
    }) {
        return Err("unique symbol was not resolved across files".to_owned());
    }
    Ok(())
}

#[test]
fn resolves_qualified_generic_impl_method_without_terminal_guessing() -> Result<(), String> {
    let definition = node(b"generic-method", "SyntaxMeshEngine < S , E >::new");
    let result = resolve_references(
        std::slice::from_ref(&definition),
        &[reference("SyntaxMeshEngine::new")],
    );
    if result.reference_nodes.first().is_none_or(|item| {
        item.kind
            != (NodeKind::Reference {
                relation: RelationKind::Calls,
            })
    }) || !result.edges.iter().any(|edge| {
        edge.source == NodeId::derive(&[b"caller"])
            && edge.target == definition.id
            && edge.relation == RelationKind::Calls
    }) {
        return Err("qualified generic impl method was not resolved exactly".to_owned());
    }
    Ok(())
}

#[test]
fn resolves_constructor_references_to_struct_definitions() -> Result<(), String> {
    let mut definition = node(b"struct-definition", "library::Widget");
    definition.kind = NodeKind::Struct;
    let result = resolve_references(std::slice::from_ref(&definition), &[reference("Widget")]);
    if result.reference_nodes.first().is_none_or(|item| {
        item.kind
            != (NodeKind::Reference {
                relation: RelationKind::Calls,
            })
    }) || !result.edges.iter().any(|edge| {
        edge.source == NodeId::derive(&[b"caller"])
            && edge.target == definition.id
            && edge.relation == RelationKind::Calls
    }) {
        return Err("constructor reference did not resolve to its struct definition".to_owned());
    }
    Ok(())
}

#[test]
fn source_anchor_names_are_byte_exact_and_have_no_terminal_alias() -> Result<(), String> {
    let mut anchor = node(b"anchor", "docs/my guide.md#résumé");
    anchor.kind = NodeKind::External {
        namespace: syntaxmesh_language_sdk::SOURCE_ANCHOR_NAMESPACE.to_owned(),
        kind: "anchor".to_owned(),
    };
    for (target, should_resolve) in [
        ("docs/my guide.md#résumé", true),
        ("docs/myguide.md#résumé", false),
        ("docs/my guide.md#Résumé", false),
        ("résumé", false),
    ] {
        let mut link = reference(target);
        link.relation = RelationKind::References;
        let result = resolve_references(std::slice::from_ref(&anchor), &[link]);
        if result
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::ResolvesTo)
            != should_resolve
        {
            return Err("source anchor was normalized or resolved by a terminal alias".to_owned());
        }
    }
    let mut semantic = anchor;
    semantic.kind = NodeKind::External {
        namespace: "syntaxmesh.semantic".to_owned(),
        kind: "anchor".to_owned(),
    };
    if resolve_references(&[semantic], &[reference("docs/my guide.md#résumé")])
        .edges
        .iter()
        .any(|edge| edge.relation == RelationKind::ResolvesTo)
    {
        return Err("an unrelated extension namespace became a source target".to_owned());
    }
    Ok(())
}

#[test]
fn resolves_exact_paths_to_indexed_document_nodes() -> Result<(), String> {
    let mut document = node(b"document", "docs/adr/decision.md");
    document.kind = NodeKind::Document;
    let mut link = reference("docs/adr/decision.md");
    link.relation = RelationKind::References;
    let result = resolve_references(&[document.clone()], &[link]);
    if result.reference_nodes.first().is_none_or(|item| {
        item.kind
            != (NodeKind::Reference {
                relation: RelationKind::References,
            })
    }) || !result
        .edges
        .iter()
        .any(|edge| edge.target == document.id && edge.relation == RelationKind::ResolvesTo)
    {
        return Err("local Markdown path did not resolve to its document node".to_owned());
    }
    Ok(())
}

#[test]
fn resolves_exact_paths_to_indexed_source_files_without_terminal_guessing() -> Result<(), String> {
    let mut file = node(b"source-file", "packages/library/src/lib.rs");
    file.kind = NodeKind::File;
    let mut link = reference("packages/library/src/lib.rs");
    link.relation = RelationKind::References;
    let result = resolve_references(&[file.clone()], &[link]);
    if !result
        .edges
        .iter()
        .any(|edge| edge.target == file.id && edge.relation == RelationKind::ResolvesTo)
    {
        return Err("exact source-file path did not resolve to its indexed file".to_owned());
    }

    let mut basename_link = reference("lib.rs");
    basename_link.relation = RelationKind::References;
    let basename_result = resolve_references(&[file], &[basename_link]);
    if basename_result
        .edges
        .iter()
        .any(|edge| edge.relation == RelationKind::ResolvesTo)
    {
        return Err("source-file link guessed a target from its basename".to_owned());
    }
    Ok(())
}

#[test]
fn preserves_missing_and_ambiguous_targets_explicitly() -> Result<(), String> {
    let missing = resolve_references(&[], &[reference("missing")]);
    let ambiguous = resolve_references(
        &[node(b"first", "left::run"), node(b"second", "right::run")],
        &[reference("run")],
    );
    if missing
        .reference_nodes
        .first()
        .is_none_or(|item| !matches!(item.kind, NodeKind::UnresolvedReference { .. }))
        || ambiguous
            .reference_nodes
            .first()
            .is_none_or(|item| !matches!(item.kind, NodeKind::AmbiguousReference { .. }))
    {
        return Err("missing or ambiguous target was not retained explicitly".to_owned());
    }
    Ok(())
}

#[test]
fn import_specifiers_never_resolve_as_terminal_symbol_names() -> Result<(), String> {
    let function = node(b"function", "utils");
    let mut import = reference("./utils");
    import.relation = RelationKind::Imports;
    let result = resolve_references(std::slice::from_ref(&function), &[import]);
    let unresolved_import = result.reference_nodes.first().is_some_and(|reference| {
        reference.kind
            == (NodeKind::UnresolvedReference {
                relation: RelationKind::Imports,
            })
    });
    if !unresolved_import
        || result
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::ResolvesTo && edge.target == function.id)
    {
        return Err("module specifier was guessed as a terminal symbol".to_owned());
    }
    Ok(())
}
