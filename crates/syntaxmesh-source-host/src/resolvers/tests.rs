use super::*;

#[test]
fn shared_setup_preserves_opt_in_and_previous_composite_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    let disabled = project_resolvers(root, &ProjectConfig::default())?;
    if disabled.provider.is_some()
        || disabled.fingerprint != syntaxmesh_resolver::REFERENCE_RESOLVER_REVISION
    {
        return Err("resolvers were enabled without project policy".into());
    }
    std::fs::write(
        root.join("syntaxmesh.toml"),
        "[module_resolution]\nprofiles = [\"node\", \"python\"]\n",
    )?;
    let setup = project_resolvers(root, &ProjectConfig::load(root)?)?;
    let previous = CompositeModuleResolutionProvider::new(vec![
        Arc::new(OxcModuleResolver::for_node_project(
            root.to_path_buf(),
            FileSystemOs,
        )?),
        Arc::new(PythonModuleResolver::new(
            root.to_path_buf(),
            PythonFileSystemOs,
            &[],
        )?),
    ]);
    let identity = previous.identity();
    let mut expected = syntaxmesh_resolver::REFERENCE_RESOLVER_REVISION.to_vec();
    expected.extend_from_slice(
        &StableId::derive(
            "cli-module-resolver-v1",
            &[
                identity.namespace.as_bytes(),
                identity.version.as_bytes(),
                &identity.settings_fingerprint,
            ],
        )
        .0,
    );
    expected.extend_from_slice(b"syntaxmesh-ecmascript-export-binding-v2");
    if setup.fingerprint != expected
        || setup.provider.as_ref().map(|provider| provider.identity()) != Some(identity)
    {
        return Err("shared setup changed provider/cache identity".into());
    }
    Ok(())
}
