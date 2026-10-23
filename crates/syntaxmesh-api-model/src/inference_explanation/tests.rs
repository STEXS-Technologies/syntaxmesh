use super::*;
use crate::{INFERENCE_EXPLANATION_SCHEMA_VERSION, InferenceDerivationClass};
use std::collections::BTreeMap;
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, FactRef, FactVersionRef, GenerationId, NodeId, Provenance,
    ProvenanceId, RelationKind, RepositoryId, StableId, WorktreeId,
};

#[test]
fn inference_explanation_round_trips_versioned_premise_provenance()
-> Result<(), Box<dyn std::error::Error>> {
    let generation = GenerationId::derive(&[b"explanation-generation"]);
    let repository = RepositoryId::derive(&[b"explanation-repository"]);
    let worktree = WorktreeId::derive(&[b"explanation-worktree"]);
    let edge_id = EdgeId::derive(&[b"explanation-edge"]);
    let provenance_id = ProvenanceId::derive(&[b"explanation-provenance"]);
    let source = NodeId::derive(&[b"explanation-source"]);
    let target = NodeId::derive(&[b"explanation-target"]);
    let reference = FactVersionRef {
        fact: FactRef::Edge(edge_id),
        valid_from: generation,
    };
    let explanation = InferenceExplanation {
        schema_version: INFERENCE_EXPLANATION_SCHEMA_VERSION,
        selection: InferenceSelection {
            repository,
            worktree,
            generation,
        },
        claim: InferenceClaim {
            id: StableId::derive("explanation-claim", &[b"claim"]),
            derivation_class: InferenceDerivationClass::DeterministicRule,
            repository,
            worktree,
            generation,
            source,
            target,
            relation: RelationKind::External {
                namespace: "example".to_owned(),
                relation: "explains".to_owned(),
            },
            rule_id: "example:rule".to_owned(),
            rule_version: "1".to_owned(),
            ontology_versions: BTreeMap::new(),
            configuration_fingerprint: StableId::derive("explanation-config", &[b"config"]),
            premises: vec![reference],
        },
        premises: vec![InferencePremiseEvidence {
            reference,
            edge: Edge {
                id: edge_id,
                source,
                target,
                relation: RelationKind::External {
                    namespace: "example".to_owned(),
                    relation: "supports".to_owned(),
                },
                provenance: provenance_id,
                extension_payload: None,
            },
            provenance: Provenance {
                id: provenance_id,
                producer_namespace: "example-extractor".to_owned(),
                producer_version: "1".to_owned(),
                evidence_class: EvidenceClass::SourceFact,
                source: None,
            },
        }],
    };
    let encoded = serde_json::to_vec(&explanation)?;
    let decoded = serde_json::from_slice::<InferenceExplanation>(&encoded)?;
    if decoded != explanation {
        return Err("explanation round trip changed versioned evidence".into());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&encoded)?;
    value
        .as_object_mut()
        .ok_or("serialized explanation is not an object")?
        .insert("unreviewed".to_owned(), serde_json::Value::Bool(true));
    if serde_json::from_value::<InferenceExplanation>(value).is_ok() {
        return Err("explanation decoder accepted an unknown field".into());
    }
    Ok(())
}
