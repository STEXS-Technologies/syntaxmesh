use super::*;

#[test]
fn trait_methods_have_definition_evidence_and_own_default_body_calls() -> Result<(), ExtractorError>
{
    let source = tests::source(
        "mod api { trait Service { fn required(&self); fn run(&self) { utility(); self.required(); } } } fn utility() {}",
    );
    let extraction = RustExtractor.extract(&source)?;
    for (name, token, marker) in [
        (
            "api::Service::required",
            "fn required(&self);",
            "fn required",
        ),
        (
            "api::Service::run",
            "fn run(&self) { utility(); self.required(); }",
            "fn run",
        ),
    ] {
        let node = extraction
            .nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| ExtractorError::InvalidInput(format!("missing trait method {name}")))?;
        let location = node
            .source
            .as_ref()
            .ok_or_else(|| ExtractorError::InvalidInput("missing method evidence".to_owned()))?;
        let start = source
            .content
            .find(marker)
            .ok_or_else(|| ExtractorError::InvalidInput("missing fixture marker".to_owned()))?;
        if node.kind != NodeKind::Function
            || location.span.start_byte != start as u64
            || location.span.end_byte != (start + token.len()) as u64
        {
            return Err(ExtractorError::InvalidInput(format!(
                "incorrect method evidence: {name}"
            )));
        }
    }
    let owner = extraction
        .nodes
        .iter()
        .find(|node| node.name == "api::Service::run")
        .ok_or_else(|| ExtractorError::InvalidInput("missing default owner".to_owned()))?;
    if extraction.references.len() != 2
        || extraction
            .references
            .iter()
            .any(|reference| reference.source != owner.id)
        || extraction
            .references
            .iter()
            .map(|reference| reference.target.as_str())
            .collect::<Vec<_>>()
            != ["utility", "self.required"]
    {
        return Err(ExtractorError::InvalidInput(
            "trait body calls lost ownership or receiver spelling".to_owned(),
        ));
    }
    Ok(())
}
