use super::*;
use syntaxmesh_language_sdk::ReferenceResolution::{Name, Unresolved};

#[test]
fn block_item_values_are_hoisted_and_do_not_leak() -> Result<(), ExtractorError> {
    let source = tests::source(
        "fn helper() {} fn caller() { helper(); { helper(); const helper: fn() = target; helper(); } helper(); { helper(); static helper: fn() = target; helper(); } helper(); }",
    );
    let extraction = RustExtractor.extract(&source)?;
    let policies: Vec<_> = extraction
        .references
        .iter()
        .map(|reference| reference.resolution)
        .collect();
    if policies
        != [
            Name, Unresolved, Unresolved, Name, Unresolved, Unresolved, Name,
        ]
    {
        return Err(ExtractorError::InvalidInput(
            "block item hoisting or restoration differs".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn value_bindings_are_scoped_and_initializers_precede_local_shadowing() -> Result<(), ExtractorError>
{
    let source = tests::source(
        "fn helper() {} fn caller(callback: fn()) { callback(); helper(); { let helper = helper(); helper(); } helper(); let closure = |helper: fn()| helper(); helper(); for helper in items { helper(); } helper(); match value { Some(helper) if helper() => helper(), _ => helper() }; if let Some(helper) = value { helper(); } else { helper(); } helper(); }",
    );
    let extraction = RustExtractor.extract(&source)?;
    let policies: Vec<_> = extraction
        .references
        .iter()
        .map(|reference| (reference.target.as_str(), reference.resolution))
        .collect();
    let expected = [
        ("callback", Unresolved),
        ("helper", Name),
        ("helper", Name),
        ("helper", Unresolved),
        ("helper", Name),
        ("helper", Unresolved),
        ("helper", Name),
        ("helper", Unresolved),
        ("helper", Name),
        ("helper", Unresolved),
        ("helper", Unresolved),
        ("helper", Name),
        ("helper", Unresolved),
        ("helper", Name),
        ("helper", Name),
    ];
    if policies != expected {
        return Err(ExtractorError::InvalidInput(format!(
            "unexpected binding constraints: {policies:?}"
        )));
    }
    Ok(())
}

#[test]
fn method_and_trait_parameters_constrain_only_bare_calls() -> Result<(), ExtractorError> {
    let source = tests::source(
        "struct Service; impl Service { fn run(helper: fn()) { helper(); Service::helper(); } fn helper() {} } trait Contract { fn run(helper: fn()) { helper(); ::helper(); } }",
    );
    let extraction = RustExtractor.extract(&source)?;
    let policies: Vec<_> = extraction
        .references
        .iter()
        .map(|reference| reference.resolution)
        .collect();
    if policies != [Unresolved, Name, Unresolved, Name] {
        return Err(ExtractorError::InvalidInput(
            "parameter constraints lost across declaration kinds".to_owned(),
        ));
    }
    Ok(())
}
