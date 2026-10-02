use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use syntaxmesh_scanner::scan;

struct BenchmarkDirectory(PathBuf);

impl BenchmarkDirectory {
    fn new() -> Result<Self, std::io::Error> {
        let path =
            std::env::temp_dir().join(format!("syntaxmesh-scanner-bench-{}", std::process::id()));
        fs::create_dir(&path)?;
        fs::create_dir(path.join(".git"))?;
        fs::create_dir(path.join("src"))?;
        fs::create_dir(path.join("ignored"))?;
        fs::write(path.join(".gitignore"), "ignored/\n")?;
        for number in 0..1_000_u32 {
            fs::write(
                path.join(format!("src/module-{number:04}.rs")),
                format!("pub fn item_{number}() -> u32 {{ {number} }}\n"),
            )?;
        }
        for number in 0..500_u32 {
            fs::write(path.join(format!("src/data-{number:04}.txt")), "not Rust\n")?;
            fs::write(
                path.join(format!("ignored/generated-{number:04}.rs")),
                "fn generated() {}\n",
            )?;
        }
        Ok(Self(path))
    }
}

impl Drop for BenchmarkDirectory {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let fixture = BenchmarkDirectory::new()?;
    for sample in 1..=5 {
        let started = Instant::now();
        let report = scan(&fixture.0, &["rs"])?;
        let elapsed = started.elapsed();
        println!(
            "sample={sample} rust_files={} yielded_entries={} scanner_elapsed_us={} end_to_end_elapsed_us={}",
            report.metrics.files_read,
            report.metrics.entries_visited,
            report.metrics.elapsed.as_micros(),
            elapsed.as_micros()
        );
    }
    Ok(())
}
