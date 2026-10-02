#![no_main]

use libfuzzer_sys::fuzz_target;
use syntaxmesh_core::{FileId, FileVersion};
use syntaxmesh_lang_bash::BashExtractor;
use syntaxmesh_lang_docs::DocumentationExtractor;
use syntaxmesh_lang_ecmascript::{JavaScriptExtractor, TypeScriptExtractor};
use syntaxmesh_lang_python::PythonExtractor;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::{LanguageExtractor, SourceFile};

const MAX_INPUT_BYTES: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let Some(selector) = data.first() else {
        return;
    };
    let selector = match *selector {
        b'0'..=b'8' => *selector - b'0',
        value => value % 9,
    };
    let input = &data[1..data.len().min(MAX_INPUT_BYTES.saturating_add(1))];
    let content = String::from_utf8_lossy(input).into_owned();
    let (path, extractor): (&str, &dyn LanguageExtractor) = match selector {
        0 => ("fuzz.rs", &RustExtractor),
        1 => ("fuzz.py", &PythonExtractor),
        2 => ("fuzz.ts", &TypeScriptExtractor),
        3 => ("fuzz.tsx", &TypeScriptExtractor),
        4 => ("fuzz.js", &JavaScriptExtractor),
        5 => ("fuzz.jsx", &JavaScriptExtractor),
        6 => ("fuzz.sh", &BashExtractor),
        7 => ("fuzz.md", &DocumentationExtractor),
        _ => ("fuzz.txt", &DocumentationExtractor),
    };
    let content_hash = *blake3::hash(content.as_bytes()).as_bytes();
    let source = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash,
            size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
        },
        content,
    };

    // Malformed source is expected to return an extractor error, never panic.
    let _ = extractor.extract(&source);
});
