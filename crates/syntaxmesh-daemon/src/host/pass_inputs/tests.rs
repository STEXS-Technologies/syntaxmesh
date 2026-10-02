use std::cell::Cell;

#[test]
fn policy_read_follows_source_capture_even_when_operator_updates_both() -> Result<(), String> {
    let verified = Cell::new(false);
    let (source_revision, selected_policy) = super::capture(
        || {
            // Force the operator's config-on then source-edit sequence during
            // capture. Reading policy first would select false for revision 2.
            verified.set(true);
            Ok(2_u64)
        },
        || Ok(verified.get()),
    )?;
    if source_revision != 2 || !selected_policy {
        return Err("new source inventory selected a policy read before capture".to_owned());
    }
    Ok(())
}

#[test]
fn failed_capture_does_not_load_policy() -> Result<(), String> {
    let loaded = Cell::new(false);
    let result = super::capture(
        || Err::<(), _>("source scan failed".to_owned()),
        || {
            loaded.set(true);
            Ok(())
        },
    );
    if result != Err("source scan failed".to_owned()) || loaded.get() {
        return Err("failed source capture loaded policy or lost its error".to_owned());
    }
    Ok(())
}
