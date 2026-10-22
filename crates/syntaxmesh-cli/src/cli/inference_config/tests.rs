use super::*;
use std::collections::BTreeMap;
use syntaxmesh_core::{NodeId, RelationKind};
use syntaxmesh_inference::OntologyRelationDefinition;

fn fixture() -> serde_json::Value {
    let rule = Rule {
        id: "example".to_owned(),
        version: "1".to_owned(),
        ontology_versions: BTreeMap::new(),
        configuration_fingerprint: StableId::derive("test", &[b"configuration"]),
        path_relation: RelationKind::Calls,
        join_relation: RelationKind::Calls,
        declaration_relation: RelationKind::Calls,
        concept: NodeId::derive(&[b"concept"]),
        output_relation: RelationKind::Calls,
    };
    serde_json::json!({
        "version": 1, "rules": [rule],
        "limits": {"input_bytes": 16384, "facts": 100, "edge_visits": 100, "depth": 3, "claims": 10},
        "range_limits": {"generations": 1, "versions": 100, "postings": 100, "claims": 10},
        "history_scan_budget": 100, "rule_budget": 1, "output_bytes": 16384,
        "premise_budget": 100, "evidence_budget": 100, "matched_claim_bytes": 16384, "page_budget": 100
    })
}

fn set(
    value: &mut serde_json::Value,
    pointer: &str,
    replacement: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(key) = pointer.strip_prefix('/')
        && !key.contains('/')
    {
        value
            .as_object_mut()
            .ok_or("test root is not an object")?
            .insert(key.to_owned(), replacement);
        return Ok(());
    }
    *value.pointer_mut(pointer).ok_or("missing test field")? = replacement;
    Ok(())
}

fn registered_fixture() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let rule_value = fixture();
    let rule = rule_value
        .pointer("/rules/0")
        .ok_or("missing fixture rule")?
        .clone();
    let mut rule: Rule = serde_json::from_value(rule)?;
    let source_concept = NodeId::derive(&[b"source-concept"]);
    let concept = rule.concept;
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    rule.join_relation = RelationKind::External {
        namespace: "example".to_owned(),
        relation: "writes".to_owned(),
    };
    rule.declaration_relation = RelationKind::External {
        namespace: "example".to_owned(),
        relation: "instance_of".to_owned(),
    };
    rule.output_relation = RelationKind::External {
        namespace: "example".to_owned(),
        relation: "participates".to_owned(),
    };
    let relation = |name: &str| OntologyRelationDefinition {
        name: name.to_owned(),
        subject_domain: Vec::new(),
        object_range: Vec::new(),
    };
    let mut writes = relation("writes");
    writes.subject_domain.push(source_concept);
    writes.object_range.push(concept);
    let mut instance = relation("instance_of");
    instance.object_range.extend([concept, source_concept]);
    let mut output = relation("participates");
    output.object_range.push(concept);
    let definition = OntologyDefinition {
        schema_version: syntaxmesh_inference::ONTOLOGY_DEFINITION_SCHEMA_VERSION,
        namespace: "example".to_owned(),
        version: "1".to_owned(),
        concepts: vec![concept, source_concept],
        instance_relation: "instance_of".to_owned(),
        relations: vec![writes, instance, output],
    };
    let mut config = fixture();
    set(&mut config, "/version", serde_json::json!(2))?;
    set(&mut config, "/rules", serde_json::json!([rule]))?;
    set(
        &mut config,
        "/ontology_definitions",
        serde_json::json!([definition]),
    )?;
    set(&mut config, "/registry_entries", serde_json::json!(16))?;
    set(&mut config, "/registry_bytes", serde_json::json!(16_384))?;
    set(&mut config, "/bindings", serde_json::json!(1))?;
    Ok(config)
}

#[test]
fn admits_explicit_configuration_and_preserves_semantic_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let value = fixture();
    let configuration = decode(serde_json::to_vec(&value)?.as_slice())?;
    let first = syntaxmesh_inference::rule_set_identity(&configuration.rules, 1, 16384)?;
    let mut changed = value;
    *changed
        .pointer_mut("/page_budget")
        .ok_or("missing page budget")? = serde_json::json!(200);
    let second = decode(serde_json::to_vec(&changed)?.as_slice())?;
    if first != syntaxmesh_inference::rule_set_identity(&second.rules, 1, 16384)? {
        return Err("execution admission changed semantic identity".into());
    }
    Ok(())
}

