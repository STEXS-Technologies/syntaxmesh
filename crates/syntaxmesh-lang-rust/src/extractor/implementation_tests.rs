use super::*;

const CONTENT: &str = "trait A { fn run(); } trait B { fn run(); } struct Service; impl A for Service { fn run() { first(); } } impl B for Service { fn run() { second(); } } impl !Missing for Service {} impl Service { fn local() {} } fn first() {} fn second() {}";

#[test]
fn local_impl_identity_is_independent_of_unrelated_declaration_insertions()
-> Result<(), ExtractorError> {
    let original = RustExtractor.extract(&tests::source(
        "fn retained() { struct Local; impl Local {} }",
    ))?;
    let extended = RustExtractor.extract(&tests::source(
        "fn added() { struct Local; impl Local {} } fn retained() { struct Local; impl Local {} }",
    ))?;
    let owner = extended
        .nodes
        .iter()
        .find(|node| node.name == "retained")
        .ok_or_else(|| ExtractorError::InvalidInput("missing retained owner".to_owned()))?;
    let ids = implementation_ids(&original);
    if ids.len() != 1
        || !extended.edges.iter().any(|edge| {
            edge.source == owner.id
                && ids.contains(&edge.target)
                && edge.relation == RelationKind::Contains
        })
    {
        return Err(ExtractorError::InvalidInput(
            "unrelated local impl changed retained identity".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn impl_containment_uses_emitted_scope_owners_and_preserves_unknown_contexts()
-> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&tests::source("mod api { struct Local; impl Local {} } fn outer() { struct Local; impl Local {} } struct Service; impl Service { fn run() { struct Local; impl Local {} } } trait Contract { fn run() { struct Local; impl Local {} } } const UNKNOWN: () = { struct Local; impl Local {} };"))?;
    let ids = implementation_ids(&extraction);
    for name in ["api", "outer", "Service::run", "Contract::run"] {
        let owner = extraction
            .nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| ExtractorError::InvalidInput("missing impl owner".to_owned()))?;
        if extraction
            .edges
            .iter()
            .filter(|edge| {
                edge.source == owner.id
                    && ids.contains(&edge.target)
                    && edge.relation == RelationKind::Contains
            })
            .count()
            != 1
        {
            return Err(ExtractorError::InvalidInput(
                "incorrect scoped impl containment".to_owned(),
            ));
        }
    }
    let unknown = ids
        .last()
        .ok_or_else(|| ExtractorError::InvalidInput("missing unknown-context impl".to_owned()))?;
    if extraction
        .edges
        .iter()
        .any(|edge| edge.target == *unknown && edge.relation == RelationKind::Contains)
    {
        return Err(ExtractorError::InvalidInput(
            "invented unknown-context containment".to_owned(),
        ));
    }
    Ok(())
}

fn implementation_ids(extraction: &Extraction) -> Vec<NodeId> {
    extraction.nodes.iter().filter(|node| matches!(&node.kind, NodeKind::External { namespace, kind } if namespace == RUST_PRODUCER_NAMESPACE && matches!(kind.as_str(), "implementation" | "negative-implementation")))
        .map(|node| node.id).collect()
}

#[test]
fn impls_inside_unmodeled_associated_types_do_not_inherit_trait_or_impl_owners()
-> Result<(), ExtractorError> {
    for content in [
        "struct Service; impl Service { type Alias = [(); { struct Local; impl Local {} 0 }]; }",
        "trait Contract { type Alias = [(); { struct Local; impl Local {} 0 }]; }",
    ] {
        let extraction = RustExtractor.extract(&tests::source(content))?;
        let ids = implementation_ids(&extraction);
        let local = ids
            .last()
            .ok_or_else(|| ExtractorError::InvalidInput("missing alias-local impl".to_owned()))?;
        if extraction
            .edges
            .iter()
            .any(|edge| edge.target == *local && edge.relation == RelationKind::Contains)
        {
            return Err(ExtractorError::InvalidInput(
                "associated type invented parent ownership".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn impl_blocks_keep_polarity_and_same_named_method_ownership() -> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&tests::source(CONTENT))?;
    let ids = implementation_ids(&extraction);
    let implementations = extraction
        .references
        .iter()
        .filter(|reference| reference.relation == RelationKind::Implements)
        .collect::<Vec<_>>();
    if ids.len() != 4
        || extraction.nodes.iter().filter(|node| matches!(&node.kind, NodeKind::External { namespace, kind } if namespace == RUST_PRODUCER_NAMESPACE && kind == "negative-implementation")).count() != 1
        || implementations
            .iter()
            .map(|reference| reference.target.as_str())
            .collect::<Vec<_>>()
            != ["A", "B"]
        || implementations
            .iter()
            .any(|reference| !ids.contains(&reference.source))
        || extraction
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Contains)
            .count()
            != 7
    {
        return Err(ExtractorError::InvalidInput(
            "incorrect impl polarity or containment".to_owned(),
        ));
    }
    let calls = extraction
        .references
        .iter()
        .filter(|reference| reference.relation == RelationKind::Calls)
        .collect::<Vec<_>>();
    if calls.len() != 2
        || calls.first().map(|call| call.source) == calls.get(1).map(|call| call.source)
    {
        return Err(ExtractorError::InvalidInput(
            "impl methods merged call ownership".to_owned(),
        ));
    }
    for (call, implementation) in calls.iter().zip(implementations) {
        if !extraction.edges.iter().any(|edge| {
            edge.source == implementation.source
                && edge.target == call.source
                && edge.relation == RelationKind::Contains
        }) {
            return Err(ExtractorError::InvalidInput(
                "method belongs to wrong impl block".to_owned(),
            ));
        }
        let span = implementation.source_location.span;
        if CONTENT.get(span.start_byte as usize..span.end_byte as usize)
            != Some(implementation.target.as_str())
        {
            return Err(ExtractorError::InvalidInput(
                "incorrect impl trait evidence".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn impl_trait_paths_reuse_segment_normalization_and_preserve_generic_evidence()
-> Result<(), ExtractorError> {
    let content = "struct Service<T>(T); impl<T> api::Contract<T> for Service<T> {}";
    let extraction = RustExtractor.extract(&tests::source(content))?;
    let implementation = extraction
        .references
        .first()
        .ok_or_else(|| ExtractorError::InvalidInput("missing generic implementation".to_owned()))?;
    let span = implementation.source_location.span;
    if implementation.target != "api::Contract"
        || content.get(span.start_byte as usize..span.end_byte as usize) != Some("api::Contract<T>")
    {
        return Err(ExtractorError::InvalidInput(
            "trait path normalization lost lookup or evidence".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn impl_identity_ignores_offsets_and_body_edits_and_distinguishes_duplicates()
-> Result<(), ExtractorError> {
    let initial = RustExtractor.extract(&tests::source(CONTENT))?;
    let edited = RustExtractor.extract(&tests::source(&format!(
        "// shifted\n{}",
        CONTENT.replace("first();", "second();")
    )))?;
    if implementation_ids(&initial) != implementation_ids(&edited) {
        return Err(ExtractorError::InvalidInput(
            "impl identity depends on body or offsets".to_owned(),
        ));
    }
    let duplicate = RustExtractor.extract(&tests::source(
        "struct Service; impl Service {} impl Service {}",
    ))?;
    let ids = implementation_ids(&duplicate);
    if ids.len() != 2 || ids.first() == ids.get(1) {
        return Err(ExtractorError::InvalidInput(
            "duplicate impls shared identity".to_owned(),
        ));
    }
    Ok(())
}
