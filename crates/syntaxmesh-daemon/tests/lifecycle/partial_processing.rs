use super::*;

#[test]
fn verified_partial_watch_repairs_without_restart_and_preserves_history()
-> Result<(), Box<dyn Error>> {
    for polling in [true, false] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("good.rs"), "pub fn partial_healthy() {}")?;
        let failed = root.join("bad.rs");
        std::fs::write(&failed, "pub fn partial_repaired( {")?;
        let database = fixture.path().join("graph.db");
        TursoGraphStore::migrate(&database)?;
        let (_process, base) = start_with_extra_options(
            &root,
            &database,
            polling,
            false,
            true,
            Some("5000"),
            &["--record-syntax-failures"],
        )?;
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let before = search(&client, &base, "partial_healthy", None)?;
        let retained = generation(&before)?.to_owned();
        if !has_nodes(&before) || has_nodes(&search(&client, &base, "partial_repaired", None)?) {
            return Err("partial watch misclassified initial source extraction".into());
        }
        std::fs::write(&failed, "pub fn partial_repaired() {}")?;
        let started = Instant::now();
        loop {
            let repaired = search(&client, &base, "partial_repaired", None)?;
            if has_nodes(&repaired) && generation(&repaired)? != retained {
                break;
            }
            if started.elapsed() > Duration::from_secs(3) {
                return Err("running partial daemon did not publish source repair".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !has_nodes(&search(&client, &base, "partial_healthy", None)?)
            || !has_nodes(&search(&client, &base, "partial_healthy", Some(&retained))?)
            || has_nodes(&search(
                &client,
                &base,
                "partial_repaired",
                Some(&retained),
            )?)
        {
            return Err("live repair changed retained partial graph or lost healthy source".into());
        }
    }
    Ok(())
}
