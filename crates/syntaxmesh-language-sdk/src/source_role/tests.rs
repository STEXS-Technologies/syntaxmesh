use super::*;

#[test]
fn role_codec_is_exact_and_preserves_legacy_positive_intent() -> Result<(), SourceRoleError> {
    if SourceRole::Unknown.payload().is_some()
        || SourceRole::from_payload(None)? != SourceRole::Unknown
    {
        return Err(SourceRoleError);
    }
    let canonical = SourceRole::TestIntent.payload().ok_or(SourceRoleError)?;
    if canonical.namespace != SOURCE_ROLE_NAMESPACE
        || canonical.schema_version != 1
        || canonical.bytes != [1]
        || SourceRole::from_payload(Some(&canonical))? != SourceRole::TestIntent
    {
        return Err(SourceRoleError);
    }
    for namespace in [SOURCE_ROLE_NAMESPACE, "syntaxmesh.lang.rust.test-role"] {
        let mut payload = canonical.clone();
        payload.namespace = namespace.to_owned();
        if SourceRole::from_payload(Some(&payload))? != SourceRole::TestIntent {
            return Err(SourceRoleError);
        }
        for bytes in [vec![], vec![0], vec![2], vec![1, 0]] {
            payload.bytes = bytes;
            if SourceRole::from_payload(Some(&payload)).is_ok() {
                return Err(SourceRoleError);
            }
        }
        payload.bytes = vec![1];
        payload.schema_version = 2;
        if SourceRole::from_payload(Some(&payload)).is_ok() {
            return Err(SourceRoleError);
        }
    }
    let mut unrelated = canonical;
    unrelated.namespace = "producer.other".to_owned();
    unrelated.bytes.clear();
    unrelated.schema_version = u32::MAX;
    if SourceRole::from_payload(Some(&unrelated))? != SourceRole::Unknown {
        return Err(SourceRoleError);
    }
    Ok(())
}
