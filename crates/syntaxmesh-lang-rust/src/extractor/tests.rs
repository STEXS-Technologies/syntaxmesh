use super::*;
use std::collections::BTreeSet;
use syntaxmesh_language_sdk::LanguageExtractor;

#[test]
fn declaration_evidence_uses_ast_tokens_not_comments_or_calls() -> Result<(), ExtractorError> {
    let content = "// nested inner Thing Choice Contract Service build\nmod nested { fn inner() {} }\nstruct Thing; enum Choice { A } trait Contract {}\nstruct Service; impl Service { fn run(&self) { self.build(); } fn build(&self) {} }\n";
    let extraction = RustExtractor.extract(&source(content))?;
    for (name, token, marker) in [
        ("nested", "nested", "mod nested"),
        ("nested::inner", "fn inner() {}", "fn inner() {}"),
        ("Thing", "Thing", "struct Thing"),
        ("Choice", "Choice", "enum Choice"),
        ("Contract", "Contract", "trait Contract"),
        ("Service", "Service", "struct Service"),
        (
            "Service::run",
            "fn run(&self) { self.build(); }",
            "fn run(&self) { self.build(); }",
        ),
        ("Service::build", "fn build(&self) {}", "fn build(&self) {}"),
    ] {
        let node = extraction
            .nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| ExtractorError::InvalidInput(format!("missing declaration {name}")))?;
        let location = node
            .source
            .as_ref()
            .ok_or_else(|| ExtractorError::InvalidInput("missing evidence".to_owned()))?;
        let marker_start = content
            .find(marker)
            .ok_or_else(|| ExtractorError::InvalidInput("missing fixture marker".to_owned()))?;
        let expected = marker_start + marker.len() - token.len();
        if location.span.start_byte != expected as u64
            || location.span.end_byte != (expected + token.len()) as u64
        {
            return Err(ExtractorError::InvalidInput(format!(
                "incorrect declaration span for {name}"
            )));
        }
    }
    Ok(())
}

