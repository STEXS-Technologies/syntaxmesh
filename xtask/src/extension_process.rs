use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use crate::TaskResult;
use syntaxmesh_extension_ipc::{FrameCodec, FrameError, MAX_FRAME_BYTES};
use syntaxmesh_extension_process::{ProcessLimits, ProcessSupervisor, SupervisionError};
use syntaxmesh_extension_sdk::{Capability, EXTENSION_MANIFEST_SCHEMA_VERSION, ExtensionManifest};

#[cfg(test)]
mod tests;

fn verify_thin_dependencies(dependency_text: &str) -> TaskResult<()> {
    for name in dependency_text
        .lines()
        .filter_map(|line| line.split_whitespace().next())
    {
        if [
            "syntaxmesh-engine",
            "syntaxmesh-store",
            "syntaxmesh-workflow",
            "turso",
            "rusqlite",
            "sqlite",
            "duckdb",
            "penelope",
            "statechronicle",
        ]
        .contains(&name)
            || name.starts_with("syntaxmesh-store-")
            || name.starts_with("syntaxmesh-integration-")
        {
            return Err(format!("thin external producer unexpectedly depends on {name}").into());
        }
    }
    Ok(())
}

fn cli(root: &Path, command: &str, paths: &[&Path], extras: &[&str]) -> TaskResult<Output> {
    Ok(Command::new("cargo")
        .current_dir(root)
        .args([
            "run",
            "--quiet",
            "--locked",
            "-p",
            "syntaxmesh-cli",
            "--",
            command,
        ])
        .args(paths)
        .args(extras)
        .output()?)
}

