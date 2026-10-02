//! Bounded libFuzzer smoke task isolated from standard stable CI.

use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir_in;

type TaskResult<T> = Result<T, Box<dyn Error>>;

const TARGET: &str = "syntaxmesh_source_extractors";
const DEFAULT_DURATION_SECONDS: u64 = 30;

pub(super) fn run(root: &Path) -> TaskResult<()> {
    let duration = env::var("SYNTAXMESH_FUZZ_SECONDS")
        .map_or(Ok(DEFAULT_DURATION_SECONDS), |value| value.parse::<u64>())?;
    if duration == 0 {
        return Err("SYNTAXMESH_FUZZ_SECONDS must be greater than zero".into());
    }

    let seed_directory = root.join("crates/fuzz/corpus").join(TARGET);
    let scratch_root = root.join("target/fuzz-smoke");
    fs::create_dir_all(&scratch_root)?;
    let scratch_corpus = tempdir_in(scratch_root)?;
    for entry in fs::read_dir(&seed_directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), scratch_corpus.path().join(entry.file_name()))?;
        }
    }

    let fuzz_directory = root.join("crates/fuzz");
    let artifact_directory = fuzz_directory.join("artifacts").join(TARGET);
    let output = Command::new("cargo")
        .current_dir(root)
        .args([
            "+nightly",
            "fuzz",
            "run",
            "--fuzz-dir",
            fuzz_directory
                .to_str()
                .ok_or("fuzz directory is not valid UTF-8")?,
            TARGET,
        ])
        .arg(scratch_corpus.path())
        .arg("--")
        .arg(format!("-max_total_time={duration}"))
        .arg("-max_len=65537")
        .arg("-verbosity=0")
        .arg("-print_final_stats=1")
        .output()?;
    if !output.status.success() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return Err(format!(
            "source extractor fuzz smoke failed; crash artifacts, if any, are under {}",
            artifact_directory.display()
        )
        .into());
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if line.starts_with("stat::") {
            println!("{line}");
        }
    }
    println!("source extractor fuzz smoke passed ({duration}s budget)");
    Ok(())
}