#[test]
fn unicode_import_and_call_evidence_has_exact_byte_offsets() -> Result<(), ExtractorError> {
    let content = "/* résumé λ */ use crate::modèle::Élément;\nfn entrée() { let _ = \"résumé\"; aide(); }\nfn aide() {}\n";
    let extraction = RustExtractor.extract(&source(content))?;
    let import = extraction
        .imports
        .first()
        .ok_or_else(|| ExtractorError::InvalidInput("missing import".to_owned()))?;
    let call = extraction
        .references
        .first()
        .ok_or_else(|| ExtractorError::InvalidInput("missing call".to_owned()))?;
    for (span, expected) in [
        (&import.source.span, "Élément"),
        (&call.source_location.span, "aide"),
    ] {
        let start = usize::try_from(span.start_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = usize::try_from(span.end_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        if content.get(start..end) != Some(expected) {
            return Err(ExtractorError::InvalidInput(
                "incorrect Unicode evidence span".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn parser_prefixes_and_raw_identifiers_retain_original_byte_ranges() -> Result<(), ExtractorError> {
    for prefix in [
        "",
        "\u{feff}",
        "#!/usr/bin/env rustx\n",
        "\u{feff}#!/usr/bin/env rustx\r\n",
    ] {
        let content = format!(
            "{prefix}/* λ */ use crate::r#type;\r\nfn r#match() {{ r#type(); }}\r\nfn r#type() {{}}\r\n"
        );
        let extraction = RustExtractor.extract(&source(&content))?;
        for node in extraction
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Function)
        {
            let span = &node
                .source
                .as_ref()
                .ok_or_else(|| ExtractorError::InvalidInput("missing declaration span".to_owned()))?
                .span;
            let expected = if node.name == "r#match" {
                "fn r#match() { r#type(); }"
            } else {
                "fn r#type() {}"
            };
            if content.get(span.start_byte as usize..span.end_byte as usize) != Some(expected) {
                return Err(ExtractorError::InvalidInput(
                    "incorrect raw declaration span".to_owned(),
                ));
            }
        }
        let import = extraction
            .imports
            .first()
            .ok_or_else(|| ExtractorError::InvalidInput("missing raw import".to_owned()))?;
        let call = extraction
            .references
            .first()
            .ok_or_else(|| ExtractorError::InvalidInput("missing raw call".to_owned()))?;
        for span in [&import.source.span, &call.source_location.span] {
            if content.get(span.start_byte as usize..span.end_byte as usize) != Some("r#type") {
                return Err(ExtractorError::InvalidInput(
                    "incorrect prefixed source span".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn source(content: &str) -> SourceFile {
    SourceFile {
        file: FileVersion {
            file_id: syntaxmesh_core::FileId::derive(&[b"src/main.rs"]),
            normalized_path: "src/main.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: content.len() as u64,
        },
        content: content.to_owned(),
    }
}

#[test]
fn extracts_nested_ast_declarations_with_stable_spans() -> Result<(), ExtractorError> {
    let content = "mod nested { fn inner() {} }\nstruct Thing;\n";
    let first = RustExtractor.extract(&source(content))?;
    let second = RustExtractor.extract(&source(content))?;
    if first != second || first.nodes.len() != 4 {
        return Err(ExtractorError::InvalidInput(
            "AST extraction is not deterministic".to_owned(),
        ));
    }
    if first.nodes.iter().any(|node| node.source.is_none()) {
        return Err(ExtractorError::InvalidInput(
            "missing declaration source span".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn extracts_named_grouped_glob_and_local_rust_use_facts() -> Result<(), ExtractorError> {
    let content = concat!(
        "use std::fmt::Debug;\n",
        "use std::io::{self, Read as Reader, Write};\n",
        "use crate::{models::{Item, self as models}, prelude::*};\n",
        "use ::core::fmt as CoreFmt;\n",
        "mod nested { use super::Outer as Alias; }\n",
        "fn local() { use super::inner::Thing as _; }\n",
    );
    let extraction = RustExtractor.extract(&source(content))?;
    let imported = extraction
        .imports
        .iter()
        .map(|item| {
            (
                item.kind,
                item.specifier.as_str(),
                item.imported_name.as_deref(),
                item.local_name.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    let expected = [
        (
            ImportKind::RustUse,
            "std::fmt::Debug",
            Some("Debug"),
            Some("Debug"),
        ),
        (ImportKind::RustUse, "std::io", Some("io"), Some("io")),
        (
            ImportKind::RustUse,
            "std::io::Read",
            Some("Read"),
            Some("Reader"),
        ),
        (
            ImportKind::RustUse,
            "std::io::Write",
            Some("Write"),
            Some("Write"),
        ),
        (
            ImportKind::RustUse,
            "crate::models::Item",
            Some("Item"),
            Some("Item"),
        ),
        (
            ImportKind::RustUse,
            "crate::models",
            Some("models"),
            Some("models"),
        ),
        (ImportKind::RustGlob, "crate::prelude", None, None),
        (
            ImportKind::RustUse,
            "::core::fmt",
            Some("fmt"),
            Some("CoreFmt"),
        ),
        (
            ImportKind::RustUse,
            "super::Outer",
            Some("Outer"),
            Some("Alias"),
        ),
        (
            ImportKind::RustUse,
            "super::inner::Thing",
            Some("Thing"),
            Some("_"),
        ),
    ];
    if imported.len() != expected.len() || expected.iter().any(|fact| !imported.contains(fact)) {
        return Err(ExtractorError::InvalidInput(format!(
            "Rust use-tree lowering was incorrect: {imported:?}"
        )));
    }

    let root_module = extraction
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Module && node.name == "src/main.rs")
        .ok_or_else(|| ExtractorError::InvalidInput("file module is missing".to_owned()))?;
    let nested_module = extraction
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Module && node.name == "nested")
        .ok_or_else(|| ExtractorError::InvalidInput("inline module is missing".to_owned()))?;
    if extraction.imports.iter().any(|item| {
        item.source.file_id != source(content).file.file_id
            || item.source.content_hash != source(content).file.content_hash
            || item.provenance != extraction.provenance.id
            || item.source.span.start_byte >= item.source.span.end_byte
            || usize::try_from(item.source.span.end_byte)
                .ok()
                .is_none_or(|end| end > content.len())
    }) || extraction
        .imports
        .iter()
        .find(|item| item.specifier == "super::Outer")
        .is_none_or(|item| item.module != nested_module.id)
        || extraction
            .imports
            .iter()
            .filter(|item| item.specifier == "super::inner::Thing")
            .any(|item| item.module != root_module.id)
    {
        return Err(ExtractorError::InvalidInput(
            "Rust use evidence or containing-module ownership was incorrect".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn rust_import_ids_ignore_line_offsets_and_distinguish_duplicates() -> Result<(), ExtractorError> {
    let first = RustExtractor.extract(&source("use std::fmt::Debug;\nuse std::fmt::Debug;\n"))?;
    let shifted = RustExtractor.extract(&source(
        "// leading line\nuse std::fmt::Debug;\nuse std::fmt::Debug;\n",
    ))?;
    let first_ids = first.imports.iter().map(|item| item.id).collect::<Vec<_>>();
    let shifted_ids = shifted
        .imports
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    if first_ids.len() != 2
        || first_ids.first() == first_ids.get(1)
        || first_ids != shifted_ids
        || first.imports.iter().any(|item| {
            item.kind != ImportKind::RustUse
                || item.specifier != "std::fmt::Debug"
                || item.local_name.as_deref() != Some("Debug")
        })
    {
        return Err(ExtractorError::InvalidInput(
            "Rust import identity depended on offsets or collapsed duplicates".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn inline_module_imports_keep_distinct_owners_when_cfg_names_repeat() -> Result<(), ExtractorError>
{
    let extraction = RustExtractor.extract(&source(
        "#[cfg(unix)] mod platform { use crate::UnixOnly; }\n#[cfg(windows)] mod platform { use crate::WindowsOnly; }\n",
    ))?;
    let modules = extraction
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Module && node.name == "platform")
        .map(|node| node.id)
        .collect::<Vec<_>>();
    let import_owners = extraction
        .imports
        .iter()
        .map(|item| item.module)
        .collect::<BTreeSet<_>>();
    if modules.len() != 2
        || import_owners.len() != 2
        || extraction.imports.len() != 2
        || modules.into_iter().collect::<BTreeSet<_>>() != import_owners
    {
        return Err(ExtractorError::InvalidInput(
            "same-named cfg modules shared Rust import ownership".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn rejects_invalid_rust() {
    let result = RustExtractor.extract(&source("fn broken( {}"));
    assert!(matches!(result, Err(ExtractorError::SyntaxError(_))));
    let template = RustExtractor.extract(&source("fn fixture_function_{NAME}() {}"));
    assert!(matches!(template, Err(ExtractorError::SyntaxError(_))));
}

#[test]
fn extracts_source_backed_references_without_resolving_them() -> Result<(), ExtractorError> {
    let source = source("fn main() { helper(); absent(); }\nfn helper() {}");
    let extraction = RustExtractor.extract(&source)?;
    let first_reference = extraction.references.first().ok_or_else(|| {
        ExtractorError::InvalidInput("first call reference was not extracted".to_owned())
    })?;
    let span_start = usize::try_from(first_reference.source_location.span.start_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let span_end = usize::try_from(first_reference.source_location.span.end_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let observed = source
        .content
        .get(span_start..span_end)
        .ok_or_else(|| ExtractorError::InvalidInput("reference span is invalid".to_owned()))?;
    if !extraction.edges.is_empty()
        || extraction.references.len() != 2
        || first_reference.target != "helper"
        || first_reference.source_location.content_hash != source.file.content_hash
        || observed != "helper"
        || !extraction
            .references
            .iter()
            .any(|reference| reference.target == "absent")
    {
        return Err(ExtractorError::InvalidInput(
            "expected unresolved source-backed call references".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn retains_generic_impl_method_symbol_path() -> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&source(
        "struct Service<T>(T); impl<T> Service<T> { fn run(&self) {} }",
    ))?;
    if !extraction
        .nodes
        .iter()
        .any(|node| node.name == "Service < T >::run")
    {
        return Err(ExtractorError::InvalidInput(
            "generic impl method lost its qualified type path".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn qualifies_self_method_calls_with_the_impl_type() -> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&source(
        "struct Service; impl Service { fn run(&self) { self.build(); } fn build(&self) {} }",
    ))?;
    if !extraction.references.iter().any(|reference| {
        reference.target == "Service::build" && reference.relation == RelationKind::Calls
    }) {
        return Err(ExtractorError::InvalidInput(
            "self method call lost its impl type context".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn identical_content_in_distinct_files_has_distinct_provenance() -> Result<(), ExtractorError> {
    let first = source("fn shared() {}");
    let mut second = first.clone();
    second.file.file_id = syntaxmesh_core::FileId::derive(&[b"src/other.rs"]);
    second.file.normalized_path = "src/other.rs".to_owned();
    let first = RustExtractor.extract(&first)?;
    let second = RustExtractor.extract(&second)?;
    if first.provenance.id == second.provenance.id
        || first.provenance.source == second.provenance.source
    {
        return Err(ExtractorError::InvalidInput(
            "identical content in distinct files shared provenance identity".to_owned(),
        ));
    }
    Ok(())
}
