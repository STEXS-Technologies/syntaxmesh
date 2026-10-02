use super::*;

#[test]
fn constraints_round_trip_without_changing_legacy_metadata() {
    assert_eq!(ReferenceResolution::Name.payload(), None);
    assert_eq!(
        ReferenceResolution::from_payload(None),
        Ok(ReferenceResolution::Name)
    );
    let payload = ReferenceResolution::Unresolved.payload();
    assert_eq!(
        ReferenceResolution::from_payload(payload.as_ref()),
        Ok(ReferenceResolution::Unresolved)
    );
    let foreign = ExtensionPayload {
        namespace: "fixture.foreign".to_owned(),
        schema_version: 999,
        bytes: vec![],
    };
    assert_eq!(
        ReferenceResolution::from_payload(Some(&foreign)),
        Ok(ReferenceResolution::Name)
    );
}

#[test]
fn reserved_constraints_fail_closed_on_unknown_or_malformed_encoding() {
    for (schema_version, bytes) in [(2, vec![1]), (1, vec![]), (1, vec![0]), (1, vec![1, 0])] {
        let payload = ExtensionPayload {
            namespace: REFERENCE_RESOLUTION_NAMESPACE.to_owned(),
            schema_version,
            bytes,
        };
        assert_eq!(
            ReferenceResolution::from_payload(Some(&payload)),
            Err(ReferenceResolutionError)
        );
    }
}
