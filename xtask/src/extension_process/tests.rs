#[test]
fn thin_producer_rejects_transitive_store_and_workflow_adapters()
-> Result<(), Box<dyn std::error::Error>> {
    for dependency in [
        "syntaxmesh-engine",
        "syntaxmesh-store",
        "syntaxmesh-store-turso",
        "syntaxmesh-store-sqlite",
        "syntaxmesh-workflow",
        "syntaxmesh-integration-penelope",
        "syntaxmesh-integration-statechronicle",
        "turso",
        "rusqlite",
        "sqlite",
        "duckdb",
        "penelope",
        "statechronicle",
    ] {
        let tree = format!("syntaxmesh-extension-sdk v0.0.0\n{dependency} v1.0.0\nserde v1.0.0\n");
        if super::verify_thin_dependencies(&tree).is_ok() {
            return Err(format!("accepted forbidden producer dependency {dependency}").into());
        }
    }
    super::verify_thin_dependencies(
        "fixture v0.0.0\nsyntaxmesh-core v0.0.0\nsyntaxmesh-runtime-protocol v0.0.0\nsyntaxmesh-extension-sdk v0.0.0\nsyntaxmesh-extension-ipc v0.0.0\nserde_json v1.0.0\n",
    )?;
    Ok(())
}
