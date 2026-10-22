use super::{DECLARED_SOURCE, configuration, invoke, selection, text};
use std::path::Path;
use syntaxmesh_core::StableId;
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference::{ClaimSelection, Limits, Rule};
use syntaxmesh_inference_sqlite::{InferenceMaterializer, RebuildRequest, StreamOrigin, Watermark};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

fn prepare<S: GraphStore + DurableRecordStore>(
    store: S,
    database: &Path,
) -> Result<StreamOrigin, Box<dyn std::error::Error>> {
    let (_, concept) = selection(&store)?;
    let history = store.generation_history()?;
    let manifest = &history.last().ok_or("missing fixture generation")?.manifest;
    let input = configuration(concept);
    let rules: Vec<Rule> =
        serde_json::from_value(input.get("rules").ok_or("missing rules")?.clone())?;
    let limits: Limits =
        serde_json::from_value(input.get("limits").ok_or("missing limits")?.clone())?;
    let selected = ClaimSelection {
        repository: manifest.repository,
        worktree: manifest.worktree,
        generation: manifest.generation,
    };
    let baseline = Watermark {
        operation: syntaxmesh_inference::materialization_identity(
            selected,
            &rules,
            1,
            limits.input_bytes,
        )?,
        selection: selected,
        generation_sequence: store.generation_sequence(manifest.generation)?,
        configuration: syntaxmesh_inference::rule_set_identity(&rules, 1, limits.input_bytes)?,
        claim_count: 2,
    };
    let mut engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    let request = RebuildRequest {
        generation: selected.generation,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 1000,
        rule_budget: 1,
        output_bytes: 16384,
        premise_budget: 1000,
    };
    if engine
        .initialize_inference_stream(database, &request, 4096)
        .is_ok()
        || database.exists()
    {
        return Err("missing destination did not interrupt setup before publication".into());
    }
    let records =
        engine
            .into_store()
            .records_with_prefix_page("syntaxmesh.penelope.inference", None, 32)?;
    if records.records.len() != 2 {
        return Err("interrupted setup did not retain parent and baseline intent".into());
    }
    Ok(StreamOrigin {
        stream: StableId::derive("inference-stream-v1", &[&baseline.operation.0]),
        baseline,
    })
}

#[test]
fn cli_recovers_prepared_parent_setup_after_reopen_without_resubmission()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("sample.rs"), DECLARED_SOURCE)?;
    for turso in [false, true] {
        let canonical = directory
            .path()
            .join(if turso { "graph.db" } else { "graph.snapshot" });
        let database = directory.path().join(if turso {
            "turso-inference.sqlite"
        } else {
            "file-inference.sqlite"
        });
        if turso
            && !invoke(&["turso-migrate", text(&canonical)?])?
                .status
                .success()
        {
            return Err("canonical migration failed".into());
        }
        let index_command = if turso { "index-turso" } else { "index" };
        let indexed = invoke(&[index_command, text(&root)?, text(&canonical)?])?;
        if !indexed.status.success() {
            return Err(String::from_utf8_lossy(&indexed.stderr).into_owned().into());
        }
        let expected = if turso {
            prepare(
                syntaxmesh_store_turso::TursoGraphStore::open(&canonical)?,
                &database,
            )?
        } else {
            prepare(
                syntaxmesh_store::FileGraphStore::open(&canonical)?,
                &database,
            )?
        };
        if !invoke(&["inference-migrate", text(&database)?])?
            .status
            .success()
        {
            return Err("downstream migration failed".into());
        }
        let command = if turso {
            "inference-recover-turso"
        } else {
            "inference-recover"
        };
        for attempt in 0..2 {
            let recovered = invoke(&[command, text(&canonical)?])?;
            if !recovered.status.success() {
                return Err(String::from_utf8_lossy(&recovered.stderr)
                    .into_owned()
                    .into());
            }
            let receipt: serde_json::Value = serde_json::from_slice(&recovered.stdout)?;
            let count = usize::from(attempt == 0);
            if receipt
                != serde_json::json!({"version":1,"baselines":count,"setups":count,"stream_effects":0})
            {
                return Err("CLI did not recover exactly the prepared dependency chain".into());
            }
        }
        let retained = InferenceMaterializer::open(&database)?;
        let checkpoint = retained
            .stream_checkpoint(expected, expected.baseline.selection.generation)?
            .ok_or("recovery missed initialization")?;
        if checkpoint.active_claims != 2
            || retained
                .stream_claims(
                    expected,
                    expected.baseline.selection.generation,
                    10,
                    16384,
                    1000,
                    4096,
                )?
                .claims
                .len()
                != 2
        {
            return Err("recovered initialization lost expected claims".into());
        }
    }
    Ok(())
}
