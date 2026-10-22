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
    unsupported["version"] = serde_json::json!(2);
    if load_value(&unsupported).is_ok() {
        return Err(invalid("unsupported contract config version was accepted"));
    }

    let mut unknown = configuration();
    unknown["unexpected"] = serde_json::json!(true);
    if load_value(&unknown).is_ok() {
        return Err(invalid(
            "unknown top-level contract config field was accepted",
        ));
    }

    let mut exhausted = configuration();
    exhausted["limits"]["edge_visits"] = serde_json::json!(0);
    if load_value(&exhausted).is_ok() {
        return Err(invalid("zero contract work budget was accepted"));
    }

    let mut empty = configuration();
    empty["contracts"] = serde_json::json!([]);
    if load_value(&empty).is_ok() {
        return Err(invalid("empty authored contract set was accepted"));
    }
    Ok(())
}

#[test]
fn invalid_node_identity_rejects_before_store_access() -> Result<(), CliError> {
    let mut malformed = configuration();
    malformed["contracts"][0]["source"] = serde_json::json!("not-a-node-id");
    if load_value(&malformed)?.contracts().is_ok() {
        return Err(invalid("malformed contract node identity was accepted"));
    }
    Ok(())
}
