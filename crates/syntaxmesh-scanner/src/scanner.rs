use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use syntaxmesh_core::{FileId, FileVersion};
use syntaxmesh_language_sdk::SourceFile;

/// Source/document extensions supported by the configured CLI extractors.
pub const SUPPORTED_SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "mjs", "cjs", "sh", "bash", "md", "markdown", "txt",
    "text", "rst", "adoc", "asciidoc",
];

/// Failure encountered while walking or reading a source tree.
#[derive(Debug)]
pub enum ScanError {
    /// Directory traversal reported an error.
    Walk(String),
    /// A selected source file could not be read.
    Io(std::io::Error),
    /// A walked path was unexpectedly outside the requested root.
    OutsideRoot(std::path::StripPrefixError),
    /// A relative path could not be represented as UTF-8.
    NonUtf8Path(PathBuf),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Walk(error) => write!(formatter, "source traversal failed: {error}"),
            Self::Io(error) => write!(formatter, "source read failed: {error}"),
            Self::OutsideRoot(error) => write!(formatter, "source path escaped root: {error}"),
            Self::NonUtf8Path(path) => {
                write!(
                    formatter,
                    "source path is not valid UTF-8: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ScanError {}

impl From<std::io::Error> for ScanError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Counters and elapsed wall time for one completed traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanMetrics {
    /// Entries yielded by the ignore-aware walker, including directories.
    pub entries_visited: u64,
    /// Files selected and read as text inputs.
    pub files_read: u64,
    /// Time spent walking, reading, normalizing, and hashing.
    pub elapsed: Duration,
}

/// Sorted source inputs and the cost data from one scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    /// Selected files in normalized relative-path order.
    pub files: Vec<SourceFile>,
    /// Traversal counters and elapsed time.
    pub metrics: ScanMetrics,
}

/// Discover text source files under `root` whose extension matches `extensions`.
///
/// Ignore rules, including `.gitignore`, are applied by the `ignore` walker.
/// Paths are stored relative to `root` and normalized to `/` separators.
///
/// # Errors
/// Returns an error if traversal fails, a selected file cannot be read, a path
/// escapes the root, or a selected relative path is not UTF-8.
pub fn scan(root: &Path, extensions: &[&str]) -> Result<ScanReport, ScanError> {
    let started = Instant::now();
    let mut entries_visited = 0_u64;
    let mut relative_paths = Vec::new();
    let mut walker = ignore::WalkBuilder::new(root);
    walker.git_global(false);
    for entry in walker.build() {
        let entry = entry.map_err(|error| ScanError::Walk(error.to_string()))?;
        entries_visited = entries_visited.saturating_add(1);
        let path = entry.path();
        if entry.file_type().is_some_and(|kind| kind.is_file())
            && path.extension().is_some_and(|extension| {
                extension.to_str().is_some_and(|extension| {
                    extensions
                        .iter()
                        .any(|item| extension.eq_ignore_ascii_case(item))
                })
            })
        {
            relative_paths.push(
                path.strip_prefix(root)
                    .map_err(ScanError::OutsideRoot)?
                    .to_path_buf(),
            );
        }
    }
    relative_paths.sort();

    let mut files = Vec::with_capacity(relative_paths.len());
    for relative in relative_paths {
        let absolute = root.join(&relative);
        let content = std::fs::read_to_string(&absolute)?;
        let normalized_path = relative
            .to_str()
            .ok_or_else(|| ScanError::NonUtf8Path(relative.clone()))?
            .replace('\\', "/");
        let content_hash = *blake3::hash(content.as_bytes()).as_bytes();
        let size_bytes =
            u64::try_from(content.len()).map_err(|error| ScanError::Walk(error.to_string()))?;
        files.push(SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[normalized_path.as_bytes()]),
                normalized_path,
                content_hash,
                size_bytes,
            },
            content,
        });
    }

    let files_read =
        u64::try_from(files.len()).map_err(|error| ScanError::Walk(error.to_string()))?;
    Ok(ScanReport {
        files,
        metrics: ScanMetrics {
            entries_visited,
            files_read,
            elapsed: started.elapsed(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Result<Self, std::io::Error> {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "syntaxmesh-scanner-test-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir(&path)?;
            std::fs::create_dir(path.join(".git"))?;
            Ok(Self(path))
        }

        fn write(&self, relative: &str, content: &str) -> Result<(), std::io::Error> {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, content)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.0));
        }
    }

    #[test]
    fn scans_only_requested_extensions_and_honors_gitignore() -> Result<(), ScanError> {
        let directory = TestDirectory::new().map_err(ScanError::Io)?;
        directory
            .write(".gitignore", "generated/\n")
            .map_err(ScanError::Io)?;
        directory
            .write("zeta/src.rs", "fn zeta() {}\n")
            .map_err(ScanError::Io)?;
        directory
            .write("alpha/mod.rs", "fn alpha() {}\n")
            .map_err(ScanError::Io)?;
        directory
            .write("readme.md", "not rust")
            .map_err(ScanError::Io)?;
        directory
            .write("generated/ignored.rs", "fn ignored() {}\n")
            .map_err(ScanError::Io)?;

        let report = scan(&directory.0, &["rs"])?;
        let paths = report
            .files
            .iter()
            .map(|file| file.file.normalized_path.as_str())
            .collect::<Vec<_>>();
        if paths != ["alpha/mod.rs", "zeta/src.rs"] {
            return Err(ScanError::Walk(format!(
                "unexpected normalized paths: {paths:?}"
            )));
        }
        if report.metrics.files_read != 2 || report.metrics.entries_visited < 4 {
            return Err(ScanError::Walk(format!(
                "unexpected scan metrics: {:?}",
                report.metrics
            )));
        }
        Ok(())
    }

    #[test]
    fn default_cli_scan_scope_is_selected_code_languages_and_documentation() -> Result<(), ScanError>
    {
        let directory = TestDirectory::new().map_err(ScanError::Io)?;
        for (path, content) in [
            ("src/main.rs", "fn main() {}"),
            ("src/tool.py", "def tool(): pass"),
            ("src/client.ts", "export const client = true;"),
            ("src/client.js", "exports.client = true;"),
            ("scripts/check.sh", "#!/bin/sh\ntrue"),
            ("docs/guide.md", "# Guide"),
            ("docs/notes.txt", "Notes"),
            ("src/other.go", "package main"),
            ("src/other.java", "class Other {}"),
            ("src/other.rb", "puts 'other'"),
            ("src/other.c", "int main(void) { return 0; }"),
        ] {
            directory.write(path, content).map_err(ScanError::Io)?;
        }

        let report = scan(&directory.0, SUPPORTED_SOURCE_EXTENSIONS)?;
        let paths = report
            .files
            .iter()
            .map(|file| file.file.normalized_path.as_str())
            .collect::<Vec<_>>();
        let expected = [
            "docs/guide.md",
            "docs/notes.txt",
            "scripts/check.sh",
            "src/client.js",
            "src/client.ts",
            "src/main.rs",
            "src/tool.py",
        ];
        if paths != expected {
            return Err(ScanError::Walk(format!(
                "default scan scope was {paths:?}, expected {expected:?}"
            )));
        }
        Ok(())
    }

    #[test]
    fn scan_identity_uses_relative_path_and_hashes_content() -> Result<(), ScanError> {
        let directory = TestDirectory::new().map_err(ScanError::Io)?;
        directory
            .write("src/lib.rs", "fn stable() {}\n")
            .map_err(ScanError::Io)?;

        let first = scan(&directory.0, &["rs"])?;
        let second = scan(&directory.0, &["rs"])?;
        if first.files != second.files {
            return Err(ScanError::Walk(
                "identical scans produced different source identities".to_owned(),
            ));
        }
        let original = first
            .files
            .first()
            .ok_or_else(|| ScanError::Walk("expected scanned Rust source".to_owned()))?;
        let original_id = original.file.file_id;
        let original_hash = original.file.content_hash;
        directory
            .write("src/lib.rs", "fn stable() { let _ = 1; }\n")
            .map_err(ScanError::Io)?;
        let changed = scan(&directory.0, &["rs"])?;
        let changed_file = changed
            .files
            .first()
            .ok_or_else(|| ScanError::Walk("expected changed Rust source".to_owned()))?;
        if changed_file.file.file_id != original_id
            || changed_file.file.content_hash == original_hash
        {
            return Err(ScanError::Walk(
                "content change did not preserve path identity and change content hash".to_owned(),
            ));
        }
        std::fs::rename(
            directory.0.join("src/lib.rs"),
            directory.0.join("src/renamed.rs"),
        )
        .map_err(ScanError::Io)?;
        let moved = scan(&directory.0, &["rs"])?;
        let moved_file = moved
            .files
            .first()
            .ok_or_else(|| ScanError::Walk("expected renamed Rust source".to_owned()))?;
        if moved_file.file.file_id == original_id
            || moved_file.file.normalized_path != "src/renamed.rs"
        {
            return Err(ScanError::Walk(
                "renamed source did not receive its normalized path identity".to_owned(),
            ));
        }
        Ok(())
    }

    #[test]
    fn scans_case_insensitive_bash_and_document_extensions() -> Result<(), ScanError> {
        let directory = TestDirectory::new().map_err(ScanError::Io)?;
        directory
            .write("scripts/Install.BASH", "echo install\n")
            .map_err(ScanError::Io)?;
        directory
            .write("docs/Guide.MD", "# Guide\n")
            .map_err(ScanError::Io)?;
        directory
            .write("notes/Plan.TXT", "Plan\n")
            .map_err(ScanError::Io)?;
        directory
            .write("docs/Guide.RST", "Guide\n=====\n")
            .map_err(ScanError::Io)?;
        let report = scan(&directory.0, SUPPORTED_SOURCE_EXTENSIONS)?;
        let paths = report
            .files
            .iter()
            .map(|source| source.file.normalized_path.as_str())
            .collect::<Vec<_>>();
        if paths
            != [
                "docs/Guide.MD",
                "docs/Guide.RST",
                "notes/Plan.TXT",
                "scripts/Install.BASH",
            ]
        {
            return Err(ScanError::Walk(format!(
                "unexpected scanned source/document paths: {paths:?}"
            )));
        }
        Ok(())
    }
}
