use std::error::Error;

use serde_json::json;
use syntaxmesh_api_model::{CONTEXT_PACK_SCHEMA_VERSION, ContextItem};
use syntaxmesh_core::EvidenceClass;

#[test]
fn legacy_context_items_keep_unknown_classification() -> Result<(), Box<dyn Error>> {
    let item: ContextItem = serde_json::from_value(json!({
        "rank":1, "kind":"source_evidence", "text":"original quote",
        "node_ids":[], "edge_ids":[], "source_path":"docs/design.md",
        "line_start":1, "line_end":1
    }))?;
    if item.evidence_class.is_some() {
        return Err(std::io::Error::other("legacy context was promoted to a source fact").into());
    }
    Ok(())
}

#[test]
fn schema_two_preserves_every_existing_evidence_class() -> Result<(), Box<dyn Error>> {
    if CONTEXT_PACK_SCHEMA_VERSION != 2 {
        return Err(std::io::Error::other("classified context schema version differs").into());
    }
    for class in [
        EvidenceClass::SourceFact,
        EvidenceClass::StaticallyResolved,
        EvidenceClass::Heuristic,
        EvidenceClass::SemanticInference,
        EvidenceClass::UserAsserted,
        EvidenceClass::RuntimeObserved,
        EvidenceClass::CommittedTransition,
        EvidenceClass::ResolutionDiagnostic,
    ] {
        let item: ContextItem = serde_json::from_value(json!({
            "rank":1, "kind":"graph_path", "text":"a relationship",
            "evidence_class":class, "node_ids":[], "edge_ids":[],
            "source_path":null, "line_start":null, "line_end":null
        }))?;
        let restored: ContextItem = serde_json::from_slice(&serde_json::to_vec(&item)?)?;
        if restored.evidence_class != Some(class) || restored != item {
            return Err(
                std::io::Error::other("context classification changed on round-trip").into(),
            );
        }
    }
    Ok(())
}
