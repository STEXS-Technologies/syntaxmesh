use super::super::*;

#[test]
fn qualified_framework_attributes_record_syntax_intent_without_identity_changes()
-> Result<(), ExtractorError> {
    let plain = RustExtractor.extract(&tests::source("async fn check() {}"))?;
    let plain_id = plain
        .nodes
        .iter()
        .find(|node| node.name == "check")
        .ok_or_else(|| ExtractorError::InvalidInput("missing plain callable".to_owned()))?
        .id;
    for attribute in [
        "#[tokio::test]",
        "#[tokio::test()]",
        "#[tokio::test(flavor = \"multi_thread\", worker_threads = 2)]",
        "#[async_std::test]",
        "#[async_std::test()]",
    ] {
        let extraction = RustExtractor.extract(&tests::source(&format!(
            "{attribute} async fn check() {{}}"
        )))?;
        let node = extraction
            .nodes
            .iter()
            .find(|node| node.name == "check")
            .ok_or_else(|| ExtractorError::InvalidInput("missing qualified callable".to_owned()))?;
        if node.id != plain_id
            || node.kind != NodeKind::Function
            || syntaxmesh_language_sdk::SourceRole::from_payload(node.extension_payload.as_ref())
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?
                != syntaxmesh_language_sdk::SourceRole::TestIntent
        {
            return Err(ExtractorError::InvalidInput(
                "qualified test intent lost identity or role".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn nested_and_duplicate_test_roles_preserve_declaration_ordering() -> Result<(), ExtractorError> {
    let plain = "mod same { fn check() {} fn helper() {} } mod same { fn check() {} } fn outer() { fn local() {} }";
    let annotated = "mod same { #[test] fn check() {} fn helper() {} } mod same { #[test] fn check() {} } fn outer() { #[test] fn local() {} }";
    let before = RustExtractor.extract(&tests::source(plain))?;
    let after = RustExtractor.extract(&tests::source(annotated))?;
    let functions = |extraction: &Extraction| {
        extraction
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Function)
            .map(|node| (node.id, node.name.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    if functions(&before) != functions(&after) {
        return Err(ExtractorError::InvalidInput(
            "nested test roles changed declaration identities".to_owned(),
        ));
    }
    let roles = after
        .nodes
        .iter()
        .filter(|node| node.extension_payload.is_some())
        .collect::<Vec<_>>();
    if roles.len() != 3
        || roles
            .iter()
            .any(|node| !matches!(node.name.as_str(), "same::check" | "outer::local"))
    {
        return Err(ExtractorError::InvalidInput(
            "nested/duplicate role attachment is incorrect".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn direct_test_intent_preserves_callable_identity_and_body_references() -> Result<(), ExtractorError>
{
    let plain = RustExtractor.extract(&tests::source("fn check() { helper(); } fn helper() {}"))?;
    let annotated = RustExtractor.extract(&tests::source(
        "#[test] fn check() { helper(); } fn helper() {}",
    ))?;
    let lookup = |extraction: &Extraction, name: &str| {
        extraction
            .nodes
            .iter()
            .find(|node| node.name == name)
            .cloned()
            .ok_or_else(|| ExtractorError::InvalidInput("missing fixture function".to_owned()))
    };
    let before = lookup(&plain, "check")?;
    let after = lookup(&annotated, "check")?;
    if before.id != after.id
        || after.kind != NodeKind::Function
        || after.extension_payload.as_ref().is_none_or(|payload| {
            payload.namespace != syntaxmesh_language_sdk::SOURCE_ROLE_NAMESPACE
                || payload.schema_version != 1
                || payload.bytes != [1]
        })
    {
        return Err(ExtractorError::InvalidInput(
            "test annotation changed callable identity or lost role".to_owned(),
        ));
    }
    if annotated.references.len() != plain.references.len()
        || !annotated
            .references
            .iter()
            .any(|reference| reference.source == after.id)
        || lookup(&annotated, "helper")?.extension_payload.is_some()
    {
        return Err(ExtractorError::InvalidInput(
            "test role changed references or annotated helper".to_owned(),
        ));
    }
    for attribute in [
        "#[cfg_attr(test, test)]",
        "#[custom::test]",
        "#[test_helper]",
        "#[test()]",
        "#[tokio::test = \"yes\"]",
        "#[tokio::other::test]",
        "#[::tokio::test]",
        "#[tokio::test{}]",
        "#[tokio::test[]]",
        "#[cfg_attr(test, tokio::test)]",
    ] {
        let extraction =
            RustExtractor.extract(&tests::source(&format!("{attribute} fn check() {{}}")))?;
        if lookup(&extraction, "check")?.extension_payload.is_some() {
            return Err(ExtractorError::InvalidInput(
                "conditional/framework/lookalike attribute became standard test role".to_owned(),
            ));
        }
    }
    Ok(())
}
