use std::error::Error;
use std::process::Command;
use syntaxmesh_core::{FileId, FileVersion, GenerationId, IndexRunId, RepositoryId, WorktreeId};
use syntaxmesh_engine::{SourceSyntaxPolicy, SyntaxMeshEngine};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::SourceFile;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

fn populate<S: GraphStore + DurableRecordStore>(store: S) -> Result<GenerationId, Box<dyn Error>> {
    let source = |path: &str, content: &str| -> Result<SourceFile, Box<dyn Error>> {
        Ok(SourceFile {
            file: FileVersion {
                file_id: FileId::derive(&[path.as_bytes()]),
                normalized_path: path.to_owned(),
                content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
                size_bytes: u64::try_from(content.len())?,
            },
            content: content.to_owned(),
        })
    };
    let mut engine = SyntaxMeshEngine::new(
        store,
        RustExtractor,
        RepositoryId::derive(&[b"coverage-cli"]),
        WorktreeId::derive(&[b"coverage-cli"]),
    );
    let first = GenerationId::derive(&[b"coverage-first"]);
    engine.index(
        &[source("input.rs", "fn original() {}")?],
        IndexRunId::derive(&[b"coverage-first"]),
        first,
    )?;
    engine = engine.with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures);
    engine.index(
        &[
            source("input.rs", "fn broken(")?,
            source("valid.rs", "fn valid() {}")?,
        ],
        IndexRunId::derive(&[b"coverage-partial"]),
        GenerationId::derive(&[b"coverage-partial"]),
    )?;
    Ok(first)
}

#[test]
fn cli_reports_partial_and_unclassified_history_on_file_and_turso() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let snapshot = temporary.path().join("graph.snapshot");
    let database = temporary.path().join("graph.db");
    let first = populate(syntaxmesh_store::FileGraphStore::open(&snapshot)?)?;
    syntaxmesh_store_turso::TursoGraphStore::migrate(&database)?;
    populate(syntaxmesh_store_turso::TursoGraphStore::open(&database)?)?;
    for (command, path) in [
        ("processing-coverage", &snapshot),
        ("processing-coverage-turso", &database),
    ] {
        for (generation, expected) in [
            ("current".to_owned(), (1, 1, 0)),
            (first.0.to_hex(), (0, 0, 1)),
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(command)
                .arg(path)
                .arg(&generation)
                .output()?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
            }
            let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            let count = |key: &str| {
                report
                    .get(key)
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len)
            };
            if count("completed") != Some(expected.0)
                || count("syntax_failed") != Some(expected.1)
                || count("unclassified") != Some(expected.2)
                || report
                    .get("schema_version")
                    .and_then(serde_json::Value::as_u64)
                    != Some(1)
            {
                return Err(
                    "CLI processing coverage mislabeled current or historical files".into(),
                );
            }
        }
    }
    let lease = syntaxmesh_ownership_host::WriterLease::acquire(&database)?;
    let denied = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("processing-coverage-turso")
        .arg(&database)
        .arg("current")
        .output()?;
    if denied.status.success() || !denied.stdout.is_empty() {
        return Err("processing coverage bypassed an active database owner".into());
    }
    drop(lease);
    Ok(())
}

#[test]
fn malformed_coverage_arguments_fail_before_storage_access() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let missing = temporary.path().join("must-not-create.db");
    for command in ["processing-coverage", "processing-coverage-turso"] {
        for arguments in [vec![], vec!["not-a-generation"], vec!["current", "extra"]] {
            let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(command)
                .arg(&missing)
                .args(arguments)
                .output()?;
            if output.status.success() || !output.stdout.is_empty() || missing.exists() {
                return Err(
                    "malformed processing coverage request touched storage or emitted a result"
                        .into(),
                );
            }
        }
    }
    Ok(())
}
#[test]
fn explicit_partial_index_retries_and_repairs_with_retained_history() -> Result<(), Box<dyn Error>>
{
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("sources");
    std::fs::create_dir(&root)?;
    let broken = root.join("input.rs");
    std::fs::write(root.join("valid.rs"), "fn valid() {}")?;
    for (index_command, coverage_command, filename) in [
        ("index", "processing-coverage", "graph.snapshot"),
        ("index-turso", "processing-coverage-turso", "graph.db"),
    ] {
        std::fs::write(&broken, "fn fixture_function_{NAME}() {}")?;
        let store = temporary.path().join(filename);
        if index_command == "index-turso" {
            syntaxmesh_store_turso::TursoGraphStore::migrate(&store)?;
        }
        let invoke_index = |partial: bool| {
            let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
            command
                .arg(index_command)
                .arg(&root)
                .arg(&store)
                .arg("--verify");
            if partial {
                command.arg("--record-syntax-failures");
            }
            command.output()
        };
        if invoke_index(false)?.status.success() {
            return Err("strict CLI indexing accepted invalid syntax".into());
        }
        let read_coverage = |selector: &str| -> Result<serde_json::Value, Box<dyn Error>> {
            let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(coverage_command)
                .arg(&store)
                .arg(selector)
                .output()?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
            }
            Ok(serde_json::from_slice(&output.stdout)?)
        };
        let initial = invoke_index(true)?;
        let initial_text = String::from_utf8(initial.stdout)?;
        if !initial.status.success()
            || !initial_text.contains("observed 2 files")
            || !initial_text.contains("processing completed=1 syntax_failed=1 unclassified=0")
        {
            return Err(format!(
                "partial CLI ingestion failed: {initial_text} {}",
                String::from_utf8_lossy(&initial.stderr)
            )
            .into());
        }
        let accepted = read_coverage("current")?;
        let retry = invoke_index(true)?;
        if !retry.status.success()
            || !String::from_utf8(retry.stdout)?.contains("source inventory unchanged")
            || read_coverage("current")? != accepted
        {
            return Err(
                "unchanged failed retry fabricated a generation or changed coverage".into(),
            );
        }
        std::fs::write(&broken, "fn repaired() {}")?;
        let repaired = invoke_index(true)?;
        if !repaired.status.success()
            || !String::from_utf8(repaired.stdout)?
                .contains("processing completed=2 syntax_failed=0 unclassified=0")
        {
            return Err("CLI repair did not publish completed coverage".into());
        }
        let old_generation = accepted
            .get("generation")
            .and_then(serde_json::Value::as_str)
            .ok_or("missing coverage generation")?;
        if read_coverage(old_generation)? != accepted {
            return Err("CLI repair lost retained failed-source history".into());
        }
    }
    Ok(())
}
