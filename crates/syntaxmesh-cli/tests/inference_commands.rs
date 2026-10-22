#![cfg(feature = "inference-sqlite")]

use std::path::Path;
use std::process::{Command, Output};
use syntaxmesh_store::GraphStore;

#[path = "inference_commands/contract_history.rs"]
mod contract_history;
#[path = "inference_commands/contracts_authoring.rs"]
mod contracts_authoring;
#[path = "inference_commands/pages.rs"]
mod pages;
#[path = "inference_commands/recovery.rs"]
mod recovery;

const DECLARED_SOURCE: &str = "pub fn example() { writer(); }\npub fn second_entry() { writer(); }\nfn writer() { target(); }\nfn target() { concept(); }\nfn concept() {}\n";
const RETRACTED_SOURCE: &str = "pub fn example() { writer(); }\npub fn second_entry() { writer(); }\nfn writer() { target(); }\nfn target() {}\nfn concept() {}\n";

fn selection<S: GraphStore>(
    store: &S,
) -> Result<(String, syntaxmesh_core::NodeId), Box<dyn std::error::Error>> {
    let history = store.generation_history()?;
    let manifest = &history.last().ok_or("fixture has no generation")?.manifest;
    let concept = store
        .nodes(manifest.generation)?
        .iter()
        .find(|node| node.kind == syntaxmesh_core::NodeKind::Function && node.name == "concept")
        .ok_or("fixture has no concept function")?
        .id;
    Ok((manifest.generation.0.to_hex(), concept))
}

fn configuration(concept: syntaxmesh_core::NodeId) -> serde_json::Value {
    let rule = syntaxmesh_inference::Rule {
        id: "example".to_owned(),
        version: "1".to_owned(),
        ontology_versions: std::collections::BTreeMap::from([(
            "example".to_owned(),
            "1".to_owned(),
        )]),
        configuration_fingerprint: syntaxmesh_core::StableId::derive("test", &[b"config"]),
        path_relation: syntaxmesh_core::RelationKind::Calls,
        join_relation: syntaxmesh_core::RelationKind::Calls,
        declaration_relation: syntaxmesh_core::RelationKind::Calls,
        concept,
        output_relation: syntaxmesh_core::RelationKind::External {
            namespace: "example".to_owned(),
            relation: "inferred-target".to_owned(),
        },
    };
    serde_json::json!({"version":1,"rules":[rule],
        "limits":{"input_bytes":1048576,"facts":1000,"edge_visits":1000,"depth":3,"claims":10},
        "range_limits":{"generations":1,"versions":1000,"postings":1000,"claims":10},
        "history_scan_budget":1000,"rule_budget":1,"output_bytes":16384,"premise_budget":1000,
        "evidence_budget":1000,"matched_claim_bytes":16384,"page_budget":4096})
}

fn invoke(args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .args(args)
        .output()?)
}

fn text(path: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    path.to_str().ok_or_else(|| "non-UTF8 fixture path".into())
}

#[test]
fn explicit_migration_is_idempotent_and_obeys_writer_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("inference.sqlite");
    for _attempt in 0..2 {
        let output = invoke(&["inference-migrate", text(&database)?])?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        syntaxmesh_inference_sqlite::InferenceMaterializer::open(&database)?;
    }
    let lease = syntaxmesh_ownership_host::WriterLease::acquire(&database)?;
    if invoke(&["inference-migrate", text(&database)?])?
        .status
        .success()
    {
        return Err("migration ignored writer ownership".into());
    }
    drop(lease);
    Ok(())
}

