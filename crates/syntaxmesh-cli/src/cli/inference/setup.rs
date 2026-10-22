use super::{
    CliError, InferenceMaterializer, WriterLease, file_engine, turso_engine, write_stdout,
};
use crate::cli::{inference_config, parse_generation_id};
use std::path::Path;
use syntaxmesh_inference_sqlite::RebuildRequest;

pub(in crate::cli) fn setup(
    turso: bool,
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
        "inference-setup[-turso] <canonical-store> <inference.sqlite> <generation-id> <configuration.json>".to_owned(),
    )
    };
    let canonical = arguments.next().ok_or_else(usage)?;
    let database = arguments.next().ok_or_else(usage)?;
    let generation = arguments.next().ok_or_else(usage)?;
    let config = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let generation = parse_generation_id(&generation)?;
    let configuration = inference_config::load(Path::new(&config))?;
    if !Path::new(&canonical).is_file() || !Path::new(&database).is_file() {
        return Err(CliError::Usage(
            "setup requires existing canonical and migrated inference stores".to_owned(),
        ));
    }
    let canonical_lease = WriterLease::acquire(Path::new(&canonical))?;
    let downstream_lease = WriterLease::acquire(Path::new(&database))?;
    drop(
        InferenceMaterializer::open(downstream_lease.target())
            .map_err(|error| CliError::Integrity(error.to_string()))?,
    );
    let request = RebuildRequest {
        generation,
        rules: &configuration.rules,
        limits: &configuration.limits,
        history_scan_budget: configuration.history_scan_budget,
        rule_budget: configuration.rule_budget,
        output_bytes: configuration.output_bytes,
        premise_budget: configuration.premise_budget,
    };
    let registered = configuration.is_registered();
    let admission = configuration.admission();
    let origin = if turso {
        let (mut engine, _) = turso_engine(&canonical_lease)?;
        if registered {
            engine.initialize_registered_inference_stream(
                downstream_lease.target(),
                &request,
                &configuration.ontology_definitions,
                admission,
                configuration.page_budget,
            )?
        } else {
            engine.initialize_inference_stream(
                downstream_lease.target(),
                &request,
                configuration.page_budget,
            )?
        }
    } else {
        let (mut engine, _) = file_engine(canonical_lease.target())?;
        if registered {
            engine.initialize_registered_inference_stream(
                downstream_lease.target(),
                &request,
                &configuration.ontology_definitions,
                admission,
                configuration.page_budget,
            )?
        } else {
            engine.initialize_inference_stream(
                downstream_lease.target(),
                &request,
                configuration.page_budget,
            )?
        }
    };
    write_stdout(&serde_json::to_string(&origin).map_err(CliError::Encode)?)
}