#[test]
fn rejects_unknown_nested_fields_versions_zero_limits_and_empty_rules()
-> Result<(), Box<dyn std::error::Error>> {
    for pointer in [
        "/extra",
        "/limits/extra",
        "/range_limits/extra",
        "/rules/0/extra",
    ] {
        let mut value = fixture();
        let (parent, key) = pointer.rsplit_once('/').ok_or("invalid test pointer")?;
        let object = if parent.is_empty() {
            Some(&mut value)
        } else {
            value.pointer_mut(parent)
        }
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("missing test object")?;
        object.insert(key.to_owned(), serde_json::json!(1));
        if decode(serde_json::to_vec(&value)?.as_slice()).is_ok() {
            return Err("unknown field admitted".into());
        }
    }
    for (pointer, replacement) in [
        ("/version", serde_json::json!(2)),
        ("/page_budget", serde_json::json!(0)),
        ("/limits/input_bytes", serde_json::json!(0)),
        ("/range_limits/generations", serde_json::json!(0)),
        ("/rules", serde_json::json!([])),
    ] {
        let mut value = fixture();
        *value.pointer_mut(pointer).ok_or("missing test field")? = replacement;
        if decode(serde_json::to_vec(&value)?.as_slice()).is_ok() {
            return Err("invalid configuration admitted".into());
        }
    }
    Ok(())
}

#[test]
fn registered_v2_binds_ontology_identity_and_keeps_admission_out_of_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let value = registered_fixture()?;
    let configuration = decode(serde_json::to_vec(&value)?.as_slice())?;
    if !configuration.is_registered() {
        return Err("version 2 config was not recognized as registered".into());
    }
    let identity = syntaxmesh_inference::rule_set_identity(
        &configuration.effective_rules()?,
        configuration.rule_budget,
        configuration.limits.input_bytes,
    )?;
    let mut admission_changed = value.clone();
    set(
        &mut admission_changed,
        "/registry_entries",
        serde_json::json!(32),
    )?;
    let changed_admission = decode(serde_json::to_vec(&admission_changed)?.as_slice())?;
    let same_identity = syntaxmesh_inference::rule_set_identity(
        &changed_admission.effective_rules()?,
        changed_admission.rule_budget,
        changed_admission.limits.input_bytes,
    )?;
    if identity != same_identity {
        return Err("admission bounds changed semantic rule identity".into());
    }

    let mut schema_changed = value;
    set(
        &mut schema_changed,
        "/ontology_definitions/0/relations/0/name",
        serde_json::json!("renamed"),
    )?;
    if decode(serde_json::to_vec(&schema_changed)?.as_slice()).is_ok() {
        return Err("schema relation substitution was not rejected".into());
    }
    Ok(())
}

#[test]
fn rejects_registered_fields_in_v1_and_incomplete_v2_configuration()
-> Result<(), Box<dyn std::error::Error>> {
    let mut v1 = fixture();
    set(&mut v1, "/ontology_definitions", serde_json::json!([]))?;
    set(&mut v1, "/registry_entries", serde_json::json!(1))?;
    if decode(serde_json::to_vec(&v1)?.as_slice()).is_ok() {
        return Err("version 1 accepted registered-only admission".into());
    }

    let mut v2 = registered_fixture()?;
    set(&mut v2, "/bindings", serde_json::json!(0))?;
    if decode(serde_json::to_vec(&v2)?.as_slice()).is_ok() {
        return Err("version 2 accepted a zero binding budget".into());
    }
    let mut unknown_nested = registered_fixture()?;
    let object = unknown_nested
        .pointer_mut("/ontology_definitions/0/relations/0")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("missing test relation object")?;
    object.insert("unknown".to_owned(), serde_json::json!(true));
    if decode(serde_json::to_vec(&unknown_nested)?.as_slice()).is_ok() {
        return Err("version 2 accepted an unknown nested ontology field".into());
    }
    Ok(())
}

#[test]
fn rejects_oversize_before_json_decode_and_duplicate_root_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let oversized = vec![b' '; 1_048_577];
    let error = decode(oversized.as_slice())
        .err()
        .ok_or("oversize was admitted")?;
    if !error.to_string().contains("exceeds 1 MiB") {
        return Err("oversized input was decoded before size admission".into());
    }
    let value = serde_json::to_string(&fixture())?;
    let repeated = value.replacen('{', "{\"version\":1,", 1);
    if decode(repeated.as_bytes()).is_ok() {
        return Err("duplicate version admitted".into());
    }
    Ok(())
}
