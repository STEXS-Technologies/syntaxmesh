use super::{
    CliError,
    sync::{read_origin, read_receipt},
    write_stdout,
};
use crate::cli::{parse_generation_id, parse_limit};
use std::path::Path;
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference_sqlite::{StreamCursor, StreamOrigin, StreamPageRequest};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::InMemoryGraphStore;

pub(in crate::cli) fn checkpoint(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "inference-checkpoint <inference.sqlite> <generation-id> <origin.json>".to_owned(),
        )
    };
    let database = arguments.next().ok_or_else(usage)?;
    let generation = arguments.next().ok_or_else(usage)?;
    let receipt = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let generation = parse_generation_id(&generation)?;
    let origin = read_origin(Path::new(&receipt))?;
    let engine = detached(origin);
    let result = engine.inference_checkpoint(Path::new(&database), origin, generation)?;
    write_stdout(&serde_json::to_string(&result).map_err(CliError::Encode)?)
}

pub(in crate::cli) fn page(mut arguments: impl Iterator<Item = String>) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage("inference-claims-page <inference.sqlite> <generation-id> <origin.json> <claims> <bytes> <premises> <tree-pages> [cursor.json]".to_owned())
    };
    let database = arguments.next().ok_or_else(usage)?;
    let generation = arguments.next().ok_or_else(usage)?;
    let receipt = arguments.next().ok_or_else(usage)?;
    let claims = arguments.next().ok_or_else(usage)?;
    let bytes = arguments.next().ok_or_else(usage)?;
    let premises = arguments.next().ok_or_else(usage)?;
    let pages = arguments.next().ok_or_else(usage)?;
    let cursor_path = arguments.next();
    if arguments.next().is_some() {
        return Err(usage());
    }
    let generation = parse_generation_id(&generation)?;
    let claim_budget = parse_limit(&claims)?;
    let byte_budget = parse_limit(&bytes)?;
    let premise_budget = parse_limit(&premises)?;
    let page_budget = parse_limit(&pages)?;
    if [claim_budget, byte_budget, premise_budget, page_budget].contains(&0) {
        return Err(CliError::Usage(
            "positive inference page budgets required".to_owned(),
        ));
    }
    let origin = read_origin(Path::new(&receipt))?;
    let cursor: Option<StreamCursor> = cursor_path
        .as_deref()
        .map(|path| read_receipt(Path::new(path)))
        .transpose()?;
    let request = StreamPageRequest {
        origin,
        generation,
        cursor,
        claim_budget,
        byte_budget,
        premise_budget,
        page_budget,
    };
    let result = detached(origin).inference_claims_page(Path::new(&database), &request)?;
    write_stdout(&serde_json::to_string(&result).map_err(CliError::Encode)?)
}

fn detached(origin: StreamOrigin) -> SyntaxMeshEngine<InMemoryGraphStore, RustExtractor> {
    let selection = origin.baseline.selection;
    SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        selection.repository,
        selection.worktree,
    )
}
