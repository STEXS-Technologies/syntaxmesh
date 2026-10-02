use syntaxmesh_core::{FileId, FileVersion};

use super::*;

#[test]
fn multiline_functions_cover_bodies_and_preserve_call_owners() -> Result<(), ExtractorError> {
    let content = "# λ prefix\nasync def outer():\n    def inner():\n        helper('λ')\n    inner()\n\ndef helper(value):\n    return value\n\nclass Service:\n    def run(self):\n        helper('ok')\n";
    let original = PythonExtractor.extract(&source("src/ranges.py", content))?;
    let shifted = PythonExtractor.extract(&source("src/ranges.py", &format!("\n{content}")))?;
    for (name, expected, has_calls) in [
        (
            "outer",
            "async def outer():\n    def inner():\n        helper('λ')\n    inner()",
            true,
        ),
        ("outer::inner", "def inner():\n        helper('λ')", true),
        ("helper", "def helper(value):\n    return value", false),
        ("Service::run", "def run(self):\n        helper('ok')", true),
    ] {
        let node = original
            .nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| ExtractorError::InvalidInput(format!("missing function {name}")))?;
        let span = &node
            .source
            .as_ref()
            .ok_or_else(|| ExtractorError::InvalidInput("missing function evidence".to_owned()))?
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
            || (has_calls
                && !original
                    .references
                    .iter()
                    .any(|reference| reference.source == node.id))
        {
            return Err(ExtractorError::InvalidInput(format!(
                "incorrect definition range, identity or owner: {name}"
            )));
        }
    }
    Ok(())
}

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
fn extracts_classes_functions_and_calls_with_source_evidence() -> Result<(), ExtractorError> {
    let extraction = PythonExtractor.extract(&source(
        "src/service.py",
        "class Service:\n    def start(self):\n        build()\n\ndef build():\n    pass\n",
    ))?;
    let class = extraction
        .nodes
        .iter()
        .find(|node| node.name == "Service")
        .ok_or_else(|| ExtractorError::InvalidInput("class missing".to_owned()))?;
    let method = extraction
        .nodes
        .iter()
        .find(|node| node.name == "Service::start")
        .ok_or_else(|| ExtractorError::InvalidInput("method missing".to_owned()))?;
    let call_reference = extraction.references.first();
    if class.kind != NodeKind::Class
        || method.kind != NodeKind::Function
        || extraction.references.len() != 1
        || call_reference
            .is_none_or(|reference| reference.target != "build" || reference.source != method.id)
    {
        return Err(ExtractorError::InvalidInput(
            "Python declarations or call reference were incorrect".to_owned(),
        ));
    }
    let Some(location) = &method.source else {
        return Err(ExtractorError::InvalidInput(
            "method source missing".to_owned(),
        ));
    };
    let start = usize::try_from(location.span.start_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let end = usize::try_from(location.span.end_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    if extraction
        .provenance
        .source
        .as_ref()
        .is_none_or(|source| source.file_id != location.file_id)
        || source(
            "src/service.py",
            "class Service:\n    def start(self):\n        build()\n\ndef build():\n    pass\n",
        )
        .content
        .get(start..end)
            != Some("def start(self):\n        build()")
    {
        return Err(ExtractorError::InvalidInput(
            "source-backed declaration evidence was invalid".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn stable_ids_do_not_depend_on_line_offsets() -> Result<(), ExtractorError> {
    let first = PythonExtractor.extract(&source("src/a.py", "def stable():\n    work()\n"))?;
    let shifted = PythonExtractor.extract(&source(
        "src/a.py",
        "# a leading comment\n\ndef stable():\n    work()\n",
    ))?;
    if first.nodes.iter().map(|node| node.id).collect::<Vec<_>>()
        != shifted.nodes.iter().map(|node| node.id).collect::<Vec<_>>()
        || first
            .references
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>()
            != shifted
                .references
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>()
        || first.provenance.id == shifted.provenance.id
    {
        return Err(ExtractorError::InvalidInput(
            "source offsets changed stable declaration identity or provenance ignored content"
                .to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn identical_content_in_distinct_files_has_distinct_provenance() -> Result<(), ExtractorError> {
    let content = "def shared():\n    pass\n";
    let first = PythonExtractor.extract(&source("src/first.py", content))?;
    let second = PythonExtractor.extract(&source("src/second.py", content))?;
    if first.provenance.id == second.provenance.id {
        return Err(ExtractorError::InvalidInput(
            "identical content in distinct files shared provenance identity".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn extracts_python_module_from_and_star_imports_with_source_evidence() -> Result<(), ExtractorError>
{
    let content = "import os\nimport package.module as local_module\nfrom .helpers import build as make, run\nfrom package import *\nfrom __future__ import annotations\n";
    let extraction = PythonExtractor.extract(&source("src/service.py", content))?;
    let imports = extraction.imports;
    if imports.len() != 5 {
        return Err(ExtractorError::InvalidInput(format!(
            "expected five module/member/star imports, found {}",
            imports.len()
        )));
    }
    let module = imports
        .iter()
        .find(|record| record.specifier == "package.module")
        .ok_or_else(|| ExtractorError::InvalidInput("module import missing".to_owned()))?;
    let from_alias = imports
        .iter()
        .find(|record| {
            record.specifier == ".helpers" && record.imported_name.as_deref() == Some("build")
        })
        .ok_or_else(|| ExtractorError::InvalidInput("relative from-import missing".to_owned()))?;
    let star = imports
        .iter()
        .find(|record| record.specifier == "package" && record.kind == ImportKind::PythonStar)
        .ok_or_else(|| ExtractorError::InvalidInput("star import missing".to_owned()))?;
    let module_start = usize::try_from(module.source.span.start_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let module_end = usize::try_from(module.source.span.end_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let from_start = usize::try_from(from_alias.source.span.start_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let from_end = usize::try_from(from_alias.source.span.end_byte)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let module_binding = content.get(module_start..module_end);
    let from_binding = content.get(from_start..from_end);
    if module.kind != ImportKind::PythonModule
        || module.local_name.as_deref() != Some("local_module")
        || from_alias.kind != ImportKind::PythonFrom
        || from_alias.local_name.as_deref() != Some("make")
        || star.imported_name.is_some()
        || module_binding != Some("package.module as local_module")
        || from_binding != Some("build as make")
        || imports
            .iter()
            .any(|record| record.specifier == "__future__")
    {
        return Err(ExtractorError::InvalidInput(
            "Python import semantics or source spans were incorrect".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn python_import_ids_ignore_line_offsets_and_distinguish_duplicate_occurrences()
-> Result<(), ExtractorError> {
    let first = PythonExtractor.extract(&source(
        "src/imports.py",
        "from package import build\nfrom package import build\n",
    ))?;
    let shifted = PythonExtractor.extract(&source(
        "src/imports.py",
        "# comment\n\nfrom package import build\nfrom package import build\n",
    ))?;
    let first_ids = first.imports.iter().map(|item| item.id).collect::<Vec<_>>();
    let shifted_ids = shifted
        .imports
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    if first_ids != shifted_ids || first_ids.first() == first_ids.get(1) {
        return Err(ExtractorError::InvalidInput(
            "Python import identity depended on offsets or collapsed duplicates".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn qualifies_self_calls_and_keeps_unknown_receivers_explicit() -> Result<(), ExtractorError> {
    let extraction = PythonExtractor.extract(&source(
        "src/service.py",
        "class Service:\n    def run(self):\n        self.build()\n    def build(self):\n        pass\n\nclass Other:\n    def build(self):\n        pass\n    def call(self):\n        client.build()\n",
    ))?;
    let targets = extraction
        .references
        .iter()
        .map(|reference| reference.target.as_str())
        .collect::<Vec<_>>();
    if targets != ["Service::build", "client.build"] {
        return Err(ExtractorError::InvalidInput(format!(
            "Python method-call targets were not conservatively qualified: {targets:?}"
        )));
    }
    Ok(())
}

#[test]
fn rejects_bad_syntax_and_non_python_extensions() {
    assert!(matches!(
        PythonExtractor.extract(&source("src/broken.py", "def broken(:\n")),
        Err(ExtractorError::SyntaxError(_))
    ));
    assert_eq!(
        PythonExtractor.extract(&source("src/module.rs", "def valid(): pass")),
        Err(ExtractorError::UnsupportedFile("src/module.rs".to_owned()))
    );
}
