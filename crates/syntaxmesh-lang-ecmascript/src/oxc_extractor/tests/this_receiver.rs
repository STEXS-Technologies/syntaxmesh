use super::{JavaScriptExtractor, TypeScriptExtractor, source};
use syntaxmesh_core::{NodeKind, RelationKind};
use syntaxmesh_language_sdk::{ExtractorError, LanguageExtractor};

#[test]
fn computed_class_keys_do_not_assume_the_declared_class_receiver() -> Result<(), ExtractorError> {
    let content =
        "class Outer { key() {} run() { class Inner { [this.key()]() {} } this.key(); } }";
    for path in ["src/keys.ts", "src/keys.js"] {
        let input = source(path, content);
        let extraction = if path.ends_with(".ts") {
            TypeScriptExtractor.extract(&input)?
        } else {
            JavaScriptExtractor.extract(&input)?
        };
        let targets = extraction
            .references
            .iter()
            .map(|reference| reference.target.as_str())
            .collect::<Vec<_>>();
        if targets != ["this.key", "Outer::key"] {
            return Err(ExtractorError::InvalidInput(
                "computed class key assumed the new class receiver".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn this_scope_preserves_arrows_but_not_dynamic_receivers() -> Result<(), ExtractorError> {
    let content = r#"class Service {
    build() {}
    run() {
        this.build();
        const lexical = () => this.build();
        function detached() { this.build(); }
        const callback = function() { this.build(); };
        const object = { run() { this.build(); } };
        function nested() { const arrow = () => this.build(); }
        const anonymous = class { run() { this.build(); } };
        class Inner { build() {} run() { this.build(); } }
        this.build();
    }
    field = () => this.build();
}"#;
    for path in ["src/receiver.ts", "src/receiver.js"] {
        let input = source(path, content);
        let extraction = if path.ends_with(".ts") {
            TypeScriptExtractor.extract(&input)?
        } else {
            JavaScriptExtractor.extract(&input)?
        };
        let count = |target: &str| {
            extraction
                .references
                .iter()
                .filter(|reference| reference.target == target)
                .count()
        };
        if extraction.references.len() != 10
            || count("Service::build") != 3
            || count("Service::run::Inner::build") != 1
            || count("this.build") != 6
        {
            return Err(ExtractorError::InvalidInput(
                "this receiver context leaked or lost lexical arrows".to_owned(),
            ));
        }
        for reference in &extraction.references {
            let start = usize::try_from(reference.source_location.span.start_byte)
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
            let end = usize::try_from(reference.source_location.span.end_byte)
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
            if content.get(start..end) != Some("build") {
                return Err(ExtractorError::InvalidInput(
                    "receiver call evidence is not exact".to_owned(),
                ));
            }
        }
        let resolved =
            syntaxmesh_resolver::resolve_references(&extraction.nodes, &extraction.references);
        // The repeated run-to-build calls share one direct edge, but retain
        // separate resolved occurrence nodes.
        if resolved
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Calls)
            .count()
            != 3
            || resolved
                .reference_nodes
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind,
                        NodeKind::Reference {
                            relation: RelationKind::Calls
                        }
                    )
                })
                .count()
                != 4
            || resolved
                .reference_nodes
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind,
                        NodeKind::UnresolvedReference {
                            relation: RelationKind::Calls
                        }
                    )
                })
                .count()
                != 6
        {
            return Err(ExtractorError::InvalidInput(
                "dynamic this receiver produced a concrete call edge".to_owned(),
            ));
        }
    }
    Ok(())
}
