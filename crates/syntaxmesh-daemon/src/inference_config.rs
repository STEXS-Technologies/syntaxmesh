use serde::Deserialize;
use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::path::{Path, PathBuf};
use syntaxmesh_api_model::InferencePageLimits;
use syntaxmesh_core::{RepositoryId, WorktreeId};
use syntaxmesh_inference_host::InferenceReadBindings;
use syntaxmesh_inference_sqlite::StreamOrigin;

const CONFIG_SCHEMA_VERSION: u32 = 1;
const MAX_CONFIG_BYTES: u64 = 1_048_576;
const MAX_ORIGIN_BYTES: u64 = 16_384;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadConfig {
    schema_version: u32,
    limits: InferencePageLimits,
    bindings: Vec<ReadBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadBinding {
    id: String,
    database: PathBuf,
    origin_file: PathBuf,
}

pub(super) fn load(
    config_path: &Path,
    source_root: &Path,
    canonical_database: &Path,
    repository: RepositoryId,
    worktree: WorktreeId,
) -> Result<InferenceReadBindings, Box<dyn std::error::Error>> {
    let config_path = config_path.canonicalize()?;
    let bytes = read_bounded(
        &config_path,
        MAX_CONFIG_BYTES,
        "inference read configuration",
    )?;
    let config: ReadConfig = serde_json::from_slice(&bytes)?;
    if config.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(invalid("unsupported inference read configuration schema"));
    }
    if config.bindings.len() > 256 {
        return Err(invalid("inference read binding limit exceeded"));
    }
    let source_root = source_root.canonicalize()?;
    let canonical_database = canonical_database.canonicalize()?;
    let config_directory = config_path
        .parent()
        .ok_or_else(|| invalid("inference read config has no parent directory"))?;
    let mut bindings =
        InferenceReadBindings::new(repository, worktree, config.limits).map_err(invalid)?;
    for binding in config.bindings {
        let database_path = if binding.database.is_absolute() {
            binding.database
        } else {
            config_directory.join(binding.database)
        };
        let database_path = database_path.canonicalize()?;
        if !database_path.is_file() {
            return Err(invalid("inference read database is not a file"));
        }
        if database_path.starts_with(&source_root) {
            return Err(invalid(
                "inference read database must be outside the indexed source root",
            ));
        }
        if database_path == canonical_database {
            return Err(invalid(
                "inference read database must not alias the canonical graph database",
            ));
        }
        let origin_path = if binding.origin_file.is_absolute() {
            binding.origin_file
        } else {
            config_directory.join(binding.origin_file)
        };
        let origin_bytes =
            read_bounded(&origin_path, MAX_ORIGIN_BYTES, "inference origin receipt")?;
        let origin: StreamOrigin = serde_json::from_slice(&origin_bytes)?;
        bindings
            .insert(binding.id, database_path, origin)
            .map_err(invalid)?;
    }
    Ok(bindings)
}

fn read_bounded(
    path: &Path,
    maximum: u64,
    label: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = path.canonicalize()?;
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len())? > maximum {
        return Err(invalid(format!("{label} exceeds its byte limit")));
    }
    Ok(bytes)
}

fn invalid(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(Error::new(ErrorKind::InvalidInput, message.into()))
}

#[cfg(test)]
mod tests;
