use super::*;
use std::collections::BTreeMap;
use syntaxmesh_core::{
    EdgeId, FactRef, FactVersionRef, GenerationId, NodeId, RelationKind, RepositoryId, StableId,
    WorktreeId,
};
use syntaxmesh_inference_sqlite::StreamCursor;

#[test]
fn converts_exact_witness_and_distinct_historical_coordinates() -> Result<(), String> {
    let born = GenerationId::derive(&[b"born"]);
    let selected = GenerationId::derive(&[b"selected"]);
    let selection = ClaimSelection {
        repository: RepositoryId::derive(&[b"repository"]),
        worktree: WorktreeId::derive(&[b"worktree"]),
        generation: selected,
    };
    let identity = StableId::derive("test-claim", &[b"claim"]);
    let configuration = StableId::derive("test-config", &[b"configuration"]);
    let source = NodeId::derive(&[b"source"]);
    let target = NodeId::derive(&[b"target"]);
    let premise = FactVersionRef {
        fact: FactRef::Edge(EdgeId::derive(&[b"premise"])),
        valid_from: born,
    };
    let stream = StableId::derive("test-stream", &[b"stream"]);
    let root = Some(StableId::derive("test-root", &[b"root"]));
    let page = StreamPage {
        checkpoint: StreamCheckpoint {
            stream,
            selection,
            sequence: 12,
            previous: Some(born),
            added_claims: 2,
            affected_claims: 3,
            closure_reasons: 4,
            active_claims: 5,
        },
        claims: vec![Claim {
            id: identity,
            derivation_class: DerivationClass::DeterministicRule,
            repository: selection.repository,
            worktree: selection.worktree,
            generation: born,
            source,
            target,
            relation: RelationKind::Calls,
            rule_id: "rule".to_owned(),
            rule_version: "1".to_owned(),
            ontology_versions: BTreeMap::from([("ontology".to_owned(), "2".to_owned())]),
            configuration_fingerprint: configuration,
            premises: vec![premise],
        }],
        next: Some(StreamCursor {
            stream,
            selection,
            root,
            after: identity,
        }),
    };
    let response = inference_page_response("rules".to_owned(), page)?;
    let witness = response.claims.first().ok_or("claim omitted")?;
    let cursor = response.next.as_ref().ok_or("cursor omitted")?;
    if response.schema_version != INFERENCE_READ_SCHEMA_VERSION
        || response.binding != "rules"
        || response.checkpoint.selection != super::selection(selection)
        || response.checkpoint.stream != stream
        || response.checkpoint.sequence != 12
        || response.checkpoint.previous != Some(born)
        || response.checkpoint.added_claims != 2
        || response.checkpoint.affected_claims != 3
        || response.checkpoint.closure_reasons != 4
        || response.checkpoint.active_claims != 5
        || witness.id != identity
        || witness.generation != born
        || witness.repository != selection.repository
        || witness.worktree != selection.worktree
        || witness.derivation_class != InferenceDerivationClass::DeterministicRule
        || witness.source != source
        || witness.target != target
        || witness.relation != RelationKind::Calls
        || witness.rule_id != "rule"
        || witness.rule_version != "1"
        || witness
            .ontology_versions
            .get("ontology")
            .map(String::as_str)
            != Some("2")
        || witness.configuration_fingerprint != configuration
        || witness.premises != vec![premise]
        || cursor.binding != "rules"
        || cursor.stream != stream
        || cursor.selection != super::selection(selection)
        || cursor.root != root
        || cursor.after != identity
    {
        return Err("adapter conversion lost witness or retained coordinates".to_owned());
    }
    Ok(())
}
