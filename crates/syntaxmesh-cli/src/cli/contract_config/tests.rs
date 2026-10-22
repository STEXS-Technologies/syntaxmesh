use super::*;
use syntaxmesh_core::{NodeId, RelationKind};
use syntaxmesh_inference::{ContractOrigin, RelationExpectation};

fn configuration() -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "contracts": [{
            "id": "entry-calls-writer",
            "version": "1",
            "origin": "declared",
            "source": "0000000000000000000000000000000000000000000000000000000000000000",
            "target": "1111111111111111111111111111111111111111111111111111111111111111",
            "relation": {"kind": "calls"},
            "expectation": "required"
        }],
        "limits": {
            "contracts": 4,
            "input_bytes": 16384,
            "edge_visits": 100,
            "results": 4
        },
        "history_scan_budget": 100,
        "output_bytes": 16384
    })
}

fn set_field(
    root: &mut serde_json::Value,
    field: &str,
    value: serde_json::Value,
) -> Result<(), CliError> {
    root.as_object_mut()
        .ok_or_else(|| invalid("test configuration is not an object"))?
        .insert(field.to_owned(), value);
    Ok(())
}

fn set_nested_field(
    root: &mut serde_json::Value,
    parent: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), CliError> {
    root.get_mut(parent)
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| invalid("test configuration parent is not an object"))?
        .insert(field.to_owned(), value);
    Ok(())
}

fn set_first_contract_field(
    root: &mut serde_json::Value,
    field: &str,
    value: serde_json::Value,
) -> Result<(), CliError> {
    root.get_mut("contracts")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|contracts| contracts.first_mut())
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| invalid("test configuration has no contract object"))?
        .insert(field.to_owned(), value);
    Ok(())
}

fn load_value(value: &serde_json::Value) -> Result<Configuration, CliError> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("contracts.json");
    std::fs::write(&path, serde_json::to_vec(value).map_err(CliError::Encode)?)?;
    load(&path)
}

#[test]
fn versioned_contract_file_maps_hex_ids_and_typed_relation() -> Result<(), CliError> {
    let parsed = load_value(&configuration())?;
    let contracts = parsed.contracts()?;
    let contract = contracts
        .first()
        .ok_or_else(|| invalid("test config lost its contract"))?;
    if contract.id != "entry-calls-writer"
        || contract.origin != ContractOrigin::Declared
        || contract.source != NodeId(syntaxmesh_core::StableId([0; 32]))
        || contract.target.0.0 != [0x11; 32]
        || contract.relation != RelationKind::Calls
        || contract.expectation != RelationExpectation::Required
        || parsed.limits().edge_visits != 100
    {
        return Err(invalid(
            "contract config changed identity, relation, or limits",
        ));
    }
    Ok(())
}

#[test]
fn invalid_versions_unknown_fields_and_exhausted_bounds_reject() -> Result<(), CliError> {
    let mut unsupported = configuration();
    set_field(&mut unsupported, "version", serde_json::json!(2))?;
    if load_value(&unsupported).is_ok() {
        return Err(invalid("unsupported contract config version was accepted"));
    }

    let mut unknown = configuration();
    set_field(&mut unknown, "unexpected", serde_json::json!(true))?;
    if load_value(&unknown).is_ok() {
        return Err(invalid(
            "unknown top-level contract config field was accepted",
        ));
    }

    let mut exhausted = configuration();
    set_nested_field(
        &mut exhausted,
        "limits",
        "edge_visits",
        serde_json::json!(0),
    )?;
    if load_value(&exhausted).is_ok() {
        return Err(invalid("zero contract work budget was accepted"));
    }

    let mut empty = configuration();
    set_field(&mut empty, "contracts", serde_json::json!([]))?;
    if load_value(&empty).is_ok() {
        return Err(invalid("empty authored contract set was accepted"));
    }
    Ok(())
}

#[test]
fn invalid_node_identity_rejects_before_store_access() -> Result<(), CliError> {
    let mut malformed = configuration();
    set_first_contract_field(&mut malformed, "source", serde_json::json!("not-a-node-id"))?;
    if load_value(&malformed)?.contracts().is_ok() {
        return Err(invalid("malformed contract node identity was accepted"));
    }
    Ok(())
}