fn require_success(output: &Output) -> TaskResult<()> {
    if !output.status.success() {
        return Err(format!(
            "extension conformance CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn producer_command(root: &Path, producer_manifest: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args([
            "run",
            "--quiet",
            "--locked",
            "--no-default-features",
            "--manifest-path",
        ])
        .arg(producer_manifest)
        .arg("--")
        .args(arguments);
    command
}

fn verify_process_supervision(
    root: &Path,
    producer_manifest: &Path,
    fixture: &Path,
    expected_frame: &[u8],
) -> TaskResult<()> {
    let grant = ExtensionManifest {
        schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
        namespace: "fixture.catalog".to_owned(),
        producer_version: "1.0.0".to_owned(),
        capabilities: vec![Capability::AnalysisFacts, Capability::RuntimeObservations],
    };
    let supervisor = ProcessSupervisor::new(ProcessLimits::new(Duration::from_secs(120)))?;
    let receipt = supervisor.run_batch(
        &mut producer_command(root, producer_manifest, &["--emit-frame"]),
        &grant,
    )?;
    let mut frame = Vec::new();
    FrameCodec::new(MAX_FRAME_BYTES)?.write_batch(&mut frame, &receipt.batch)?;
    if frame != expected_frame || receipt.stdout_bytes != expected_frame.len() {
        return Err("supervised external producer changed its authorized frame".into());
    }

    if !matches!(
        supervisor.run_batch(
            &mut producer_command(root, producer_manifest, &["--emit-truncated-frame-success"],),
            &grant,
        ),
        Err(SupervisionError::Frame(FrameError::Io(_)))
    ) {
        return Err("supervisor did not reject a successful process with a partial frame".into());
    }

    let short = ProcessSupervisor::new(ProcessLimits {
        timeout: Duration::from_secs(120),
        stdout_bytes: 64,
        stderr_bytes: 128,
    })?;
    if !matches!(
        short.run_batch(
            &mut producer_command(root, producer_manifest, &["--emit-frame"]),
            &grant,
        ),
        Err(SupervisionError::StdoutLimit)
    ) {
        return Err("supervisor did not enforce the external producer output cap".into());
    }

    let timed = ProcessSupervisor::new(ProcessLimits {
        timeout: Duration::from_secs(1),
        stdout_bytes: MAX_FRAME_BYTES + 12,
        stderr_bytes: 128,
    })?;
    if !matches!(
        timed.run_batch(
            &mut producer_command(root, producer_manifest, &["--sleep"]),
            &grant,
        ),
        Err(SupervisionError::TimedOut { .. })
    ) {
        return Err("supervisor did not kill a producer at its deadline".into());
    }

    let marker = fixture.join("descendant-survived-marker");
    if !matches!(
        timed.run_batch(
            &mut producer_command(
                root,
                producer_manifest,
                &[
                    "--spawn-descendant",
                    marker.to_str().ok_or("non-UTF8 process-test marker path")?,
                ],
            ),
            &grant,
        ),
        Err(SupervisionError::TimedOut { .. })
    ) {
        return Err("supervisor did not time out the producer with a child process".into());
    }
    std::thread::sleep(Duration::from_secs(2));
    if marker.exists() {
        return Err("producer descendant survived process-group cleanup".into());
    }
    Ok(())
}

pub(crate) fn verify(root: &Path, producer_manifest: &Path) -> TaskResult<()> {
    let dependencies = Command::new("cargo")
        .current_dir(root)
        .args([
            "tree",
            "--locked",
            "--no-default-features",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--manifest-path",
        ])
        .arg(producer_manifest)
        .output()?;
    require_success(&dependencies)?;
    let dependency_text = String::from_utf8(dependencies.stdout)?;
    verify_thin_dependencies(&dependency_text)?;
    let produce = |mode: &str| {
        Command::new("cargo")
            .current_dir(root)
            .args([
                "run",
                "--quiet",
                "--locked",
                "--no-default-features",
                "--manifest-path",
            ])
            .arg(producer_manifest)
            .args(["--", mode])
            .output()
    };
    let complete = produce("--emit-frame")?;
    require_success(&complete)?;
    let fixture = tempfile::tempdir()?;
    verify_process_supervision(root, producer_manifest, fixture.path(), &complete.stdout)?;
    let partial = produce("--emit-truncated-frame")?;
    if partial.status.success()
        || partial.stdout.len() != 11
        || !String::from_utf8_lossy(&partial.stderr).contains("injected external producer failure")
    {
        return Err("external producer did not fail after its partial header".into());
    }
    let source = fixture.path().join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("lib.rs"), "pub fn fixture() {}\n")?;
    let grant = fixture.path().join("operator-grant.json");
    // Operator policy is independent of the producer's self-declared manifest.
    std::fs::write(&grant, br#"{"schema_version":1,"namespace":"fixture.catalog","producer_version":"1.0.0","capabilities":["analysis_facts","runtime_observations"]}"#)?;
    let frame = fixture.path().join("producer.frame");
    let run = blake3::hash(b"external-producer-conformance-run")
        .to_hex()
        .to_string();
    let generation = blake3::hash(b"external-producer-conformance-generation")
        .to_hex()
        .to_string();
    for turso in [false, true] {
        let store = fixture
            .path()
            .join(if turso { "graph.db" } else { "graph.snapshot" });
        if turso {
            require_success(&cli(root, "turso-migrate", &[&store], &[])?)?;
        }
        require_success(&cli(
            root,
            if turso { "index-turso" } else { "index" },
            &[&source, &store],
            &["--verify"],
        )?)?;
        let export_command = if turso { "export-turso" } else { "export" };
        let initial = cli(root, export_command, &[&store], &[])?;
        require_success(&initial)?;
        let status_command = if turso { "status-turso" } else { "status" };
        let initial_status = cli(root, status_command, &[&store], &[])?;
        require_success(&initial_status)?;
        std::fs::write(&frame, &partial.stdout)?;
        let command = if turso {
            "ingest-extension-turso"
        } else {
            "ingest-extension"
        };
        let rejected = cli(
            root,
            command,
            &[&store, &grant, &frame],
            &[&run, &generation, "--verify"],
        )?;
        if rejected.status.success()
            || !rejected.stdout.is_empty()
            || !String::from_utf8_lossy(&rejected.stderr).contains("extension frame rejected")
        {
            return Err("partial external producer frame was not rejected".into());
        }
        let after_rejection = cli(root, export_command, &[&store], &[])?;
        require_success(&after_rejection)?;
        if after_rejection.stdout != initial.stdout {
            return Err("producer failure mutated accepted graph".into());
        }
        let after_status = cli(root, status_command, &[&store], &[])?;
        require_success(&after_status)?;
        if after_status.stdout != initial_status.stdout {
            return Err("producer failure changed engine/workflow diagnostics".into());
        }
        std::fs::write(&frame, &complete.stdout)?;
        let accepted = cli(
            root,
            command,
            &[&store, &grant, &frame],
            &[&run, &generation, "--verify"],
        )?;
        require_success(&accepted)?;
        let receipt: serde_json::Value = serde_json::from_slice(&accepted.stdout)?;
        if receipt
            .get("generation")
            .and_then(serde_json::Value::as_str)
            != Some(generation.as_str())
            || receipt
                .get("verification")
                .and_then(serde_json::Value::as_str)
                != Some("Verified")
        {
            return Err(
                "independent producer import did not return a verified publication receipt".into(),
            );
        }
        let search = if turso { "search-turso" } else { "search" };
        for needle in ["inventory", "out-of-tree-probe reports healthy"] {
            let found = cli(root, search, &[&store], &[needle])?;
            require_success(&found)?;
            if !String::from_utf8_lossy(&found.stdout).contains(needle) {
                return Err("independent producer fact was not queryable after CLI exit".into());
            }
        }
        let audit = cli(
            root,
            if turso {
                "statechronicle-verify-turso"
            } else {
                "statechronicle-verify"
            },
            &[&store],
            &[],
        )?;
        require_success(&audit)?;
        if !String::from_utf8_lossy(&audit.stdout).contains("statechronicle_history=verified") {
            return Err("independent producer import history was not recorded and verified".into());
        }
    }
    println!(
        "independent extension producer: verified File/Turso import and partial-process-failure isolation OK"
    );
    Ok(())
}
