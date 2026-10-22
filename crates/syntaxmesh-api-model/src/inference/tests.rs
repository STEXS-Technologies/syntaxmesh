use super::*;

#[test]
fn page_round_trip_preserves_witness_birth_and_selected_cursor()
-> Result<(), Box<dyn std::error::Error>> {
    let born = GenerationId::derive(&[b"born"]);
    let selected = GenerationId::derive(&[b"selected"]);
    let repository = RepositoryId::derive(&[b"repository"]);
    let worktree = WorktreeId::derive(&[b"worktree"]);
    let stream = StableId::derive("test-stream", &[b"stream"]);
    let selection = InferenceSelection {
        repository,
        worktree,
        generation: selected,
    };
    let claim = InferenceClaim {
        id: StableId::derive("test-claim", &[b"claim"]),
        derivation_class: InferenceDerivationClass::DeterministicRule,
        repository,
        worktree,
        generation: born,
        source: NodeId::derive(&[b"source"]),
        target: NodeId::derive(&[b"target"]),
        relation: RelationKind::Calls,
        rule_id: "rule".to_owned(),
        rule_version: "1".to_owned(),
        ontology_versions: BTreeMap::from([("ontology".to_owned(), "2".to_owned())]),
        configuration_fingerprint: StableId::derive("test-config", &[b"config"]),
        premises: vec![FactVersionRef {
            fact: syntaxmesh_core::FactRef::Edge(syntaxmesh_core::EdgeId::derive(&[b"premise"])),
            valid_from: born,
        }],
    };
    let response = InferencePageResponse {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding: "reviewed-rules".to_owned(),
        checkpoint: InferenceCheckpoint {
            stream,
            selection,
            sequence: 9,
            previous: Some(born),
            added_claims: 0,
            affected_claims: 0,
            closure_reasons: 0,
            active_claims: 2,
        },
        next: Some(InferencePageCursor {
            binding: "reviewed-rules".to_owned(),
            stream,
            selection,
            root: Some(StableId::derive("test-root", &[b"root"])),
            after: claim.id,
        }),
        claims: vec![claim],
    };
    let bytes = serde_json::to_vec(&response)?;
    let decoded: InferencePageResponse = serde_json::from_slice(&bytes)?;
    if decoded != response || born == selected {
        return Err("page round trip changed witness or historical coordinates".into());
    }
    Ok(())
}

#[test]
fn page_request_round_trip_preserves_full_width_limits() -> Result<(), Box<dyn std::error::Error>> {
    let request = InferencePageRequest {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding: "reviewed-rules".to_owned(),
        generation: GenerationId::derive(&[b"selected"]),
        limits: InferencePageLimits {
            claims: u64::MAX,
            payload_bytes: 4096,
            premises: 128,
            tree_pages: 64,
        },
        cursor: None,
    };
    let bytes = serde_json::to_vec(&request)?;
    let decoded: InferencePageRequest = serde_json::from_slice(&bytes)?;
    if decoded != request {
        return Err("inference request lost coordinates or fixed-width limits".into());
    }
    let mut value = serde_json::to_value(&request)?;
    let object = value.as_object_mut().ok_or("request must be an object")?;
    object.insert("database".to_owned(), serde_json::json!("/untrusted/path"));
    if serde_json::from_value::<InferencePageRequest>(value).is_ok() {
        return Err("client-selected database field accepted".into());
    }
    Ok(())
}

#[test]
fn unretained_checkpoint_is_explicit_and_classes_have_stable_names()
-> Result<(), Box<dyn std::error::Error>> {
    let response = InferenceCheckpointResponse {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding: "reviewed-rules".to_owned(),
        generation: GenerationId::derive(&[b"unretained"]),
        checkpoint: None,
    };
    let value = serde_json::to_value(&response)?;
    if value.get("checkpoint") != Some(&serde_json::Value::Null)
        || serde_json::from_value::<InferenceCheckpointResponse>(value)? != response
    {
        return Err("unretained checkpoint wire meaning changed".into());
    }
    for (class, name) in [
        (
            InferenceDerivationClass::DeterministicRule,
            "deterministic_rule",
        ),
        (InferenceDerivationClass::HeuristicRule, "heuristic_rule"),
        (InferenceDerivationClass::ModelInference, "model_inference"),
        (InferenceDerivationClass::UserDeclared, "user_declared"),
        (
            InferenceDerivationClass::RuntimeCorrelation,
            "runtime_correlation",
        ),
    ] {
        if serde_json::to_value(class)? != serde_json::json!(name)
            || serde_json::from_value::<InferenceDerivationClass>(serde_json::json!(name))? != class
        {
            return Err("inference class wire name changed".into());
        }
    }
    Ok(())
}