#[test]
fn file_and_turso_recovery_preserve_generation_and_reject_invalid_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("sample.rs"), DECLARED_SOURCE)?;
    for turso in [false, true] {
        let store = directory
            .path()
            .join(if turso { "graph.db" } else { "graph.snapshot" });
        let indexing = if turso { "index-turso" } else { "index" };
        let recovery = if turso {
            "inference-recover-turso"
        } else {
            "inference-recover"
        };
        if turso {
            let output = invoke(&["turso-migrate", text(&store)?])?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
            }
        }
        let indexed_output = invoke(&[indexing, text(&root)?, text(&store)?])?;
        if !indexed_output.status.success() {
            return Err(String::from_utf8_lossy(&indexed_output.stderr)
                .into_owned()
                .into());
        }
        let status_command = if turso { "status-turso" } else { "status" };
        let before = invoke(&[status_command, text(&store)?, text(&root)?])?;
        for _attempt in 0..2 {
            let output = invoke(&[recovery, text(&store)?])?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
            }
            let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            if receipt
                != serde_json::json!({"version":1,"contract_history":0,"baselines":0,"setups":0,"stream_effects":0})
            {
                return Err("empty recovery reported effects".into());
            }
        }
        let after = invoke(&[status_command, text(&store)?, text(&root)?])?;
        if !before.status.success() || before.stdout != after.stdout {
            return Err("recovery changed canonical generation status".into());
        }
        let (generation, concept) = if turso {
            selection(&syntaxmesh_store_turso::TursoGraphStore::open(&store)?)?
        } else {
            selection(&syntaxmesh_store::FileGraphStore::open(&store)?)?
        };
        let config = directory.path().join(if turso {
            "turso-config.json"
        } else {
            "file-config.json"
        });
        let downstream = directory.path().join(if turso {
            "turso-inference.sqlite"
        } else {
            "file-inference.sqlite"
        });
        std::fs::write(&config, serde_json::to_vec(&configuration(concept))?)?;
        let setup_command = if turso {
            "inference-setup-turso"
        } else {
            "inference-setup"
        };
        let setup_args = [
            setup_command,
            text(&store)?,
            text(&downstream)?,
            &generation,
            text(&config)?,
        ];
        if invoke(&setup_args)?.status.success() || downstream.exists() {
            return Err("setup implicitly migrated downstream storage".into());
        }
        if !invoke(&["inference-migrate", text(&downstream)?])?
            .status
            .success()
        {
            return Err("downstream fixture migration failed".into());
        }
        let initialized = invoke(&setup_args)?;
        if !initialized.status.success() {
            return Err(String::from_utf8_lossy(&initialized.stderr)
                .into_owned()
                .into());
        }
        let origin: syntaxmesh_inference_sqlite::StreamOrigin =
            serde_json::from_slice(&initialized.stdout)?;
        if origin.baseline.claim_count == 0 || invoke(&setup_args)?.stdout != initialized.stdout {
            return Err("setup did not preserve complete origin receipt on retry".into());
        }
        if syntaxmesh_inference_sqlite::InferenceMaterializer::open(&downstream)?
            .stream_checkpoint(origin, origin.baseline.selection.generation)?
            .is_none()
        {
            return Err("setup receipt has no retained checkpoint".into());
        }
        let downstream_lease = syntaxmesh_ownership_host::WriterLease::acquire(&downstream)?;
        if invoke(&setup_args)?.status.success() {
            return Err("setup ignored downstream ownership".into());
        }
        drop(downstream_lease);
        let receipt_path = directory.path().join(if turso {
            "turso-origin.json"
        } else {
            "file-origin.json"
        });
        std::fs::write(&receipt_path, &initialized.stdout)?;
        std::fs::write(root.join("sample.rs"), RETRACTED_SOURCE)?;
        let updated = invoke(&[indexing, text(&root)?, text(&store)?])?;
        if !updated.status.success() {
            return Err(String::from_utf8_lossy(&updated.stderr).into_owned().into());
        }
        let (successor, _) = if turso {
            selection(&syntaxmesh_store_turso::TursoGraphStore::open(&store)?)?
        } else {
            selection(&syntaxmesh_store::FileGraphStore::open(&store)?)?
        };
        let sync_command = if turso {
            "inference-sync-turso"
        } else {
            "inference-sync"
        };
        let sync_args = [
            sync_command,
            text(&store)?,
            text(&downstream)?,
            &successor,
            text(&config)?,
            text(&receipt_path)?,
        ];
        let synchronized = invoke(&sync_args)?;
        if !synchronized.status.success() {
            return Err(String::from_utf8_lossy(&synchronized.stderr)
                .into_owned()
                .into());
        }
        let checkpoint: syntaxmesh_inference_sqlite::StreamCheckpoint =
            serde_json::from_slice(&synchronized.stdout)?;
        if checkpoint.selection.generation.0.to_hex() != successor
            || checkpoint.active_claims != 0
            || checkpoint.affected_claims == 0
            || invoke(&sync_args)?.stdout != synchronized.stdout
            || invoke(&setup_args)?.stdout != initialized.stdout
        {
            return Err("synchronization or setup retry changed retained receipts".into());
        }
        let retained = syntaxmesh_inference_sqlite::InferenceMaterializer::open(&downstream)?;
        let baseline_claims = retained
            .stream_claims(
                origin,
                origin.baseline.selection.generation,
                10,
                16_384,
                1000,
                4096,
            )?
            .claims;
        if baseline_claims.len() != origin.baseline.claim_count {
            return Err("successor retraction lost baseline claim history".into());
        }
        // Keep the next backend's initial corpus identical to this backend's.
        std::fs::write(root.join("sample.rs"), DECLARED_SOURCE)?;
        let restoration = invoke(&[indexing, text(&root)?, text(&store)?])?;
        if !restoration.status.success() {
            return Err(String::from_utf8_lossy(&restoration.stderr)
                .into_owned()
                .into());
        }
        let (restored_generation, _) = if turso {
            selection(&syntaxmesh_store_turso::TursoGraphStore::open(&store)?)?
        } else {
            selection(&syntaxmesh_store::FileGraphStore::open(&store)?)?
        };
        let restored_output = invoke(&[
            sync_command,
            text(&store)?,
            text(&downstream)?,
            &restored_generation,
            text(&config)?,
            text(&receipt_path)?,
        ])?;
        if !restored_output.status.success() {
            return Err(String::from_utf8_lossy(&restored_output.stderr)
                .into_owned()
                .into());
        }
        let restored_checkpoint: syntaxmesh_inference_sqlite::StreamCheckpoint =
            serde_json::from_slice(&restored_output.stdout)?;
        let restored_claims = retained
            .stream_claims(
                origin,
                restored_checkpoint.selection.generation,
                10,
                16_384,
                1000,
                4096,
            )?
            .claims;
        if restored_claims.len() != baseline_claims.len()
            || restored_claims
                .iter()
                .any(|claim| baseline_claims.iter().any(|old| old.id == claim.id))
        {
            return Err("restoration revived old claims instead of new premise versions".into());
        }
        let lease = syntaxmesh_ownership_host::WriterLease::acquire(&store)?;
        pages::exercise(
            &downstream,
            &receipt_path,
            origin,
            checkpoint.selection.generation,
            &baseline_claims,
        )?;
        let historical = invoke(&[
            "inference-checkpoint",
            text(&downstream)?,
            &generation,
            text(&receipt_path)?,
        ])?;
        if !historical.status.success() {
            return Err(String::from_utf8_lossy(&historical.stderr)
                .into_owned()
                .into());
        }
        let historic_checkpoint: Option<syntaxmesh_inference_sqlite::StreamCheckpoint> =
            serde_json::from_slice(&historical.stdout)?;
        if historic_checkpoint.is_none_or(|entry| {
            entry.active_claims != origin.baseline.claim_count
                || entry.selection != origin.baseline.selection
        }) {
            return Err("detached checkpoint lost retained baseline selection".into());
        }
        let missing_inference = directory.path().join("missing-inference.sqlite");
        if invoke(&[
            "inference-checkpoint",
            text(&missing_inference)?,
            &generation,
            text(&receipt_path)?,
        ])?
        .status
        .success()
            || missing_inference.exists()
        {
            return Err("checkpoint read created missing downstream storage".into());
        }
        if invoke(&[recovery, text(&store)?])?.status.success() {
            return Err("recovery ignored canonical writer ownership".into());
        }
        drop(lease);
    }
    let missing = directory.path().join("missing");
    for command in ["inference-recover", "inference-recover-turso"] {
        if invoke(&[command, text(&missing)?])?.status.success() || missing.exists() {
            return Err("recovery created missing canonical storage".into());
        }
    }
    if invoke(&["inference-migrate", text(&missing)?, "extra"])?
        .status
        .success()
        || missing.exists()
    {
        return Err("invalid migration arguments reached storage".into());
    }
    Ok(())
}
