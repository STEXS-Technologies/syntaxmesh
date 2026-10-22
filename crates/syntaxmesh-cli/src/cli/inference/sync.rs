use super::{
    CliError, InferenceMaterializer, WriterLease, file_engine, turso_engine, write_stdout,
};
use crate::cli::{inference_config, parse_generation_id};
use std::io::Read;
use std::path::Path;
use syntaxmesh_inference_sqlite::{StreamOrigin, StreamSyncRequest};

pub(in crate::cli) fn synchronize(
    turso: bool,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage("inference-sync[-turso] <canonical-store> <inference.sqlite> <generation-id> <configuration.json> <origin.json>".to_owned())
    };
    let canonical = arguments.next().ok_or_else(usage)?;
    let database = arguments.next().ok_or_else(usage)?;
    let generation = arguments.next().ok_or_else(usage)?;
    let config = arguments.next().ok_or_else(usage)?;
    let receipt = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let through = parse_generation_id(&generation)?;
    let configuration = inference_config::load(Path::new(&config))?;
    let origin = read_origin(Path::new(&receipt))?;
    if !Path::new(&canonical).is_file() || !Path::new(&database).is_file() {
        return Err(CliError::Usage(
            "synchronization requires existing canonical and migrated inference stores".to_owned(),
        ));
    }
    let canonical_lease = WriterLease::acquire(Path::new(&canonical))?;
    let downstream_lease = WriterLease::acquire(Path::new(&database))?;
    drop(
        InferenceMaterializer::open(downstream_lease.target())
            .map_err(|error| CliError::Integrity(error.to_string()))?,
    );
    let request = StreamSyncRequest {
        origin,
        through,
        rules: &configuration.rules,
        limits: &configuration.limits,
        range_limits: &configuration.range_limits,
        history_scan_budget: configuration.history_scan_budget,
        rule_budget: configuration.rule_budget,
        output_bytes: configuration.output_bytes,
        premise_budget: configuration.premise_budget,
        evidence_budget: configuration.evidence_budget,
        matched_claim_bytes: configuration.matched_claim_bytes,
        page_budget: configuration.page_budget,
    };
    let registered = configuration.is_registered();
    let admission = configuration.admission();
    let checkpoint = if turso {
        let (mut engine, _) = turso_engine(&canonical_lease)?;
        if registered {
            engine.synchronize_registered_inference_stream(
                downstream_lease.target(),
                &request,
                &configuration.ontology_definitions,
                admission,
            )?
        } else {
            engine.synchronize_inference_stream(downstream_lease.target(), &request)?
        }
    } else {
        let (mut engine, _) = file_engine(canonical_lease.target())?;
        if registered {
            engine.synchronize_registered_inference_stream(
                downstream_lease.target(),
                &request,
                &configuration.ontology_definitions,
                admission,
            )?
        } else {
            engine.synchronize_inference_stream(downstream_lease.target(), &request)?
        }
    };
    write_stdout(&serde_json::to_string(&checkpoint).map_err(CliError::Encode)?)
}

pub(super) fn read_origin(path: &Path) -> Result<StreamOrigin, CliError> {
    read_receipt(path)
}

pub(super) fn read_receipt<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, CliError> {
    const MAX_BYTES: u64 = 16_384;
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(CliError::Usage(
            "origin receipt must be a regular file".to_owned(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len())
        .map_err(|_error| CliError::Usage("origin size overflow".to_owned()))?
        > MAX_BYTES
    {
        return Err(CliError::Usage("origin receipt exceeds 16 KiB".to_owned()));
    }
    serde_json::from_slice(&bytes).map_err(CliError::Encode)
}

#[cfg(test)]
mod tests;
