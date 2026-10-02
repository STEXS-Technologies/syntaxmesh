use crate::{OwnerEndpoint, WriterLease};

#[test]
fn endpoint_round_trip_and_publication_require_matching_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let store = fixture.path().join("graph.db");
    let other = fixture.path().join("other.db");
    std::fs::write(&store, b"store")?;
    std::fs::write(&other, b"other")?;
    let endpoint = OwnerEndpoint::new(&store, "127.0.0.1:7331".parse()?)?;
    let second = OwnerEndpoint::new(&store, "[::1]:7331".parse()?)?;
    if endpoint.instance() == second.instance() || OwnerEndpoint::discover(&store)?.is_some() {
        return Err("instance reused or absent owner discovered".into());
    }
    let decoded = OwnerEndpoint::decode(&endpoint.encode()?, &store)?;
    if decoded.instance() != endpoint.instance()
        || decoded.address() != endpoint.address()
        || decoded.store() != store.canonicalize()?
    {
        return Err("endpoint round trip changed identity".into());
    }
    let foreign = WriterLease::acquire(&other)?;
    if endpoint.publish(&foreign).is_ok() {
        return Err("endpoint published through foreign lease".into());
    }
    let lease = WriterLease::acquire(&store)?;
    endpoint.publish(&lease)?;
    let discovered = OwnerEndpoint::discover(&store)?.ok_or("active endpoint missing")?;
    if discovered.instance() != endpoint.instance() {
        return Err("wrong active instance discovered".into());
    }
    drop(lease);
    if OwnerEndpoint::discover(&store)?.is_some() {
        return Err("stale endpoint accepted after release".into());
    }
    Ok(())
}

#[test]
fn untrusted_records_reject_foreign_scope_addresses_schema_and_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let store = fixture.path().join("graph.db");
    std::fs::write(&store, b"store")?;
    let endpoint = OwnerEndpoint::new(&store, "127.0.0.1:7331".parse()?)?;
    let valid: serde_json::Value = serde_json::from_slice(&endpoint.encode()?)?;
    for (key, value) in [
        ("schema_version", serde_json::json!(2)),
        (
            "store",
            serde_json::json!(fixture.path().join("foreign.db")),
        ),
        ("address", serde_json::json!("0.0.0.0:7331")),
        ("address", serde_json::json!("192.0.2.1:7331")),
        ("address", serde_json::json!("127.0.0.1:0")),
        ("address", serde_json::json!("localhost:7331")),
        ("instance", serde_json::json!("short")),
        ("instance", serde_json::json!("F".repeat(64))),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid
            .as_object_mut()
            .ok_or("record is not an object")?
            .insert(key.to_owned(), value);
        if OwnerEndpoint::decode(&serde_json::to_vec(&invalid)?, &store).is_ok() {
            return Err(format!("invalid endpoint field accepted: {key}").into());
        }
    }
    for invalid in [
        b"{}".as_slice(),
        b"{\"schema_version\":1,\"schema_version\":1}".as_slice(),
    ] {
        if OwnerEndpoint::decode(invalid, &store).is_ok() {
            return Err("missing or duplicate endpoint fields accepted".into());
        }
    }
    if OwnerEndpoint::decode(&vec![b' '; 65537], &store).is_ok() {
        return Err("oversized record accepted".into());
    }
    for address in ["0.0.0.0:7331", "127.0.0.1:0"] {
        if OwnerEndpoint::new(&store, address.parse()?).is_ok() {
            return Err("invalid bound address accepted".into());
        }
    }
    Ok(())
}
