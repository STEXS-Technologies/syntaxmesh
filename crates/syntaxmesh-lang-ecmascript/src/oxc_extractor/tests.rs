mod namespace_literals;
mod this_receiver;

use super::*;
use std::collections::BTreeSet;
use syntaxmesh_core::{FileId, FileVersion};

fn source(path: &str, content: &str) -> SourceFile {
    SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
        },
        content: content.to_owned(),
    }
}

#[test]
fn extracts_declarations_calls_and_exact_evidence() -> Result<(), ExtractorError> {
    let content = "class Service { run() { this.build(); client.send(); } build() {} } function start() { new Service(); }";
    let extraction = TypeScriptExtractor.extract(&source("src/app.ts", content))?;
    let class = extraction
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Class && node.name == "Service")
        .ok_or_else(|| ExtractorError::InvalidInput("class missing".to_owned()))?;
    let method = extraction
        .nodes
        .iter()
        .find(|node| node.name == "Service::build")
        .ok_or_else(|| ExtractorError::InvalidInput("method missing".to_owned()))?;
    if !extraction
        .references
        .iter()
        .any(|reference| reference.target == "Service::build")
        || !extraction
            .references
            .iter()
            .any(|reference| reference.target == "client.send")
        || !extraction
            .references
            .iter()
            .any(|reference| reference.target == "Service")
        || class.source.is_none()
        || method.source.is_none()
    {
        return Err(ExtractorError::InvalidInput(
            "declaration or call facts were incomplete".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn named_arrow_owns_its_calls_and_covers_its_definition() -> Result<(), ExtractorError> {
    let extraction = JavaScriptExtractor.extract(&source(
        "src/app.js",
        "export const load = (id) => client.fetch(id);",
    ))?;
    let function = extraction
        .nodes
        .iter()
        .find(|node| node.name == "load")
        .ok_or_else(|| ExtractorError::InvalidInput("arrow function missing".to_owned()))?;
    if !extraction
        .references
        .iter()
        .any(|reference| reference.source == function.id && reference.target == "client.fetch")
    {
        return Err(ExtractorError::InvalidInput(
            "arrow call was not assigned to its function".to_owned(),
        ));
    }
    let location = function
        .source
        .as_ref()
        .ok_or_else(|| ExtractorError::InvalidInput("arrow span missing".to_owned()))?;
    if location
        .span
        .end_byte
        .saturating_sub(location.span.start_byte)
        != u64::try_from("load = (id) => client.fetch(id)".len()).unwrap_or(u64::MAX)
    {
        return Err(ExtractorError::InvalidInput(
            "arrow evidence did not cover the definition".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn function_evidence_covers_ast_definitions_without_adjacent_code() -> Result<(), ExtractorError> {
    let content = "// λ before definitions\nfunction start() {\n  client.send('λ');\n}\nclass Service {\n  run() {\n    this.build();\n  }\n  build() {}\n}\nconst load = (id) => {\n  return client.fetch(id);\n};\nfunction next() {}\n";
    let extraction = TypeScriptExtractor.extract(&source("src/ranges.ts", content))?;
    let shifted = TypeScriptExtractor.extract(&source("src/ranges.ts", &format!("\n{content}")))?;
    for (name, expected) in [
        ("start", "function start() {\n  client.send('λ');\n}"),
        ("Service::run", "run() {\n    this.build();\n  }"),
        ("Service::build", "build() {}"),
        ("load", "load = (id) => {\n  return client.fetch(id);\n}"),
    ] {
        let node = extraction
            .nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| ExtractorError::InvalidInput(format!("missing definition {name}")))?;
        let location = node
            .source
            .as_ref()
            .ok_or_else(|| ExtractorError::InvalidInput("missing definition range".to_owned()))?;
        let start = usize::try_from(location.span.start_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = usize::try_from(location.span.end_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        if content.get(start..end) != Some(expected)
            || shifted
                .nodes
                .iter()
                .find(|candidate| candidate.name == name)
                .is_none_or(|candidate| candidate.id != node.id)
        {
            return Err(ExtractorError::InvalidInput(format!(
                "incorrect definition evidence or unstable ID for {name}"
            )));
        }
    }
    Ok(())
}

#[test]
fn accepts_modern_typescript_and_raw_jsx_ampersands() -> Result<(), ExtractorError> {
    TypeScriptExtractor.extract(&source(
        "stores/panel/console/store.test.ts",
        "const actual = await importOriginal<typeof import('@/lib/utils')>()",
    ))?;
    let tsx_source = source(
        "view.tsx",
        "function View() { return <><img alt='A & B' /><p>Tracking & Cookies</p></>; afterView(); }",
    );
    let extraction = TypeScriptExtractor.extract(&tsx_source)?;
    let reference = extraction
        .references
        .iter()
        .find(|reference| reference.target == "afterView")
        .ok_or_else(|| ExtractorError::InvalidInput("later call missing".to_owned()))?;
    let start = usize::try_from(reference.source_location.span.start_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let end = usize::try_from(reference.source_location.span.end_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    if tsx_source.content.get(start..end) != Some("afterView") {
        return Err(ExtractorError::InvalidInput(
            "Oxc source span did not map to original input".to_owned(),
        ));
    }
    TypeScriptExtractor.extract(&source("view.TSX", "const View = () => <div />;"))?;
    Ok(())
}

#[test]
fn rejects_malformed_source_and_wrong_language_extension() {
    assert!(matches!(
        TypeScriptExtractor.extract(&source("bad.ts", "function {")),
        Err(ExtractorError::SyntaxError(_))
    ));
    assert!(matches!(
        JavaScriptExtractor.extract(&source("bad.js", "function {")),
        Err(ExtractorError::SyntaxError(_))
    ));
    assert!(
        JavaScriptExtractor
            .extract(&source("bad.ts", "function valid() {}"))
            .is_err()
    );
}

#[test]
fn identical_content_in_distinct_files_has_distinct_provenance() -> Result<(), ExtractorError> {
    let content = "function shared() {}";
    let first = TypeScriptExtractor.extract(&source("src/first.ts", content))?;
    let second = TypeScriptExtractor.extract(&source("src/second.ts", content))?;
    if first.provenance.id == second.provenance.id {
        return Err(ExtractorError::InvalidInput(
            "identical content in distinct files shared provenance identity".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn extracts_typed_imports_exports_and_reexports() -> Result<(), ExtractorError> {
    let input = source(
        "src/app.ts",
        "import value, { format as pretty, type Thing } from './value.js'; import * as ns from './ns.js'; import './side.js'; export { value as renamed, pretty }; export { other as publicOther } from '../shared'; export * from './all.js'; export * as namespace from './namespace.js'; export default value; export const local = 1; export const { alpha, beta: gamma } = thing; export function run() {} export class Widget {} export type Model = string; const later = import('./lazy.ts'); const cjs = require('./cjs');",
    );
    let extraction = TypeScriptExtractor.extract(&input)?;
    let module = extraction
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Module)
        .ok_or_else(|| ExtractorError::InvalidInput("file module node missing".to_owned()))?;
    if extraction.imports.len() != 7
        || extraction.exports.len() != 12
        || extraction
            .imports
            .iter()
            .any(|record| record.module != module.id || record.source.file_id != input.file.file_id)
        || extraction
            .exports
            .iter()
            .any(|record| record.module != module.id || record.source.file_id != input.file.file_id)
        || extraction
            .references
            .iter()
            .any(|reference| reference.relation == RelationKind::Imports)
        || !extraction.imports.iter().any(|record| {
            record.specifier == "./value.js"
                && record.kind == ImportKind::Default
                && record.imported_name.as_deref() == Some("default")
                && record.local_name.as_deref() == Some("value")
        })
        || !extraction.imports.iter().any(|record| {
            record.specifier == "./value.js"
                && record.kind == ImportKind::Named
                && record.imported_name.as_deref() == Some("format")
                && record.local_name.as_deref() == Some("pretty")
        })
        || !extraction.imports.iter().any(|record| {
            record.specifier == "./value.js"
                && record.imported_name.as_deref() == Some("Thing")
                && record.type_only
        })
        || !extraction
            .imports
            .iter()
            .any(|record| record.specifier == "./side.js" && record.kind == ImportKind::SideEffect)
        || !extraction
            .imports
            .iter()
            .any(|record| record.specifier == "./lazy.ts" && record.kind == ImportKind::Dynamic)
        || !extraction
            .imports
            .iter()
            .any(|record| record.specifier == "./cjs" && record.kind == ImportKind::CommonJs)
        || !extraction.exports.iter().any(|record| {
            record.source_specifier.as_deref() == Some("../shared")
                && record.kind == ExportKind::NamedReExport
                && record.exported_name.as_deref() == Some("publicOther")
                && record.local_name.as_deref() == Some("other")
        })
        || !extraction.exports.iter().any(|record| {
            record.source_specifier.as_deref() == Some("./all.js")
                && record.kind == ExportKind::StarReExport
        })
        || !extraction.exports.iter().any(|record| {
            record.source_specifier.as_deref() == Some("./namespace.js")
                && record.kind == ExportKind::NamespaceReExport
                && record.exported_name.as_deref() == Some("namespace")
        })
        || !extraction.exports.iter().any(|record| {
            record.kind == ExportKind::Local
                && record.exported_name.as_deref() == Some("local")
                && record.local_name.as_deref() == Some("local")
        })
        || !extraction.exports.iter().any(|record| {
            record.kind == ExportKind::Local
                && record.exported_name.as_deref() == Some("Model")
                && record.type_only
        })
        || !extraction.exports.iter().any(|record| {
            record.kind == ExportKind::Local
                && record.exported_name.as_deref() == Some("alpha")
                && record.local_name.as_deref() == Some("alpha")
        })
        || !extraction.exports.iter().any(|record| {
            record.kind == ExportKind::Local
                && record.exported_name.as_deref() == Some("gamma")
                && record.local_name.as_deref() == Some("gamma")
        })
    {
        return Err(ExtractorError::InvalidInput(
            "typed module import/export occurrences were not retained precisely".to_owned(),
        ));
    }
    let occurrence = extraction
        .imports
        .iter()
        .find(|record| record.specifier == "./value.js")
        .ok_or_else(|| ExtractorError::InvalidInput("static import missing".to_owned()))?;
    let start = usize::try_from(occurrence.source.span.start_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let end = usize::try_from(occurrence.source.span.end_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    if input.content.get(start..end) != Some("value") {
        return Err(ExtractorError::InvalidInput(
            "import binding did not preserve its exact source span".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn extracts_static_namespace_member_uses_and_skips_shadowed_or_dynamic_access()
-> Result<(), ExtractorError> {
    let input = source(
        "src/app.ts",
        "import * as schema from './schema'; const value = schema.user; const optional = schema?.optionalMember; const computed = schema[key];",
    );
    let extraction = TypeScriptExtractor.extract(&input)?;
    let members = extraction
        .imports
        .iter()
        .filter(|record| record.kind == ImportKind::NamespaceMember)
        .collect::<Vec<_>>();
    if members.len() != 2
        || !members.iter().any(|record| {
            record.specifier == "./schema"
                && record.imported_name.as_deref() == Some("user")
                && record.local_name.as_deref() == Some("schema.user")
        })
        || !members.iter().any(|record| {
            record.imported_name.as_deref() == Some("optionalMember")
                && record.local_name.as_deref() == Some("schema.optionalMember")
        })
    {
        return Err(ExtractorError::InvalidInput(
            "static namespace member import facts were not extracted precisely".to_owned(),
        ));
    }
    let user = members
        .iter()
        .find(|record| record.imported_name.as_deref() == Some("user"))
        .ok_or_else(|| ExtractorError::InvalidInput("user member missing".to_owned()))?;
    let start = usize::try_from(user.source.span.start_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let end = usize::try_from(user.source.span.end_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    if input.content.get(start..end) != Some("user") {
        return Err(ExtractorError::InvalidInput(
            "namespace member span did not select the property token".to_owned(),
        ));
    }

    let shadowed = source(
        "src/shadowed.ts",
        "import * as schema from './schema'; function load(schema: unknown) { return schema.secret; }",
    );
    let shadowed = TypeScriptExtractor.extract(&shadowed)?;
    if shadowed
        .imports
        .iter()
        .any(|record| record.kind == ImportKind::NamespaceMember)
    {
        return Err(ExtractorError::InvalidInput(
            "a shadowed namespace local was recorded as an imported member".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn import_and_export_ids_survive_line_movement_and_distinguish_duplicates()
-> Result<(), ExtractorError> {
    let first = TypeScriptExtractor.extract(&source(
        "src/identity.ts",
        "import { item as local } from './lib'; import { item as local } from './lib'; export { local as publicName }; export { local as publicName };",
    ))?;
    let moved = TypeScriptExtractor.extract(&source(
        "src/identity.ts",
        "\nimport { item as local } from './lib'; import { item as local } from './lib'; export { local as publicName }; export { local as publicName };",
    ))?;
    let mut first_import_ids = first
        .imports
        .iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    let mut moved_import_ids = moved
        .imports
        .iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    let mut first_export_ids = first
        .exports
        .iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    let mut moved_export_ids = moved
        .exports
        .iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    first_import_ids.sort_unstable();
    moved_import_ids.sort_unstable();
    first_export_ids.sort_unstable();
    moved_export_ids.sort_unstable();
    let import_ids_unique =
        first_import_ids.iter().collect::<BTreeSet<_>>().len() == first_import_ids.len();
    let export_ids_unique =
        first_export_ids.iter().collect::<BTreeSet<_>>().len() == first_export_ids.len();
    if first_import_ids.len() != 2
        || first_export_ids.len() != 2
        || !import_ids_unique
        || !export_ids_unique
        || first_import_ids != moved_import_ids
        || first_export_ids != moved_export_ids
    {
        return Err(ExtractorError::InvalidInput(
            "typed source-fact IDs changed with line movement or collided for duplicates"
                .to_owned(),
        ));
    }
    Ok(())
}
