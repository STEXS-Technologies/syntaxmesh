use super::*;

use crate::test_fixture as fixture;

#[test]
fn pinned_rebuilds_reopen_and_match_query_without_regressing_latest_watermark() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("inference.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (rule, retained, current) = fixture::fixture(&mut store)?;
    fixture::verify(&store, &rule, retained, current)?;
    let limits = fixture::limits();
    let rules = [rule.clone()];
    let mut request = RebuildRequest {
        generation: retained,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let original = materializer.rebuild(&store, &request)?;
    let expected = Query::new(&store, retained).infer(&rule, &limits, 100)?;
    if materializer.claims(original, 10, 16_384)? != expected
        || materializer.rebuild(&store, &request)? != original
    {
        return Err("pinned materialization differs from fresh query".into());
    }
    let exact_bytes = usize::try_from(bincode::serialized_size(
        expected.first().ok_or("missing claim")?,
    )?)?;
    if materializer.claims(original, 1, exact_bytes)? != expected
        || materializer
            .claims(
                original,
                1,
                exact_bytes.checked_sub(1).ok_or("zero payload")?,
            )
            .is_ok()
    {
        return Err("materialized output byte boundary not enforced".into());
    }
    request.generation = current;
    request.output_bytes = 1;
    let empty = materializer.rebuild(&store, &request)?;
    if !materializer.claims(empty, 1, 1)?.is_empty() {
        return Err("retracted generation retained active derived claims".into());
    }
    request.generation = retained;
    if materializer.rebuild(&store, &request).is_ok() {
        return Err("insufficient output budget published".into());
    }
    request.output_bytes = 16_384;
    materializer.rebuild(&store, &request)?;
    let latest: String = materializer.connection.query_row(
        "SELECT operation FROM inference_watermarks",
        [],
        |row| row.get(0),
    )?;
    if latest != empty.operation.to_hex() {
        return Err("historical rebuild regressed latest watermark".into());
    }
    drop(materializer);
    let mut reopened = InferenceMaterializer::open(&path)?;
    if reopened.claims(original, 10, 16_384)? != expected
        || !reopened.claims(empty, 1, 1)?.is_empty()
    {
        return Err("reopen changed retained materializations".into());
    }
    let mut foreign = original;
    foreign.selection.generation = current;
    if reopened.claims(foreign, 10, 16_384).is_ok() {
        return Err("substituted receipt accepted".into());
    }
    reopened.connection.execute(
        "UPDATE inference_premises SET valid_from = 'tampered' WHERE operation = ?1",
        [original.operation.to_hex()],
    )?;
    if reopened.claims(original, 10, 16_384).is_ok() {
        return Err("tampered witness postings accepted".into());
    }
    if reopened.rebuild(&store, &request).is_ok() {
        return Err("retry silently accepted tampered witness postings".into());
    }
    fixture::verify_restoration(&mut store, &rule, retained, current)?;
    request.generation = store
        .current_generation(original.selection.repository, original.selection.worktree)?
        .ok_or("missing restored current generation")?
        .generation;
    let restored = reopened.rebuild(&store, &request)?;
    let fresh = Query::new(&store, request.generation).infer(&rule, &limits, 100)?;
    if restored.operation == original.operation
        || fresh.len() != 1
        || reopened.claims(restored, 10, 16_384)? != fresh
        || fresh.first().map(|claim| claim.id) == expected.first().map(|claim| claim.id)
    {
        return Err("restoration revived old operation or mismatched fresh rebuild".into());
    }
    Ok(())
}

#[test]
fn registered_rebuild_binds_schema_identity_and_rejects_invalid_facts_atomically() -> Result<()> {
    use syntaxmesh_core::NodeId;
    use syntaxmesh_inference::{
        ONTOLOGY_DEFINITION_SCHEMA_VERSION, OntologyDefinition, OntologyRegistry,
        OntologyRelationDefinition,
    };
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("registered-inference.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = syntaxmesh_store::InMemoryGraphStore::new();
    let (mut rule, retained, _) = fixture::fixture(&mut store)?;
    let source_concept = NodeId::derive(&[b"source-concept"]);
    rule.ontology_versions
        .insert("example".to_owned(), "1".to_owned());
    let external = |name: &str| OntologyRelationDefinition {
        name: name.to_owned(),
        subject_domain: Vec::new(),
        object_range: Vec::new(),
    };
    let mut writes = external("writes");
    writes.subject_domain.push(source_concept);
    writes.object_range.push(rule.concept);
    let mut instance = external("instance_of");
    instance.object_range.extend([rule.concept, source_concept]);
    let mut output = external("participates");
    output.object_range.push(rule.concept);
    let definition = OntologyDefinition {
        schema_version: ONTOLOGY_DEFINITION_SCHEMA_VERSION,
        namespace: "example".to_owned(),
        version: "1".to_owned(),
        concepts: vec![rule.concept, source_concept],
        instance_relation: "instance_of".to_owned(),
        relations: vec![writes.clone(), instance, output],
    };
    let registry =
        OntologyRegistry::from_definitions(std::slice::from_ref(&definition), 16, 16_384)?;
    let rules = [rule.clone()];
    let limits = fixture::limits();
    let request = RebuildRequest {
        generation: retained,
        rules: &rules,
        limits: &limits,
        history_scan_budget: 100,
        rule_budget: 1,
        output_bytes: 16_384,
        premise_budget: 100,
    };
    let bound = registry.bind_rules(&rules, 1, 1, limits.input_bytes)?;
    let expected = Query::new(&store, retained).infer_bound(
        &registry,
        &bound,
        &limits,
        request.history_scan_budget,
        request.rule_budget,
        1,
    )?;
    let ordinary = materializer.rebuild(&store, &request)?;
    let registered = materializer.rebuild_registered(&store, &request, &registry, 1)?;
    if expected.is_empty()
        || registered.operation == ordinary.operation
        || registered.configuration == ordinary.configuration
        || materializer.claims(registered, 10, 16_384)? != expected
    {
        return Err("registered rebuild did not preserve schema-bound query output".into());
    }

    let mut changed_definition = definition;
    let changed_writes = changed_definition
        .relations
        .iter_mut()
        .find(|relation| relation.name == "writes")
        .ok_or("missing writes relation")?;
    changed_writes.subject_domain = vec![rule.concept];
    let changed_registry = OntologyRegistry::from_definitions(&[changed_definition], 16, 16_384)?;
    let changed_bound = changed_registry.bind_rules(&rules, 1, 1, limits.input_bytes)?;
    if syntaxmesh_inference::rule_set_identity(bound.rules(), 1, limits.input_bytes)?
        == syntaxmesh_inference::rule_set_identity(changed_bound.rules(), 1, limits.input_bytes)?
    {
        return Err("same-version schema edit reused rule-set identity".into());
    }
    let operation_count: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_operations",
        [],
        |row| row.get(0),
    )?;
    if materializer
        .rebuild_registered(&store, &request, &changed_registry, 1)
        .is_ok()
    {
        return Err("ontology-invalid retained fact was materialized".into());
    }
    let after_failure: i64 = materializer.connection.query_row(
        "SELECT COUNT(*) FROM inference_operations",
        [],
        |row| row.get(0),
    )?;
    if operation_count != after_failure {
        return Err("failed ontology validation published a partial operation".into());
    }
    drop(materializer);
    let reopened = InferenceMaterializer::open(&path)?;
    if reopened.claims(registered, 10, 16_384)? != expected {
        return Err("registered bound claims changed after reopen".into());
    }
    Ok(())
}

#[test]
fn explicit_migration_is_idempotent_and_checksum_tampering_rejects() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("inference.sqlite");
    if InferenceMaterializer::open(&path).is_ok() || path.exists() {
        return Err("ordinary open created an unmigrated database".into());
    }
    let unrelated = directory.path().join("unrelated.sqlite");
    let foreign = Connection::open(&unrelated)?;
    foreign.execute_batch("CREATE TABLE application_data(value TEXT); INSERT INTO application_data VALUES ('preserve');")?;
    if InferenceMaterializer::migrate(&unrelated).is_ok() {
        return Err("unrelated database adopted as inference storage".into());
    }
    let preserved: String =
        foreign.query_row("SELECT value FROM application_data", [], |row| row.get(0))?;
    if preserved != "preserve" {
        return Err("foreign database changed".into());
    }
    InferenceMaterializer::migrate(&path)?;
    InferenceMaterializer::migrate(&path)?;
    drop(InferenceMaterializer::open(&path)?);
    let connection = Connection::open(&path)?;
    connection.execute("UPDATE inference_migrations SET checksum = 'tampered'", [])?;
    if InferenceMaterializer::open(&path).is_ok() || InferenceMaterializer::migrate(&path).is_ok() {
        return Err("tampered migration ledger accepted".into());
    }
    Ok(())
}

