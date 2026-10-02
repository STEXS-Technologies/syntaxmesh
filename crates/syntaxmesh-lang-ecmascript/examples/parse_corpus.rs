use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn main() {
    let Some(root) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!(
            "usage: cargo run -p syntaxmesh-lang-ecmascript --example parse_corpus -- <path>"
        );
        std::process::exit(2);
    };

    let mut files = Vec::new();
    if let Err(error) = collect(&root, &mut files) {
        eprintln!("failed to scan {}: {error}", root.display());
        std::process::exit(2);
    }

    let mut failed = 0usize;
    for path in &files {
        let result = parse_file(path);
        if let Err(error) = result {
            failed = failed.saturating_add(1);
            eprintln!("{}: {error}", path.display());
        }
    }
    println!(
        "Oxc corpus parse: {} supported files, {} failures",
        files.len(),
        failed
    );
    if failed > 0 {
        std::process::exit(1);
    }
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        if source_type(path).is_some() {
            files.push(path.to_owned());
        }
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            collect(&entry?.path(), files)?;
        }
    }
    Ok(())
}

fn parse_file(path: &Path) -> Result<(), String> {
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let source_type = source_type(path).ok_or_else(|| "unsupported extension".to_owned())?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, source_type).parse();
    if parsed.fatal_error || !parsed.diagnostics.is_empty() {
        let diagnostics = parsed
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(if diagnostics.is_empty() {
            "fatal syntax error".to_owned()
        } else {
            diagnostics
        });
    }
    Ok(())
}

fn source_type(path: &Path) -> Option<SourceType> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "ts" => Some(SourceType::ts()),
        "tsx" => Some(SourceType::tsx()),
        "js" | "mjs" | "cjs" => Some(SourceType::mjs()),
        "jsx" => Some(SourceType::jsx()),
        _ => None,
    }
}
