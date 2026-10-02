//! Focused real-source diagnosis, not a candidate-policy or quality benchmark.

#[test]
#[ignore = "requires SYNTAXMESH_CONTEXT_EVAL_ROOT pointing at Sim"]
fn typescript_helper_route_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::var("SYNTAXMESH_CONTEXT_EVAL_ROOT")?);
    let (_fixture, server, generation, fingerprint, files) = super::indexed_external_source_server(
        &root,
        &["scripts/generate-docs.ts"],
        false,
        syntaxmesh_engine::SourceSyntaxPolicy::Strict,
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(server.with_query(move |service| {
        let find = |name: &str| {
            let nodes = service.historical_search(name, 256).map_err(|error| error.to_string())?;
            let functions = nodes.into_iter().filter(|node| node.name == name && node.kind == syntaxmesh_core::NodeKind::Function).collect::<Vec<_>>();
            match functions.as_slice() {
                [node] => Ok(node.id),
                _ => Err(format!("expected one canonical function for {name}")),
            }
        };
        let helper = find("generateMarkdownForBlock")?;
        let target = find("generateBlockDoc")?;
        let mut reports = Vec::new();
        for max_nodes in [64, 256] {
            let neighborhood = service.historical_neighborhood(&[helper], 2, max_nodes, 1024, 4096, 4 * 1024 * 1024).map_err(|error| error.to_string())?;
            reports.push(serde_json::json!({
                "max_nodes":max_nodes, "nodes":neighborhood.nodes.len(),
                "target_depth":neighborhood.nodes.iter().find(|node| node.id == target).map(|node| node.depth),
                "truncated":neighborhood.truncated,
                "direct_relations":neighborhood.edges.iter().map(|entry| &entry.edge).filter(|edge| (edge.source == helper && edge.target == target) || (edge.source == target && edge.target == helper)).map(|edge| &edge.relation).collect::<Vec<_>>()
            }));
        }
        Ok(serde_json::json!({"generation":generation,"fingerprint":fingerprint,"files":files,"helper":helper,"target":target,"reports":reports}))
    }))?;
    eprintln!("typescript_helper_route result={report}");
    Ok(())
}
