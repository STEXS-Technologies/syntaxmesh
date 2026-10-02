use super::super::*;

#[test]
fn multiline_definitions_preserve_source_and_owner_identity() -> Result<(), ExtractorError> {
    let content = "// λ prefix\nfn outer() {\n  fn inner() {\n    helper();\n  }\n  inner();\n}\nfn helper() {}\nstruct Service;\nimpl Service {\n  fn run(&self) {\n    helper();\n  }\n}\ntrait Contract {\n  fn required(&self);\n  fn default(&self) {\n    helper();\n  }\n}\n";
    let original = RustExtractor.extract(&tests::source(content))?;
    let shifted = RustExtractor.extract(&tests::source(&format!("\n{content}")))?;
    for (name, expected) in [
        (
            "outer",
            "fn outer() {\n  fn inner() {\n    helper();\n  }\n  inner();\n}",
        ),
        ("outer::inner", "fn inner() {\n    helper();\n  }"),
        ("Service::run", "fn run(&self) {\n    helper();\n  }"),
        ("Contract::required", "fn required(&self);"),
        (
            "Contract::default",
            "fn default(&self) {\n    helper();\n  }",
        ),
    ] {
        let node = original
            .nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| ExtractorError::InvalidInput(format!("missing function {name}")))?;
        let span = &node
            .source
            .as_ref()
            .ok_or_else(|| ExtractorError::InvalidInput("missing definition evidence".to_owned()))?
            .span;
        let start = usize::try_from(span.start_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = usize::try_from(span.end_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        if content.get(start..end) != Some(expected)
            || shifted
                .nodes
                .iter()
                .find(|candidate| candidate.name == name)
                .is_none_or(|candidate| candidate.id != node.id)
        {
            return Err(ExtractorError::InvalidInput(format!(
                "incorrect range or unstable identity: {name}"
            )));
        }
        if name != "Contract::required"
            && !original
                .references
                .iter()
                .any(|reference| reference.source == node.id)
        {
            return Err(ExtractorError::InvalidInput(format!(
                "lost call ownership: {name}"
            )));
        }
    }
    Ok(())
}
