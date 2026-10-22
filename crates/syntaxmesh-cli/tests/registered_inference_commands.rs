#![cfg(feature = "inference-sqlite")]

use std::path::Path;
use std::process::{Command, Output};
use syntaxmesh_core::NodeId;
use syntaxmesh_inference::{
    Limits, ONTOLOGY_DEFINITION_SCHEMA_VERSION, OntologyDefinition, OntologyRelationDefinition,
    Rule,
};
use syntaxmesh_query::Query;
use syntaxmesh_store::GraphStore;

#[path = "../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
mod fixture;

fn invoke(args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .args(args)
        .output()?)
}

fn text(path: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    path.to_str().ok_or_else(|| "non-UTF8 fixture path".into())
}

fn registered_configuration(rule: &Rule) -> serde_json::Value {
    let source_concept = NodeId::derive(&[b"source-concept"]);
    let relation = |name: &str| OntologyRelationDefinition {
        name: name.to_owned(),
        subject_domain: Vec::new(),
        object_range: Vec::new(),
    };
    let mut writes = relation("writes");
    writes.subject_domain.push(source_concept);
    writes.object_range.push(rule.concept);
    let mut instance = relation("instance_of");
    instance.object_range.extend([rule.concept, source_concept]);
    let mut output = relation("participates");
    output.object_range.push(rule.concept);
    let definition = OntologyDefinition {
        schema_version: ONTOLOGY_DEFINITION_SCHEMA_VERSION,
        namespace: "example".to_owned(),
        version: "1".to_owned(),
        concepts: vec![rule.concept, source_concept],
        instance_relation: "instance_of".to_owned(),
        relations: vec![writes, instance, output],
    };
    serde_json::json!({
        "version": 2,
        "rules": [rule],
        "ontology_definitions": [definition],
        "registry_entries": 16,
        "registry_bytes": 16384,
        "bindings": 1,
        "limits": {"input_bytes": 16384, "facts": 20, "edge_visits": 100, "depth": 2, "claims": 10},
        "range_limits": {"generations": 1, "versions": 100, "postings": 100, "claims": 10},
        "history_scan_budget": 100,
        "rule_budget": 1,
        "output_bytes": 16384,
        "premise_budget": 100,
        "evidence_budget": 100,
        "matched_claim_bytes": 16384,
        "page_budget": 4096
    })
}

fn exercise_registered_cli<S: GraphStore>(
    mut store: S,
    canonical: &Path,
    downstream: &Path,
    config_path: &Path,
    turso: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut rule, declared, retracted) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, declared, retracted)?;
    fixture::verify_restoration(&mut store, &rule, declared, retracted)?;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    rule.join_relation = syntaxmesh_core::RelationKind::External {
        namespace: "example".to_owned(),
        relation: "writes".to_owned(),
    };
    rule.declaration_relation = syntaxmesh_core::RelationKind::External {
        namespace: "example".to_owned(),
        relation: "instance_of".to_owned(),
    };
    rule.output_relation = syntaxmesh_core::RelationKind::External {
        namespace: "example".to_owned(),
        relation: "participates".to_owned(),
    };
    let baseline = store
        .manifest(declared)?
        .parent
        .ok_or("fixture is missing baseline generation")?;
    drop(store);

    std::fs::write(
        config_path,
        serde_json::to_vec(&registered_configuration(&rule))?,
    )?;
    let config_check = invoke(&["inference-config-check", text(config_path)?])?;
    if !config_check.status.success()
        || !String::from_utf8_lossy(&config_check.stdout).starts_with("inference_configuration=")
    {
        return Err(String::from_utf8_lossy(&config_check.stderr)
            .into_owned()
            .into());
    }

    let migrate = invoke(&["inference-migrate", text(downstream)?])?;
    if !migrate.status.success() {
        return Err(String::from_utf8_lossy(&migrate.stderr).into_owned().into());
    }
    let setup_command = if turso {
        "inference-setup-turso"
    } else {
        "inference-setup"
    };
    let setup = invoke(&[
        setup_command,
        text(canonical)?,
        text(downstream)?,
        &baseline.0.to_hex(),
        text(config_path)?,
    ])?;
    if !setup.status.success() {
        return Err(String::from_utf8_lossy(&setup.stderr).into_owned().into());
    }
    let origin: syntaxmesh_inference_sqlite::StreamOrigin = serde_json::from_slice(&setup.stdout)?;
    if origin.baseline.claim_count != 0 {
        return Err("registered baseline should precede its opening witness".into());
    }

    let receipt_path = config_path.with_extension("origin.json");
    std::fs::write(&receipt_path, &setup.stdout)?;
    let sync_command = if turso {
        "inference-sync-turso"
    } else {
        "inference-sync"
    };
    let sync = invoke(&[
        sync_command,
        text(canonical)?,
        text(downstream)?,
        &declared.0.to_hex(),
        text(config_path)?,
        text(&receipt_path)?,
    ])?;
    if !sync.status.success() {
        return Err(String::from_utf8_lossy(&sync.stderr).into_owned().into());
    }
    let checkpoint: syntaxmesh_inference_sqlite::StreamCheckpoint =
        serde_json::from_slice(&sync.stdout)?;
    if checkpoint.added_claims != 1 || checkpoint.active_claims != 1 {
        return Err("registered CLI sync did not publish the typed witness".into());
    }
    let retained = syntaxmesh_inference_sqlite::InferenceMaterializer::open(downstream)?;
    if retained
        .stream_claims(origin, declared, 10, 16_384, 100, 4096)?
        .claims
        .len()
        != 1
    {
        return Err("registered CLI receipt does not expose its retained claim".into());
    }
    Ok(())
}

#[test]
fn file_registered_configuration_runs_setup_and_successor() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let canonical = directory.path().join("registered.snapshot");
    let downstream = directory.path().join("registered-inference.sqlite");
    let config = directory.path().join("registered-config.json");
    exercise_registered_cli(
        syntaxmesh_store::FileGraphStore::open(&canonical)?,
        &canonical,
        &downstream,
        &config,
        false,
    )
}

#[test]
fn turso_registered_configuration_runs_setup_and_successor()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let canonical = directory.path().join("registered.db");
    let downstream = directory.path().join("registered-inference.sqlite");
    let config = directory.path().join("registered-config.json");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&canonical)?;
    exercise_registered_cli(
        syntaxmesh_store_turso::TursoGraphStore::open(&canonical)?,
        &canonical,
        &downstream,
        &config,
        true,
    )
}
