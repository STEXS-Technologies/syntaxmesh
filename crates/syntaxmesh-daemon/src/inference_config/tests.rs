use super::*;
use syntaxmesh_api_model::{INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpointRequest};
use syntaxmesh_core::{GenerationId, StableId};
use syntaxmesh_core::{RepositoryId, WorktreeId};
use syntaxmesh_inference::ClaimSelection;
use syntaxmesh_inference_sqlite::{InferenceMaterializer, Watermark};

fn origin(
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
) -> StreamOrigin {
    StreamOrigin {
        stream: StableId::derive("test-stream", &[b"read-config"]),
        baseline: Watermark {
            operation: StableId::derive("test-operation", &[b"read-config"]),
            selection: ClaimSelection {
                repository,
                worktree,
                generation,
            },
            generation_sequence: 0,
            configuration: StableId::derive("test-configuration", &[b"read-config"]),
            claim_count: 0,
        },
    }
}

fn config_value(database: &Path, origin_file: &Path) -> serde_json::Value {
    serde_json::json!({
        "schema_version": CONFIG_SCHEMA_VERSION,
        "limits": {
            "claims": 32,
            "payload_bytes": 65536,
            "premises": 1024,
            "tree_pages": 512
        },
        "bindings": [{"id": "rules", "database": database, "origin_file": origin_file}]
    })
}

fn write_origin(
    path: &Path,
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(
        path,
        serde_json::to_vec(&origin(repository, worktree, generation))?,
    )?;
    Ok(())
}

struct Fixture {
    _directory: tempfile::TempDir,
    source: PathBuf,
    canonical: PathBuf,
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
}

fn setup() -> Result<Fixture, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    let state = directory.path().join("state");
    std::fs::create_dir_all(&source)?;
    std::fs::create_dir_all(&state)?;
    let canonical = state.join("graph.db");
    let downstream = state.join("inference.sqlite");
    std::fs::File::create(&canonical)?;
    InferenceMaterializer::migrate(&downstream)?;
    let repository = RepositoryId::derive(&[b"read-config-repository"]);
    let worktree = WorktreeId::derive(&[b"read-config-worktree"]);
    let generation = GenerationId::derive(&[b"read-config-generation"]);
    Ok(Fixture {
        _directory: directory,
        source,
        canonical,
        repository,
        worktree,
        generation,
    })
}

#[test]
fn versioned_config_resolves_relative_database_and_preserves_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup()?;
    let state = fixture
        .canonical
        .parent()
        .ok_or("canonical database has no parent")?;
    let config_path = fixture._directory.path().join("inference-read.json");
    write_origin(
        &state.join("origin.json"),
        fixture.repository,
        fixture.worktree,
        fixture.generation,
    )?;
    std::fs::write(
        &config_path,
        serde_json::to_vec(&config_value(
            Path::new("state/inference.sqlite"),
            Path::new("state/origin.json"),
        ))?,
    )?;
    let bindings = load(
        &config_path,
        &fixture.source,
        &fixture.canonical,
        fixture.repository,
        fixture.worktree,
    )?;
    let request = InferenceCheckpointRequest {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding: "rules".to_owned(),
        generation: fixture.generation,
    };
    let (database, retained_origin, selected) = bindings.checkpoint(&request)?;
    if database != state.join("inference.sqlite")
        || retained_origin != origin(fixture.repository, fixture.worktree, fixture.generation)
        || selected != fixture.generation
    {
        return Err("relative config path or retained origin changed during admission".into());
    }
    Ok(())
}

