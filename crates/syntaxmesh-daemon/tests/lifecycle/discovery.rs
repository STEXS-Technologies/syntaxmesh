use super::*;
use syntaxmesh_ownership_host::OwnerEndpoint;

fn stop(process: &mut Process) -> Result<(), Box<dyn Error>> {
    #[cfg(unix)]
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(i32::try_from(process.0.id())?),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    wait_exit(process)
}

#[test]
fn published_endpoint_binds_each_request_and_restart_changes_instance() -> Result<(), Box<dyn Error>>
{
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("lib.rs"), "pub fn discovered_symbol() {}")?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let (mut process, base) = start(&root, &database, true)?;
    let endpoint =
        OwnerEndpoint::discover(&database)?.ok_or("ready daemon did not publish endpoint")?;
    if format!("http://{}", endpoint.address()) != base {
        return Err("published endpoint did not match listener".into());
    }
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()?;
    let response = client
        .get(format!("{base}/api/v1/search"))
        .query(&[("text", "discovered_symbol")])
        .header("x-syntaxmesh-owner-instance", endpoint.instance())
        .send()?;
    if !response.status().is_success()
        || response
            .headers()
            .get("x-syntaxmesh-owner-instance")
            .and_then(|value| value.to_str().ok())
            != Some(endpoint.instance())
        || !has_nodes(&response.json()?)
    {
        return Err("guarded discovery request did not bind positive query".into());
    }
    let duplicate = client
        .get(format!("{base}/healthz"))
        .header("x-syntaxmesh-owner-instance", endpoint.instance())
        .header("x-syntaxmesh-owner-instance", endpoint.instance())
        .send()?;
    if duplicate.status().as_u16() != 412 {
        return Err("duplicate owner guard was accepted".into());
    }
    stop(&mut process)?;
    if OwnerEndpoint::discover(&database)?.is_some() {
        return Err("stale endpoint remained discoverable".into());
    }
    let (mut restarted, restart_base) = start(&root, &database, true)?;
    let fresh = OwnerEndpoint::discover(&database)?.ok_or("restart endpoint missing")?;
    if fresh.instance() == endpoint.instance()
        || client
            .get(format!("{restart_base}/api/v1/search"))
            .query(&[("text", "discovered_symbol")])
            .header("x-syntaxmesh-owner-instance", endpoint.instance())
            .send()?
            .status()
            .as_u16()
            != 412
    {
        return Err("restart accepted a previous owner instance".into());
    }
    stop(&mut restarted)?;
    audit_history(&root, &database)?;
    Ok(())
}

#[test]
fn discovery_publication_failure_exits_without_readiness_and_releases_lease()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("lib.rs"), "pub fn failed_discovery() {}")?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    std::fs::create_dir(fixture.path().join("graph.db.syntaxmesh-owner.discovery"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"))
        .arg(&root)
        .arg(&database)
        .arg("127.0.0.1:0")
        .args(["--poll-only", "--watch-duration-ms", "100"])
        .output()?;
    if output.status.success()
        || !output.stdout.is_empty()
        || !String::from_utf8_lossy(&output.stderr)
            .contains("discovery sidecar must be a regular file")
        || syntaxmesh_ownership_host::is_writer_active(&database)?
    {
        return Err("failed discovery startup reported readiness or retained ownership".into());
    }
    Ok(())
}
