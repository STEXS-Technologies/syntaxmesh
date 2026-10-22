use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use syntaxmesh_core::{GenerationId, IndexRunId};
use syntaxmesh_engine::{EngineReceipt, SyntaxMeshEngine};
use syntaxmesh_extension_ipc::{FrameCodec, MAX_FRAME_BYTES};
use syntaxmesh_extension_process::{
    ProcessLimits, ProcessReceipt, ProcessSupervisor, ProducerCommand,
};
use syntaxmesh_extension_sdk::{ExtensionManifest, FactBatch};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use super::identifiers::{parse_generation_id, parse_index_run_id};
use super::{CliError, file_engine, turso_engine, write_stdout};

const GRANT_BYTES: usize = 64 * 1024;
const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(60);

pub(super) fn ingest(arguments: impl Iterator<Item = String>, turso: bool) -> Result<(), CliError> {
    let mut arguments: Vec<_> = arguments.collect();
    let verify = arguments
        .last()
        .is_some_and(|argument| argument == "--verify");
    if verify {
        arguments.pop();
    }
    let [store, grant, frame, run, generation] = arguments.as_slice() else {
        return Err(CliError::Usage("ingest-extension[-turso] requires <store> <grant.json> <frame> <run-id> <generation-id> [--verify]".to_owned()));
    };
    let run = parse_index_run_id(run)?;
    let generation = parse_generation_id(generation)?;
    let grant: ExtensionManifest =
        serde_json::from_slice(&read_file(Path::new(grant), GRANT_BYTES)?)
            .map_err(CliError::Encode)?;
    let bytes = read_file(Path::new(frame), MAX_FRAME_BYTES + 12)?;
    let mut input = bytes.as_slice();
    let batch = FrameCodec::new(MAX_FRAME_BYTES)
        .and_then(|codec| codec.read_batch_for(&mut input, &grant))
        .map_err(|error| CliError::Usage(format!("extension frame rejected: {error}")))?
        .ok_or_else(|| CliError::Usage("extension frame is empty".to_owned()))?;
    if !input.is_empty() {
        return Err(CliError::Usage(
            "extension import requires exactly one frame without trailing bytes".to_owned(),
        ));
    }
    let ownership = super::ownership::IndexOwnership::acquire(Path::new(store))?;
    if turso {
        publish(
            turso_engine(&ownership)?.0,
            batch,
            run,
            generation,
            verify,
            None,
        )
    } else {
        publish(
            file_engine(Path::new(store))?.0,
            batch,
            run,
            generation,
            verify,
            None,
        )
    }
}

pub(super) fn run_process(
    arguments: impl Iterator<Item = String>,
    turso: bool,
) -> Result<(), CliError> {
    let mut arguments = arguments.collect::<Vec<_>>();
    let separator = arguments
        .iter()
        .position(|argument| argument == "--")
        .ok_or_else(|| {
            CliError::Usage("run-extension requires `-- <program> [args...]`".to_owned())
        })?;
    let child_arguments = arguments.split_off(separator.saturating_add(1));
    arguments.truncate(separator);
    if arguments.len() < 4 || child_arguments.is_empty() {
        return Err(CliError::Usage(
            "run-extension[-turso] requires <store> <grant.json> <run-id> <generation-id> and a program after --".to_owned(),
        ));
    }
    let generation_text = arguments.remove(3);
    let run_text = arguments.remove(2);
    let grant_path = arguments.remove(1);
    let store_path = arguments.remove(0);
    let run = parse_index_run_id(&run_text)?;
    let generation = parse_generation_id(&generation_text)?;
    let mut timeout = DEFAULT_PROCESS_TIMEOUT;
    let mut timeout_seen = false;
    let mut verify = false;
    let mut environment_names = BTreeSet::new();
    let mut index = 0_usize;
    while index < arguments.len() {
        match arguments.get(index).map(String::as_str) {
            Some("--verify") if !verify => verify = true,
            Some("--timeout-ms") if !timeout_seen => {
                let value = arguments
                    .get(index.saturating_add(1))
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        CliError::Usage(
                            "extension process timeout must be positive milliseconds".to_owned(),
                        )
                    })?;
                timeout = Duration::from_millis(value);
                timeout_seen = true;
                index = index.saturating_add(1);
            }
            Some("--env") => {
                let name = arguments
                    .get(index.saturating_add(1))
                    .ok_or_else(|| {
                        CliError::Usage("--env requires an environment variable name".to_owned())
                    })?
                    .clone();
                if name.is_empty() || name.contains(['=', '\0']) || !environment_names.insert(name)
                {
                    return Err(CliError::Usage(
                        "extension environment allowlist contains an invalid or duplicate name"
                            .to_owned(),
                    ));
                }
                index = index.saturating_add(1);
            }
            Some(other) => {
                return Err(CliError::Usage(format!(
                    "unknown or duplicate run-extension option: {other}"
                )));
            }
            None => return Err(CliError::Usage("invalid run-extension options".to_owned())),
        }
        index = index.saturating_add(1);
    }
    let program = child_arguments
        .first()
        .ok_or_else(|| CliError::Usage("run-extension requires a program".to_owned()))?;
    let grant: ExtensionManifest =
        serde_json::from_slice(&read_file(Path::new(&grant_path), GRANT_BYTES)?)
            .map_err(CliError::Encode)?;
    grant
        .validate()
        .map_err(|error| CliError::Usage(format!("invalid extension grant: {error}")))?;

    // Serialize the full external operation with other writers, not just its
    // final commit: a producer must never publish against a moving base store.
    let ownership = super::ownership::IndexOwnership::acquire(Path::new(&store_path))?;
    let mut process_command = ProducerCommand::new(program);
    process_command.args(child_arguments.iter().skip(1));
    for name in environment_names {
        let value = std::env::var_os(&name).ok_or_else(|| {
            CliError::Usage(format!(
                "allowlisted environment variable is not set: {name}"
            ))
        })?;
        process_command.env(name, value);
    }
    let supervisor = ProcessSupervisor::new(ProcessLimits::new(timeout))
        .map_err(|error| CliError::Usage(error.to_string()))?;
    let process_receipt = supervisor
        .run_batch(&mut process_command, &grant)
        .map_err(|error| CliError::Usage(format!("extension producer failed: {error}")))?;
    if !process_receipt.stderr.is_empty() {
        std::io::stderr().write_all(&process_receipt.stderr)?;
        if !process_receipt.stderr_truncated {
            std::io::stderr().write_all(b"\n")?;
        }
    }
    let summary = ProcessSummary::from(&process_receipt);
    if turso {
        publish(
            turso_engine(&ownership)?.0,
            process_receipt.batch,
            run,
            generation,
            verify,
            Some(summary),
        )
    } else {
        publish(
            file_engine(Path::new(&store_path))?.0,
            process_receipt.batch,
            run,
            generation,
            verify,
            Some(summary),
        )
    }
}

fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>, CliError> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(CliError::Usage(
            "extension inputs must be regular operator-owned files".to_owned(),
        ));
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(CliError::Usage(
            "extension input is not a regular file".to_owned(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(
        u64::try_from(limit)
            .map_err(|_error| CliError::Usage("invalid input limit".to_owned()))?
            .saturating_add(1),
    )
    .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(CliError::Usage(format!(
            "extension input exceeds {limit} bytes"
        )));
    }
    Ok(bytes)
}

fn publish<S: GraphStore + DurableRecordStore>(
    mut engine: SyntaxMeshEngine<S, RustExtractor>,
    batch: FactBatch,
    run: IndexRunId,
    generation: GenerationId,
    verify: bool,
    process: Option<ProcessSummary>,
) -> Result<(), CliError> {
    if verify {
        engine = engine.with_statechronicle_verification();
    }
    let receipt = engine.ingest_extension_facts(batch, run, generation)?;
    write_receipt(run, &receipt, process)
}

fn write_receipt(
    run: IndexRunId,
    receipt: &EngineReceipt,
    process: Option<ProcessSummary>,
) -> Result<(), CliError> {
    let mut output = serde_json::Map::from_iter([
        ("schema_version".to_owned(), serde_json::json!(1)),
        ("run_id".to_owned(), serde_json::json!(run.0.to_hex())),
        (
            "generation".to_owned(),
            serde_json::json!(receipt.publication.generation.generation.0.to_hex()),
        ),
        (
            "verification".to_owned(),
            serde_json::json!(format!("{:?}", receipt.verification)),
        ),
    ]);
    if let Some(process) = process {
        output.insert(
            "process".to_owned(),
            serde_json::json!({
                "elapsed_ms": process.elapsed_ms,
                "stdout_bytes": process.stdout_bytes,
                "stderr_truncated": process.stderr_truncated,
            }),
        );
    }
    write_stdout(&format!(
        "{}\n",
        serde_json::to_string(&output).map_err(CliError::Encode)?
    ))
}

#[derive(Clone, Copy)]
struct ProcessSummary {
    elapsed_ms: u64,
    stdout_bytes: usize,
    stderr_truncated: bool,
}

impl From<&ProcessReceipt> for ProcessSummary {
    fn from(receipt: &ProcessReceipt) -> Self {
        Self {
            elapsed_ms: u64::try_from(receipt.elapsed.as_millis()).unwrap_or(u64::MAX),
            stdout_bytes: receipt.stdout_bytes,
            stderr_truncated: receipt.stderr_truncated,
        }
    }
}
