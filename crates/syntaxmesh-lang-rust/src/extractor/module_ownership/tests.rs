use super::*;

#[test]
fn imports_bind_actual_local_module_declarations() -> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&tests::source(
        "mod local { use crate::root; } fn outer() { mod local { use crate::function; } } struct Service; impl Service { fn run() { mod local { use crate::method; } } } trait Contract { fn run() { mod local { use crate::default_body; } } }",
    ))?;
    if extraction.imports.len() != 4 {
        return Err(ExtractorError::InvalidInput(
            "lost local module imports".to_owned(),
        ));
    }
    for (specifier, owner) in [
        ("crate::root", "local"),
        ("crate::function", "outer::local"),
        ("crate::method", "Service::run::local"),
        ("crate::default_body", "Contract::run::local"),
    ] {
        if !extraction.imports.iter().any(|import| {
            import.specifier == specifier
                && extraction.nodes.iter().any(|node| {
                    node.id == import.module && node.name == owner && node.kind == NodeKind::Module
                })
        }) {
            return Err(ExtractorError::InvalidInput(
                "wrong source-backed import owner".to_owned(),
            ));
        }
    }
    Ok(())
}
