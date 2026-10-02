use super::*;

#[test]
fn preserves_unknown_method_receivers_and_full_expression_evidence() -> Result<(), ExtractorError> {
    let source = tests::source(
        "struct Service; impl Service { fn run(&self) { self.build(); (self).build(); (&self).build(); client.build(1); } fn build(&self) {} } fn caller() { client.build(); self.build(); }",
    );
    let extraction = RustExtractor.extract(&source)?;
    let expected = [
        ("Service::build", "self.build()"),
        ("Service::build", "(self).build()"),
        ("Service::build", "(&self).build()"),
        ("client.build", "client.build(1)"),
        ("client.build", "client.build()"),
        ("self.build", "self.build()"),
    ];
    if extraction.references.len() != expected.len() {
        return Err(ExtractorError::InvalidInput(
            "missing receiver calls".to_owned(),
        ));
    }
    for (reference, (target, evidence)) in extraction.references.iter().zip(expected) {
        let span = reference.source_location.span;
        let start = usize::try_from(span.start_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = usize::try_from(span.end_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        if reference.target != target || source.content.get(start..end) != Some(evidence) {
            return Err(ExtractorError::InvalidInput(format!(
                "receiver qualification or evidence differs: {}",
                reference.target
            )));
        }
    }
    Ok(())
}

#[test]
fn retains_absolute_and_type_qualified_call_targets_and_complete_evidence()
-> Result<(), ExtractorError> {
    let source = tests::source(
        "fn caller() { helper(); Service::build(); <Service as Contract>::helper(); <Service>::helper(); ::helper(); }",
    );
    let extraction = RustExtractor.extract(&source)?;
    let expected = [
        ("helper", "helper"),
        ("Service::build", "Service::build"),
        (
            "< Service as Contract > :: helper",
            "<Service as Contract>::helper",
        ),
        ("< Service > :: helper", "<Service>::helper"),
        ("::helper", "::helper"),
    ];
    if extraction.references.len() != expected.len() {
        return Err(ExtractorError::InvalidInput(
            "missing call paths".to_owned(),
        ));
    }
    for (reference, (target, evidence)) in extraction.references.iter().zip(expected) {
        let span = reference.source_location.span;
        let start = usize::try_from(span.start_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = usize::try_from(span.end_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        if reference.target != target || source.content.get(start..end) != Some(evidence) {
            return Err(ExtractorError::InvalidInput(format!(
                "call qualification or evidence differs: {}",
                reference.target
            )));
        }
    }
    Ok(())
}
