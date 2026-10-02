use super::super::*;
use std::fmt::Write;

#[test]
fn indexed_ordinals_match_the_previous_node_scan() -> Result<(), ExtractorError> {
    let input = tests::source(
        "fn repeated() { fn local() {} } fn repeated() { fn local() {} } struct repeated; trait Contract { fn run(); fn run(); } mod duplicate { fn same() {} } mod duplicate { fn same() {} }",
    );
    let extraction = RustExtractor.extract(&input)?;
    let mut prior: Vec<&Node> = Vec::new();
    for node in &extraction.nodes {
        if !prior.is_empty() {
            let ordinal = prior
                .iter()
                .filter(|old| old.kind == node.kind && old.name == node.name)
                .count();
            let identity = if ordinal == 0 {
                node.name.clone()
            } else {
                format!("{}#{ordinal}", node.name)
            };
            if node.id != declaration_node_id(&input.file, &node.kind, &identity) {
                return Err(ExtractorError::InvalidInput(
                    "declaration identity changed".to_owned(),
                ));
            }
        }
        prior.push(node);
    }
    Ok(())
}

#[test]
fn ordinal_map_is_shared_across_recursive_declarations() -> Result<(), ExtractorError> {
    let mut text = String::new();
    for ordinal in 0..1_000 {
        writeln!(text, "fn unique_{ordinal}() {{ fn local() {{}} }}")
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    }
    let extraction = RustExtractor.extract(&tests::source(&text))?;
    if extraction.nodes.len() != 2_001 || extraction.edges.len() != 1_000 {
        return Err(ExtractorError::InvalidInput(
            "recursive declarations lost".to_owned(),
        ));
    }
    Ok(())
}
