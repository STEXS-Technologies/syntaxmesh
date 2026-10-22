use super::*;
use crate::{RebuildRequest, StreamSyncRequest, test_fixture};
use syntaxmesh_core::{EdgeId, ExtensionPayload, GraphDelta, IndexRunId};
use syntaxmesh_inference::InvalidationLimits;
use syntaxmesh_store::{GraphStore, InMemoryGraphStore};

#[test]
fn one_active_leaf_reads_with_one_page_after_sixty_four_closed_lifetimes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("sparse.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = InMemoryGraphStore::new();
    let (rule, declared, mut current) = test_fixture::fixture(&mut store)?;
    let mut declaration = store
        .historical_snapshot(declared)?
        .edges
        .into_iter()
        .find(|edge| edge.id == EdgeId::derive(&[b"instance"]))
        .ok_or("missing declaration")?;
    let limits = test_fixture::limits();
    let rules = [rule];
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation: current,
            rules: &rules,
            limits: &limits,
            history_scan_budget: 4096,
            rule_budget: 1,
            output_bytes: 16_384,
            premise_budget: 100,
        },
    )?;
    let origin = materializer.begin_stream(baseline, 10, 16_384, 100, 4096)?;
    let mut first_birth = None;
    for index in 0..65 {
        let manifest = store.manifest(current)?;
        let next = GenerationId::derive(&[b"sparse-generation", index.to_string().as_bytes()]);
        declaration.extension_payload = Some(ExtensionPayload {
            namespace: "test.inference".to_owned(),
            schema_version: 1,
            bytes: index.to_string().into_bytes(),
        });
        store.apply_delta(GraphDelta {
            repository: manifest.repository,
            worktree: manifest.worktree,
            run_id: IndexRunId::derive(&[b"sparse-run", index.to_string().as_bytes()]),
            expected_base: Some(current),
            next_generation: next,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: vec![declaration.clone()],
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;
        materializer.sync_stream(
            &store,
            &StreamSyncRequest {
                origin,
                through: next,
                rules: &rules,
                limits: &limits,
                range_limits: &InvalidationLimits {
                    generations: 1,
                    versions: 100,
                    postings: 100,
                    claims: 10,
                },
                history_scan_budget: 4096,
                rule_budget: 1,
                output_bytes: 16_384,
                premise_budget: 100,
                evidence_budget: 100,
                matched_claim_bytes: 16_384,
                page_budget: 4096,
            },
        )?;
        if first_birth.is_none() {
            first_birth = Some(next);
        }
        current = next;
    }
    let closed: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_stream_claims WHERE closed_sequence IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    if closed != 64 {
        return Err("sparse fixture did not retain sixty-four closed lifetimes".into());
    }
    let selected = materializer.stream_claims(origin, current, 1, 16_384, 3, 1)?;
    let identity = selected.claims.first().ok_or("missing active claim")?.id;
    let oldest = first_birth.ok_or("missing oldest checkpoint")?;
    let old_claim = materializer
        .stream_claims(origin, oldest, 1, 16_384, 3, 1)?
        .claims
        .first()
        .ok_or("missing oldest claim")?
        .id;
    if materializer.stream_claim_active(origin, current, identity, 1)?
        != (selected.checkpoint, true)
        || materializer
            .stream_claim_active(origin, current, old_claim, 1)?
            .1
        || !materializer
            .stream_claim_active(origin, oldest, old_claim, 1)?
            .1
        || materializer
            .stream_claim_active(origin, current, identity, 0)
            .is_ok()
        || materializer
            .stream_claim_active(origin, GenerationId::derive(&[b"unknown"]), identity, 1)
            .is_ok()
    {
        return Err("retained point membership differs from witness oracle".into());
    }
    if selected.claims.len() != 1
        || selected
            .claims
            .first()
            .is_none_or(|claim| claim.generation != current)
    {
        return Err("single-page active root did not return the selected witness".into());
    }
    let mut plan = materializer
        .connection
        .prepare(&format!("EXPLAIN QUERY PLAN {CLAIMS_SQL}"))?;
    let point = materializer.stream_claim(origin, current, identity, 16_384, 3, 1)?;
    let page_request = StreamPageRequest {
        origin,
        generation: current,
        cursor: None,
        claim_budget: 1,
        byte_budget: 16_384,
        premise_budget: 3,
        page_budget: 1,
    };
    let page = materializer.stream_claims_page(&page_request)?;
    if page.claims != selected.claims
        || page.checkpoint != selected.checkpoint
        || page.next.is_some()
    {
        return Err("sparse single-page read differs from retained witness".into());
    }
    let payload_bytes = usize::try_from(bincode::serialized_size(
        point.claim.as_ref().ok_or("missing point witness")?,
    )?)?;
    if point.checkpoint != selected.checkpoint
        || point.claim.as_ref() != selected.claims.first()
        || materializer
            .stream_claim(origin, current, old_claim, 1, 1, 1)?
            .claim
            .is_some()
        || materializer
            .stream_claim(
                origin,
                current,
                identity,
                payload_bytes.checked_sub(1).ok_or("empty payload")?,
                3,
                1,
            )
            .is_ok()
        || materializer
            .stream_claim(origin, current, identity, payload_bytes, 2, 1)
            .is_ok()
        || materializer
            .stream_claim(origin, current, identity, payload_bytes, 3, 0)
            .is_ok()
        || materializer
            .stream_claim(origin, current, identity, payload_bytes, 0, 1)
            .is_ok()
        || materializer
            .stream_claim(origin, current, identity, 0, 3, 1)
            .is_ok()
        || materializer.stream_claim(origin, current, identity, payload_bytes, 3, 1)? != point
    {
        return Err("point payload differs from enumeration or bypasses bounds".into());
    }
    let details = plan
        .query_map(
            params![
                origin.stream.to_hex(),
                selected
                    .claims
                    .first()
                    .ok_or("missing active witness")?
                    .id
                    .to_hex()
            ],
            |row| row.get::<_, String>(3),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !details
        .iter()
        .any(|detail| detail.contains("SEARCH") && detail.contains("stream=? AND claim=?"))
    {
        return Err("root reader did not seek exact stream/claim identity".into());
    }
    drop(plan);
    materializer.connection.execute(
        "UPDATE inference_stream_claims SET payload = X'00' WHERE closed_sequence IS NOT NULL",
        [],
    )?;
    if materializer.stream_claims(origin, current, 1, 16_384, 3, 1)? != selected
        || materializer
            .stream_claims(
                origin,
                first_birth.ok_or("missing oldest root")?,
                1,
                16_384,
                3,
                1,
            )
            .is_ok()
    {
        return Err(
            "active root scanned closed payloads or historical root hid corrupted selected payload"
                .into(),
        );
    }
    drop(store);
    if materializer.stream_claim(origin, current, identity, 16_384, 3, 1)? != point
        || materializer
            .stream_claim(origin, oldest, old_claim, 16_384, 3, 1)
            .is_ok()
    {
        return Err("point lookup scanned closed payloads or ignored selected corruption".into());
    }
    drop(materializer);
    let reopened = InferenceMaterializer::open(&path)?;
    if reopened.stream_claims_page(&page_request)? != page {
        return Err("sparse paged read required closed payloads or replay".into());
    }
    if reopened.stream_claim(origin, current, identity, 16_384, 3, 1)? != point {
        return Err("point payload required replay after reopen".into());
    }
    if reopened.stream_claim_active(origin, current, identity, 1)? != (selected.checkpoint, true)
        || reopened
            .stream_claim_active(origin, current, old_claim, 1)?
            .1
    {
        return Err("point membership required canonical replay or closed payloads".into());
    }
    if reopened.stream_claims(origin, current, 1, 16_384, 3, 1)? != selected {
        return Err("sparse active root required replay after reopen".into());
    }
    Ok(())
}
