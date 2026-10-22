use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use crate::TaskResult;
use syntaxmesh_extension_ipc::{FrameCodec, FrameError, MAX_FRAME_BYTES};
use syntaxmesh_extension_process::{
    ProcessLimits, ProcessSupervisor, ProducerCommand, SupervisionError,
};
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

fn producer_command(binary: &Path, arguments: &[&str]) -> ProducerCommand {
    let mut command = ProducerCommand::new(binary);
    command.args(arguments);
    command
}

fn verify_process_supervision(
    binary: &Path,
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
    let receipt = supervisor.run_batch(&mut producer_command(binary, &["--emit-frame"]), &grant)?;
    let mut frame = Vec::new();
    FrameCodec::new(MAX_FRAME_BYTES)?.write_batch(&mut frame, &receipt.batch)?;
    if frame != expected_frame || receipt.stdout_bytes != expected_frame.len() {
        return Err("supervised external producer changed its authorized frame".into());
    }

    if !matches!(
        supervisor.run_batch(
            &mut producer_command(binary, &["--emit-truncated-frame-success"]),
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
        short.run_batch(&mut producer_command(binary, &["--emit-frame"]), &grant,),
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
        timed.run_batch(&mut producer_command(binary, &["--sleep"]), &grant,),
        Err(SupervisionError::TimedOut { .. })
    ) {
        return Err("supervisor did not kill a producer at its deadline".into());
    }

    let marker = fixture.join("descendant-survived-marker");
    if !matches!(
        timed.run_batch(
            &mut producer_command(
                binary,
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

fn build_producer_binary(root: &Path, manifest: &Path) -> TaskResult<std::path::PathBuf> {
    let build = Command::new("cargo")
        .current_dir(root)
        .args([
            "build",
            "--quiet",
            "--locked",
            "--no-default-features",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()?;
    require_success(&build)?;
    let metadata = Command::new("cargo")
        .current_dir(root)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()?;
    require_success(&metadata)?;
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout)?;
    let target_directory = metadata
        .get("target_directory")
        .and_then(serde_json::Value::as_str)
        .ok_or("producer Cargo metadata omitted target_directory")?;
    let binary = std::path::Path::new(target_directory)
        .join("debug")
        .join(format!(
            "syntaxmesh-out-of-tree-extension-fixture{}",
            std::env::consts::EXE_SUFFIX
        ));
    if !binary.is_file() {
        return Err(format!("built extension producer is missing: {}", binary.display()).into());
    }
    Ok(binary)
}

#[derive(Clone, Copy)]
struct CliProducerInvocation<'arguments> {
    turso: bool,
    verify: bool,
    process_options: &'arguments [&'arguments str],
    parent_environment: Option<(&'arguments str, &'arguments str)>,
    producer_arguments: &'arguments [&'arguments str],
    run: &'arguments str,
    generation: &'arguments str,
}

fn run_cli_producer(
    root: &Path,
    store: &Path,
    grant: &Path,
    binary: &Path,
    invocation: CliProducerInvocation<'_>,
) -> TaskResult<Output> {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args([
            "run",
            "--quiet",
            "--locked",
            "-p",
            "syntaxmesh-cli",
            "--",
            if invocation.turso {
                "run-extension-turso"
            } else {
                "run-extension"
            },
        ])
        .arg(store)
        .arg(grant)
        .arg(invocation.run)
        .arg(invocation.generation)
        .args(["--timeout-ms", "30000"])
        .args(invocation.process_options);
    if let Some((name, value)) = invocation.parent_environment {
        command.env(name, value);
    }
    if invocation.verify {
        command.arg("--verify");
    }
    command
        .arg("--")
        .arg(binary)
        .args(invocation.producer_arguments);
    Ok(command.output()?)
}

fn verify_cli_process_host(
    root: &Path,
    fixture: &Path,
    store: &Path,
    grant: &Path,
    binary: &Path,
    turso: bool,
    verify: bool,
) -> TaskResult<()> {
    let export_command = if turso { "export-turso" } else { "export" };
    let initial = cli(root, export_command, &[store], &[])?;
    require_success(&initial)?;
    let failed_run = blake3::hash(if turso {
        b"external-process-cli-failure-turso-run"
    } else {
        b"external-process-cli-failure-file-run"
    })
    .to_hex()
    .to_string();
    let failed_generation = blake3::hash(if turso {
        b"external-process-cli-failure-turso-generation"
    } else {
        b"external-process-cli-failure-file-generation"
    })
    .to_hex()
    .to_string();
    let failed = run_cli_producer(
        root,
        store,
        grant,
        binary,
        CliProducerInvocation {
            turso,
            verify,
            process_options: &[],
            parent_environment: None,
            producer_arguments: &["--emit-truncated-frame-success"],
            run: &failed_run,
            generation: &failed_generation,
        },
    )?;
    if failed.status.success()
        || !failed.stdout.is_empty()
        || !String::from_utf8_lossy(&failed.stderr).contains("extension producer failed")
    {
        return Err("CLI host accepted a partial extension frame".into());
    }
    let unchanged = cli(root, export_command, &[store], &[])?;
    require_success(&unchanged)?;
    if unchanged.stdout != initial.stdout {
        return Err("failed CLI producer changed the accepted graph".into());
    }

    let invalid_grant = fixture.join("invalid-process-grant.json");
    std::fs::write(
        &invalid_grant,
        br#"{"schema_version":0,"namespace":"fixture.catalog","producer_version":"1.0.0","capabilities":["analysis_facts"]}"#,
    )?;
    let marker = fixture.join("invalid-grant-process-started");
    let invalid_run = blake3::hash(b"external-process-cli-invalid-grant-run")
        .to_hex()
        .to_string();
    let invalid_generation = blake3::hash(b"external-process-cli-invalid-grant-generation")
        .to_hex()
        .to_string();
    let marker_text = marker
        .to_str()
        .ok_or("non-UTF8 invalid-grant marker path")?;
    let invalid = run_cli_producer(
        root,
        store,
        &invalid_grant,
        binary,
        CliProducerInvocation {
            turso,
            verify,
            process_options: &[],
            parent_environment: None,
            producer_arguments: &["--write-marker", marker_text],
            run: &invalid_run,
            generation: &invalid_generation,
        },
    )?;
    if invalid.status.success()
        || marker.exists()
        || !String::from_utf8_lossy(&invalid.stderr).contains("invalid extension grant")
    {
        return Err("CLI host launched a producer before validating its grant".into());
    }

    let environment_marker = fixture.join("producer-environment-default");
    let environment_marker_text = environment_marker
        .to_str()
        .ok_or("non-UTF8 environment marker path")?;
    let environment_run = blake3::hash(b"external-process-cli-environment-default-run")
        .to_hex()
        .to_string();
    let environment_generation =
        blake3::hash(b"external-process-cli-environment-default-generation")
            .to_hex()
            .to_string();
    let environment = run_cli_producer(
        root,
        store,
        grant,
        binary,
        CliProducerInvocation {
            turso,
            verify,
            process_options: &[],
            parent_environment: Some((
                "SYNTAXMESH_EXTENSION_FIXTURE_SECRET",
                "fixture-secret-value",
            )),
            producer_arguments: &["--write-environment-marker", environment_marker_text],
            run: &environment_run,
            generation: &environment_generation,
        },
    )?;
    if environment.status.success() || std::fs::read_to_string(&environment_marker)? != "unset" {
        return Err("CLI host passed ambient environment to the producer by default".into());
    }
    let allowlisted_marker = fixture.join("producer-environment-allowlisted");
    let allowlisted_marker_text = allowlisted_marker
        .to_str()
        .ok_or("non-UTF8 allowlisted environment marker path")?;
    let allowlisted_run = blake3::hash(b"external-process-cli-environment-allowlisted-run")
        .to_hex()
        .to_string();
    let allowlisted_generation =
        blake3::hash(b"external-process-cli-environment-allowlisted-generation")
            .to_hex()
            .to_string();
    let allowlisted = run_cli_producer(
        root,
        store,
        grant,
        binary,
        CliProducerInvocation {
            turso,
            verify,
            process_options: &["--env", "SYNTAXMESH_EXTENSION_FIXTURE_SECRET"],
            parent_environment: Some((
                "SYNTAXMESH_EXTENSION_FIXTURE_SECRET",
                "fixture-secret-value",
            )),
            producer_arguments: &["--write-environment-marker", allowlisted_marker_text],
            run: &allowlisted_run,
            generation: &allowlisted_generation,
        },
    )?;
    if allowlisted.status.success()
        || std::fs::read_to_string(&allowlisted_marker)? != "fixture-secret-value"
    {
        return Err(
            "CLI host failed to pass an explicitly allowlisted environment variable".into(),
        );
    }

    let run = blake3::hash(if turso {
        b"external-process-cli-success-turso-run"
    } else {
        b"external-process-cli-success-file-run"
    })
    .to_hex()
    .to_string();
    let generation = blake3::hash(if turso {
        b"external-process-cli-success-turso-generation"
    } else {
        b"external-process-cli-success-file-generation"
    })
    .to_hex()
    .to_string();
    let accepted = run_cli_producer(
        root,
        store,
        grant,
        binary,
        CliProducerInvocation {
            turso,
            verify,
            process_options: &[],
            parent_environment: None,
            producer_arguments: &["--emit-frame"],
            run: &run,
            generation: &generation,
        },
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
            != Some(if verify { "Verified" } else { "Durable" })
        || receipt
            .pointer("/process/stdout_bytes")
            .and_then(serde_json::Value::as_u64)
            .is_none_or(|bytes| bytes == 0)
    {
        return Err("CLI process host returned an incomplete publication receipt".into());
    }
    let published = cli(root, export_command, &[store], &[])?;
    require_success(&published)?;
    let exported = String::from_utf8_lossy(&published.stdout);
    if !exported.contains("RuntimeObservation") || !exported.contains("inventory") {
        return Err("CLI process host did not publish external facts and observations".into());
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
    let producer_binary = build_producer_binary(root, producer_manifest)?;
    verify_process_supervision(&producer_binary, fixture.path(), &complete.stdout)?;
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
        verify_cli_process_host(
            root,
            fixture.path(),
            &store,
            &grant,
            &producer_binary,
            turso,
            true,
        )?;
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
