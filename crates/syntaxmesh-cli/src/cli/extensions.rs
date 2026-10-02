use std::fs::File;
use std::io::Read;
use std::path::Path;

use syntaxmesh_core::{GenerationId, IndexRunId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_extension_ipc::{FrameCodec, MAX_FRAME_BYTES};
use syntaxmesh_extension_sdk::{ExtensionManifest, FactBatch};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

use super::identifiers::{parse_generation_id, parse_index_run_id};
use super::{CliError, file_engine, turso_engine, write_stdout};

const GRANT_BYTES: usize = 64 * 1024;

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
        publish(turso_engine(&ownership)?.0, batch, run, generation, verify)
    } else {
        publish(
            file_engine(Path::new(store))?.0,
            batch,
            run,
            generation,
            verify,
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
) -> Result<(), CliError> {
    if verify {
        engine = engine.with_statechronicle_verification();
    }
    let receipt = engine.ingest_extension_facts(batch, run, generation)?;
    let output = serde_json::json!({
        "schema_version": 1,
        "run_id": run.0.to_hex(),
        "generation": receipt.publication.generation.generation.0.to_hex(),
        "verification": format!("{:?}", receipt.verification),
    });
    write_stdout(&format!(
        "{}\n",
        serde_json::to_string(&output).map_err(CliError::Encode)?
    ))
}
