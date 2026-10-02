use super::*;
use syntaxmesh_extension_ipc::{FrameCodec, MAX_FRAME_BYTES};

#[test]
fn authorized_extension_import_publishes_and_rejects_trailing_frames_on_both_hosts()
-> Result<(), Box<dyn Error>> {
    for (turso, verify) in [(false, false), (true, false), (false, true), (true, true)] {
        let fixture = FixtureDirectory::new()?;
        let store = fixture
            .path
            .join(if turso { "graph.db" } else { "graph.snapshot" });
        if turso {
            TursoGraphStore::migrate(&store)?;
        }
        let mut index_command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
        index_command
            .arg(if turso { "index-turso" } else { "index" })
            .arg(&fixture.path)
            .arg(&store);
        if verify {
            index_command.arg("--verify");
        }
        let index = index_command.output()?;
        if !index.status.success() {
            return Err(format!(
                "fixture indexing failed: {}",
                String::from_utf8_lossy(&index.stderr)
            )
            .into());
        }
        let manifest = ExtensionManifest {
            schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
            namespace: "fixture.external".to_owned(),
            producer_version: "1".to_owned(),
            capabilities: vec![Capability::RuntimeObservations],
        };
        let batch = FactBatch {
            manifest: manifest.clone(),
            provenance: vec![],
            nodes: vec![],
            edges: vec![],
            observations: vec![Observation {
                schema_version: OBSERVATION_SCHEMA_VERSION,
                producer_namespace: manifest.namespace.clone(),
                producer_version: manifest.producer_version.clone(),
                origin: ObservationOrigin::ClientProbe,
                observed_at_unix_nanos: 99,
                subject: "inventory".to_owned(),
                relation: "state".to_owned(),
                object: "available".to_owned(),
                correlation_id: None,
            }],
        };
        let grant = fixture.path.join("grant.json");
        let frame = fixture.path.join("batch.frame");
        fs::write(&grant, serde_json::to_vec(&manifest)?)?;
        let mut wire = Vec::new();
        FrameCodec::new(MAX_FRAME_BYTES)?.write_batch(&mut wire, &batch)?;
        fs::write(&frame, &wire)?;
        let generation = GenerationId::derive(&[b"cli-extension-generation"])
            .0
            .to_hex();
        let run = IndexRunId::derive(&[b"cli-extension-run"]).0.to_hex();
        let command = if turso {
            "ingest-extension-turso"
        } else {
            "ingest-extension"
        };
        let invoke = |store_path: &Path| {
            let mut import = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
            import
                .arg(command)
                .arg(store_path)
                .arg(&grant)
                .arg(&frame)
                .arg(&run)
                .arg(&generation);
            if verify {
                import.arg("--verify");
            }
            import.output()
        };
        let mut sidecar_name = store
            .file_name()
            .ok_or("fixture store filename is missing")?
            .to_os_string();
        sidecar_name.push(".syntaxmesh-owner.lock");
        let owner = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(store.with_file_name(sidecar_name))?;
        owner.try_lock()?;
        let blocked = invoke(&store)?;
        if blocked.status.success()
            || !blocked.stdout.is_empty()
            || !String::from_utf8_lossy(&blocked.stderr).contains("already owned")
        {
            return Err("extension import bypassed index ownership".into());
        }
        drop(owner);
        let accepted = invoke(&store)?;
        if !accepted.status.success() {
            return Err(format!(
                "extension import failed: {}",
                String::from_utf8_lossy(&accepted.stderr)
            )
            .into());
        }
        let receipt: serde_json::Value = serde_json::from_slice(&accepted.stdout)?;
        if receipt
            .get("generation")
            .and_then(serde_json::Value::as_str)
            != Some(generation.as_str())
            || receipt
                .get("verification")
                .and_then(serde_json::Value::as_str)
                != Some(if verify { "Verified" } else { "Durable" })
            || receipt
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
                != Some(1)
        {
            return Err("publication receipt is incomplete".into());
        }
        let export = || {
            Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(if turso { "graph-at-turso" } else { "graph-at" })
                .arg(&store)
                .arg(&generation)
                .output()
        };
        let before = export()?;
        if verify {
            let audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(if turso {
                    "statechronicle-verify-turso"
                } else {
                    "statechronicle-verify"
                })
                .arg(&store)
                .output()?;
            if !audit.status.success()
                || !String::from_utf8_lossy(&audit.stdout)
                    .contains("statechronicle_history=verified")
            {
                return Err(
                    "verified extension import did not extend an auditable history chain".into(),
                );
            }
        }
        if !before.status.success()
            || !String::from_utf8_lossy(&before.stdout).contains("RuntimeObservation")
        {
            return Err("imported runtime observation was not queryable after CLI exit".into());
        }
        // A second frame must fail before either store or workflow is opened.
        let mut multiple = wire.clone();
        multiple.extend_from_slice(&wire);
        fs::write(&frame, multiple)?;
        let rejected = invoke(&store)?;
        if rejected.status.success()
            || !rejected.stdout.is_empty()
            || !String::from_utf8_lossy(&rejected.stderr).contains("exactly one frame")
        {
            return Err("trailing extension frame was accepted or acknowledged".into());
        }
        let after = export()?;
        if !after.status.success() || after.stdout != before.stdout {
            return Err("failed import changed historical graph".into());
        }
        fs::write(&frame, &wire)?;
        let mut wrong_grant = manifest;
        wrong_grant.namespace = "fixture.other".to_owned();
        fs::write(&grant, serde_json::to_vec(&wrong_grant)?)?;
        let absent_store = fixture.path.join("must-not-create");
        let unauthorized = invoke(&absent_store)?;
        if unauthorized.status.success()
            || absent_store.exists()
            || !String::from_utf8_lossy(&unauthorized.stderr).contains("host-authorized")
        {
            return Err("unauthorized import was not rejected before opening store".into());
        }
        for oversized_grant in [true, false] {
            if oversized_grant {
                fs::write(&grant, vec![b' '; 64 * 1024 + 1])?;
            } else {
                fs::write(&grant, serde_json::to_vec(&batch.manifest)?)?;
                fs::write(&frame, vec![0; MAX_FRAME_BYTES + 13])?;
            }
            let oversized = invoke(&absent_store)?;
            if oversized.status.success()
                || absent_store.exists()
                || !String::from_utf8_lossy(&oversized.stderr).contains("exceeds")
            {
                return Err(
                    "oversized extension input was not rejected before opening store".into(),
                );
            }
        }
        if !verify {
            fs::write(&grant, serde_json::to_vec(&batch.manifest)?)?;
            fs::write(&frame, &wire)?;
            let failed_generation = GenerationId::derive(&[b"cli-extension-verification-failed"])
                .0
                .to_hex();
            let failed_run = IndexRunId::derive(&[b"cli-extension-verification-failed-run"])
                .0
                .to_hex();
            let attempted = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(command)
                .arg(&store)
                .arg(&grant)
                .arg(&frame)
                .arg(&failed_run)
                .arg(&failed_generation)
                .arg("--verify")
                .output()?;
            if attempted.status.success()
                || !attempted.stdout.is_empty()
                || !String::from_utf8_lossy(&attempted.stderr)
                    .contains("does not extend the verified history head")
            {
                return Err("unverified-base import did not report verification failure".into());
            }
            let accepted_despite_verification = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(if turso { "graph-at-turso" } else { "graph-at" })
                .arg(&store)
                .arg(&failed_generation)
                .output()?;
            if !accepted_despite_verification.status.success() {
                return Err("verification failure incorrectly erased accepted generation".into());
            }
        }
    }
    Ok(())
}
