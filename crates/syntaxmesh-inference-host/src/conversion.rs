use syntaxmesh_api_model::{
    INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpoint, InferenceClaim, InferenceDerivationClass,
    InferencePageCursor, InferencePageResponse, InferenceSelection,
};
use syntaxmesh_inference::{Claim, ClaimSelection, DerivationClass};
use syntaxmesh_inference_sqlite::{StreamCheckpoint, StreamPage};

/// Convert retained coordinates without dropping counts or historical scope.
/// This does not validate storage, admit a request or authenticate a caller.
///
/// # Errors
/// Rejects counts that cannot be represented by the fixed-width wire contract.
pub fn inference_checkpoint_record(value: StreamCheckpoint) -> Result<InferenceCheckpoint, String> {
    Ok(InferenceCheckpoint {
        stream: value.stream,
        selection: selection(value.selection),
        sequence: value.sequence,
        previous: value.previous,
        added_claims: count(value.added_claims)?,
        affected_claims: count(value.affected_claims)?,
        closure_reasons: count(value.closure_reasons)?,
        active_claims: count(value.active_claims)?,
    })
}

/// Convert a materializer-validated page, using the admitted operator binding.
/// This is representation conversion, not request or witness validation.
///
/// # Errors
/// Rejects counts outside the fixed-width contract or invalid binding syntax.
pub fn inference_page_response(
    binding: String,
    page: StreamPage,
) -> Result<InferencePageResponse, String> {
    super::admission::valid_identifier(&binding)?;
    let checkpoint = inference_checkpoint_record(page.checkpoint)?;
    let next = page.next.map(|cursor| InferencePageCursor {
        binding: binding.clone(),
        stream: cursor.stream,
        selection: selection(cursor.selection),
        root: cursor.root,
        after: cursor.after,
    });
    Ok(InferencePageResponse {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding,
        checkpoint,
        claims: page.claims.into_iter().map(claim).collect(),
        next,
    })
}

fn count(value: usize) -> Result<u64, String> {
    u64::try_from(value).map_err(|_error| "inference count exceeds wire contract".to_owned())
}

const fn selection(value: ClaimSelection) -> InferenceSelection {
    InferenceSelection {
        repository: value.repository,
        worktree: value.worktree,
        generation: value.generation,
    }
}

fn claim(value: Claim) -> InferenceClaim {
    InferenceClaim {
        id: value.id,
        derivation_class: match value.derivation_class {
            DerivationClass::DeterministicRule => InferenceDerivationClass::DeterministicRule,
            DerivationClass::HeuristicRule => InferenceDerivationClass::HeuristicRule,
            DerivationClass::ModelInference => InferenceDerivationClass::ModelInference,
            DerivationClass::UserDeclared => InferenceDerivationClass::UserDeclared,
            DerivationClass::RuntimeCorrelation => InferenceDerivationClass::RuntimeCorrelation,
        },
        repository: value.repository,
        worktree: value.worktree,
        generation: value.generation,
        source: value.source,
        target: value.target,
        relation: value.relation,
        rule_id: value.rule_id,
        rule_version: value.rule_version,
        ontology_versions: value.ontology_versions,
        configuration_fingerprint: value.configuration_fingerprint,
        premises: value.premises,
    }
}

#[cfg(test)]
mod tests;
