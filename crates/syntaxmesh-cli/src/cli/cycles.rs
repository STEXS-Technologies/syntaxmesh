use super::{CliError, file_engine, parse_generation_id, turso_engine, write_stdout};
use std::path::Path;
use syntaxmesh_core::{GenerationId, RelationKind};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

pub(super) fn run(arguments: impl Iterator<Item = String>, turso: bool) -> Result<(), CliError> {
    let arguments = arguments.collect::<Vec<_>>();
    let [store, generation, relation] = arguments.as_slice() else {
        return Err(CliError::Usage(
            "cycles[-turso] requires <store> <current|generation-id> <calls|imports|all>"
                .to_owned(),
        ));
    };
    let selected = if generation == "current" {
        None
    } else {
        Some(parse_generation_id(generation)?)
    };
    let filter = match relation.as_str() {
        "calls" => Some(RelationKind::Calls),
        "imports" => Some(RelationKind::Imports),
        "all" => None,
        _ => {
            return Err(CliError::Usage(
                "cycle relation must be calls, imports, or all".to_owned(),
            ));
        }
    };
    if turso {
        let lease = syntaxmesh_ownership_host::WriterLease::acquire(Path::new(store))?;
        let (engine, current) = turso_engine(&lease)?;
        report(
            &engine,
            selected.unwrap_or(current),
            filter.as_ref(),
            relation,
        )
    } else {
        let (engine, current) = file_engine(Path::new(store))?;
        report(
            &engine,
            selected.unwrap_or(current),
            filter.as_ref(),
            relation,
        )
    }
}

fn report<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    filter: Option<&RelationKind>,
    label: &str,
) -> Result<(), CliError> {
    let components = engine
        .query(generation)
        .cyclic_components(filter)?
        .into_iter()
        .map(|members| {
            members
                .into_iter()
                .map(|id| id.0.to_hex())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    write_stdout(
        &serde_json::to_string(&serde_json::json!({"schema_version":1,
        "generation":generation.0.to_hex(),"relation":label,"components":components}))
        .map_err(CliError::Encode)?,
    )
}