#[test]
fn publication_failure_rolls_back_claims_postings_and_watermark() -> Result<()> {
    use syntaxmesh_core::{
        EdgeId, FactRef, FactVersionRef, NodeId, RelationKind, RepositoryId, WorktreeId,
    };
    use syntaxmesh_inference::DerivationClass;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("inference.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let selection = ClaimSelection {
        repository: RepositoryId::derive(&[b"repo"]),
        worktree: WorktreeId::derive(&[b"worktree"]),
        generation: GenerationId::derive(&[b"generation"]),
    };
    let claim = Claim {
        id: StableId::derive("claim", &[b"1"]),
        derivation_class: DerivationClass::DeterministicRule,
        repository: selection.repository,
        worktree: selection.worktree,
        generation: selection.generation,
        source: NodeId::derive(&[b"source"]),
        target: NodeId::derive(&[b"target"]),
        relation: RelationKind::External {
            namespace: "example".to_owned(),
            relation: "derived".to_owned(),
        },
        rule_id: "example:rule".to_owned(),
        rule_version: "1".to_owned(),
        ontology_versions: std::collections::BTreeMap::new(),
        configuration_fingerprint: StableId::derive("config", &[b"1"]),
        premises: vec![FactVersionRef {
            fact: FactRef::Edge(EdgeId::derive(&[b"premise"])),
            valid_from: selection.generation,
        }],
    };
    let watermark = Watermark {
        operation: StableId::derive("operation", &[b"1"]),
        selection,
        generation_sequence: 1,
        configuration: claim.configuration_fingerprint,
        claim_count: 1,
    };
    let payloads = vec![bincode::serialize(&claim)?];
    materializer.connection.execute_batch("CREATE TRIGGER interrupt_inference BEFORE INSERT ON inference_watermarks BEGIN SELECT RAISE(ABORT, 'injected interruption'); END;")?;
    if materializer
        .publish(watermark, std::slice::from_ref(&claim), &payloads)
        .is_ok()
    {
        return Err("injected transaction failure ignored".into());
    }
    for table in [
        "inference_operations",
        "inference_claims",
        "inference_premises",
        "inference_watermarks",
        "inference_lifecycle",
        "inference_lifecycle_receipts",
    ] {
        let count: i64 = materializer.connection.query_row(
            &format!("SELECT COUNT(*) FROM {table}"),
            [],
            |row| row.get(0),
        )?;
        if count != 0 {
            return Err("partial materialization survived rollback".into());
        }
    }
    materializer
        .connection
        .execute_batch("DROP TRIGGER interrupt_inference")?;
    materializer.publish(watermark, std::slice::from_ref(&claim), &payloads)?;
    materializer.publish(watermark, std::slice::from_ref(&claim), &payloads)?;
    drop(materializer);
    let reopened = InferenceMaterializer::open(&path)?;
    let payload: Vec<u8> =
        reopened
            .connection
            .query_row("SELECT payload FROM inference_claims", [], |row| row.get(0))?;
    let decoded: Claim = bincode::deserialize(&payload)?;
    if decoded != claim {
        return Err("retained claim round trip differs".into());
    }
    Ok(())
}
