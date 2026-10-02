#[test]
fn content_selection_is_explicit_and_duplicate_flags_reject() -> Result<(), String> {
    let base = ["graph.db", "source", "cl100k_base"];
    let plain = super::parse(base.map(std::ffi::OsString::from))?;
    let content = super::parse(
        base.into_iter()
            .chain(["--source-content-context"])
            .map(std::ffi::OsString::from),
    )?;
    if plain.source_content
        || !content.source_content
        || super::parse(
            base.into_iter()
                .chain(["--source-content-context", "--source-content-context"])
                .map(std::ffi::OsString::from),
        )
        .is_ok()
        || super::parse(["graph.db", "source", "invalid"].map(std::ffi::OsString::from)).is_ok()
    {
        return Err("invalid or implicit source-content selection".to_owned());
    }
    Ok(())
}

#[test]
fn discovery_flags_compose_in_either_order_without_implicit_defaults() -> Result<(), String> {
    let base = ["graph.db", "source", "cl100k_base"];
    if super::parse(base.map(std::ffi::OsString::from))?.lexical_first {
        return Err("lexical-first became implicit".to_owned());
    }
    for flags in [
        ["--lexical-first-context", "--source-content-context"],
        ["--source-content-context", "--lexical-first-context"],
    ] {
        let parsed = super::parse(base.into_iter().chain(flags).map(std::ffi::OsString::from))?;
        if !parsed.lexical_first || !parsed.source_content {
            return Err("packing and content policies did not compose".to_owned());
        }
    }
    if super::parse(
        base.into_iter()
            .chain(["--lexical-first-context", "--lexical-first-context"])
            .map(std::ffi::OsString::from),
    )
    .is_ok()
    {
        return Err("duplicate discovery flag was accepted".to_owned());
    }
    Ok(())
}