#[test]
fn config_rejects_unknown_fields_scope_substitution_and_canonical_alias()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup()?;
    let config_path = fixture
        ._directory
        .path()
        .join("invalid-inference-read.json");
    let state = fixture
        .canonical
        .parent()
        .ok_or("canonical database has no parent")?;
    let origin_path = state.join("origin.json");
    write_origin(
        &origin_path,
        fixture.repository,
        fixture.worktree,
        fixture.generation,
    )?;
    let mut unknown = config_value(
        Path::new("state/inference.sqlite"),
        Path::new("state/origin.json"),
    );
    unknown
        .as_object_mut()
        .ok_or("config must be an object")?
        .insert("unexpected".to_owned(), serde_json::Value::Null);
    std::fs::write(&config_path, serde_json::to_vec(&unknown)?)?;
    if load(
        &config_path,
        &fixture.source,
        &fixture.canonical,
        fixture.repository,
        fixture.worktree,
    )
    .is_ok()
    {
        return Err("unknown inference config field was accepted".into());
    }
    std::fs::write(
        &config_path,
        serde_json::to_vec(&config_value(
            Path::new("state/inference.sqlite"),
            &origin_path,
        ))?,
    )?;
    write_origin(
        &origin_path,
        RepositoryId::derive(&[b"foreign-read-config-repository"]),
        fixture.worktree,
        fixture.generation,
    )?;
    if load(
        &config_path,
        &fixture.source,
        &fixture.canonical,
        fixture.repository,
        fixture.worktree,
    )
    .is_ok()
    {
        return Err("foreign inference binding scope was accepted".into());
    }
    write_origin(
        &origin_path,
        fixture.repository,
        fixture.worktree,
        fixture.generation,
    )?;
    std::fs::write(
        &config_path,
        serde_json::to_vec(&config_value(
            &fixture.canonical,
            Path::new("state/origin.json"),
        ))?,
    )?;
    if load(
        &config_path,
        &fixture.source,
        &fixture.canonical,
        fixture.repository,
        fixture.worktree,
    )
    .is_ok()
    {
        return Err("canonical graph database was accepted as downstream inference".into());
    }
    Ok(())
}

#[test]
fn config_rejects_oversized_or_missing_storage_without_creating_it()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup()?;
    let state = fixture
        .canonical
        .parent()
        .ok_or("canonical database has no parent")?;
    let config_path = fixture
        ._directory
        .path()
        .join("oversized-inference-read.json");
    let origin_path = state.join("origin.json");
    write_origin(
        &origin_path,
        fixture.repository,
        fixture.worktree,
        fixture.generation,
    )?;
    std::fs::write(
        &config_path,
        vec![b' '; usize::try_from(MAX_CONFIG_BYTES + 1)?],
    )?;
    if load(
        &config_path,
        &fixture.source,
        &fixture.canonical,
        fixture.repository,
        fixture.worktree,
    )
    .is_ok()
    {
        return Err("oversized inference config was accepted".into());
    }
    let missing = fixture._directory.path().join("missing.sqlite");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&config_value(&missing, Path::new("state/origin.json")))?,
    )?;
    if load(
        &config_path,
        &fixture.source,
        &fixture.canonical,
        fixture.repository,
        fixture.worktree,
    )
    .is_ok()
        || missing.exists()
    {
        return Err("missing downstream storage was created or accepted".into());
    }
    Ok(())
}

#[test]
fn config_rejects_oversized_origin_receipt() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup()?;
    let state = fixture
        .canonical
        .parent()
        .ok_or("canonical database has no parent")?;
    let config_path = fixture
        ._directory
        .path()
        .join("oversized-origin-config.json");
    let origin_path = state.join("origin.json");
    std::fs::write(
        &origin_path,
        vec![b' '; usize::try_from(MAX_ORIGIN_BYTES + 1)?],
    )?;
    std::fs::write(
        &config_path,
        serde_json::to_vec(&config_value(
            Path::new("state/inference.sqlite"),
            Path::new("state/origin.json"),
        ))?,
    )?;
    if load(
        &config_path,
        &fixture.source,
        &fixture.canonical,
        fixture.repository,
        fixture.worktree,
    )
    .is_ok()
    {
        return Err("oversized inference origin receipt was accepted".into());
    }
    Ok(())
}
