use super::{CliError, file_engine, turso_engine, write_stdout};
use std::path::Path;
use syntaxmesh_inference_sqlite::InferenceMaterializer;
use syntaxmesh_ownership_host::WriterLease;

mod setup;
pub(super) use setup::setup;
mod author;
pub(super) use author::retain_contracts;
mod contract_check;
pub(super) use contract_check::check_contracts;
mod as_of;
pub(super) use as_of::result_as_of;
mod outcome_as_of;
pub(super) use outcome_as_of::outcome_as_of;
mod outcome_timeline;
pub(super) use outcome_timeline::outcome_timeline;
mod contract_history;
pub(super) use contract_history::contract_history;
mod catch_up;
pub(super) use catch_up::catch_up;
mod sync;
pub(super) use sync::synchronize;
mod read;
pub(super) use read::{checkpoint, page};

pub(super) fn run(
    command: &str,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let target = arguments
        .next()
        .ok_or_else(|| CliError::Usage(format!("{command} requires one storage path")))?;
    if arguments.next().is_some() {
        return Err(CliError::Usage(format!(
            "{command} requires one storage path"
        )));
    }
    let path = Path::new(&target);
    if command != "inference-migrate" && !path.is_file() {
        return Err(CliError::Usage(
            "inference recovery requires an existing canonical store".to_owned(),
        ));
    }
    let lease = WriterLease::acquire(path)?;
    if command == "inference-migrate" {
        InferenceMaterializer::migrate(lease.target())
            .map_err(|error| CliError::Integrity(error.to_string()))?;
        return write_stdout("inference_schema_migrated=true");
    }
    let recovered = if command == "inference-recover-turso" {
        let (mut engine, _) = turso_engine(&lease)?;
        engine.recover_inference()?
    } else {
        let (mut engine, _) = file_engine(lease.target())?;
        engine.recover_inference()?
    };
    let result = serde_json::json!({
        "version": 1,
        "contract_history": recovered.contract_history,
        "baselines": recovered.baselines,
        "setups": recovered.setups,
        "stream_effects": recovered.stream_effects,
    });
    write_stdout(&serde_json::to_string(&result).map_err(CliError::Encode)?)
}
