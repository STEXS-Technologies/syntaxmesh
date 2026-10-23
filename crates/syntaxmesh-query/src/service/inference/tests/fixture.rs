use super::*;
use std::collections::BTreeMap;
use std::error::Error;
use std::result::Result;
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, GenerationId, GraphDelta, IndexRunId, Node, NodeId, NodeKind,
    Provenance, ProvenanceId, RelationKind, RepositoryId, StableId, WorktreeId,
};

pub(super) fn fixture<S: GraphStore>(
    store: &mut S,
) -> Result<(Rule, GenerationId, GenerationId), Box<dyn Error>> {
    let [entry, writer, target, concept, source_concept] =
        ["entry", "writer", "target", "concept", "source-concept"]
            .map(|name| NodeId::derive(&[name.as_bytes()]));
    let first = GenerationId::derive(&[b"first"]);
    let declared = GenerationId::derive(&[b"declared"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"declaration"]),
        producer_namespace: "test.inference".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let external = |relation: &str| RelationKind::External {
        namespace: "example".to_owned(),
        relation: relation.to_owned(),
    };
    let edge = |name: &[u8], source, destination, relation| Edge {
        id: EdgeId::derive(&[name]),
        source,
        target: destination,
        relation,
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut delta = GraphDelta {
        repository: RepositoryId::derive(&[b"inference-repo"]),
        worktree: WorktreeId::derive(&[b"inference-worktree"]),
        run_id: IndexRunId::derive(&[b"first-run"]),
        expected_base: None,
        next_generation: first,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance.clone()],
        upsert_nodes: [entry, writer, target, concept, source_concept]
            .into_iter()
            .map(|id| Node {
                id,
                kind: NodeKind::Function,
                name: id.0.to_hex(),
                owner_file: None,
                source: None,
                provenance: provenance.id,
                extension_payload: None,
            })
            .collect(),
        upsert_edges: vec![edge(b"calls", entry, writer, RelationKind::Calls)],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    store.apply_delta(delta.clone())?;
    delta.expected_base = Some(first);
    delta.next_generation = declared;
    delta.run_id = IndexRunId::derive(&[b"declaration-run"]);
    delta.upsert_nodes.clear();
    delta.upsert_provenance.clear();
    delta.upsert_edges = vec![
        edge(b"writes", writer, target, external("writes")),
        edge(b"instance", target, concept, external("instance_of")),
        edge(
            b"writer-instance",
            writer,
            source_concept,
            external("instance_of"),
        ),
    ];
    store.apply_delta(delta.clone())?;
    let rule = Rule {
        id: "example:write_path".to_owned(),
        version: "1".to_owned(),
        ontology_versions: BTreeMap::new(),
        configuration_fingerprint: StableId::derive("config", &[b"1"]),
        path_relation: RelationKind::Calls,
        join_relation: external("writes"),
        declaration_relation: external("instance_of"),
        concept,
        output_relation: external("participates"),
    };
    let before = Query::new(store, declared).infer(&rule, &limits(), 100)?;
    let range = syntaxmesh_inference::DiscoveryRange {
        from: first,
        generations: 1,
        versions: 100,
    };
    if Query::new(store, declared).infer_discovered_rules(
        &range,
        std::slice::from_ref(&rule),
        &limits(),
        100,
        1,
    )? != before
    {
        return Err("canonical openings did not discover the enabled witness".into());
    }
    let claim = before.first().ok_or("missing declared inference")?;
    if before.len() != 1
        || claim
            .premises
            .iter()
            .filter(|reference| reference.valid_from == first)
            .count()
            != 1
        || claim
            .premises
            .iter()
            .filter(|reference| reference.valid_from == declared)
            .count()
            != 2
    {
        return Err("inference assigned fabricated premise versions".into());
    }
    let retracted = GenerationId::derive(&[b"retracted"]);
    delta.expected_base = Some(declared);
    delta.next_generation = retracted;
    delta.run_id = IndexRunId::derive(&[b"retraction-run"]);
    delta.upsert_edges.clear();
    delta.remove_edges = vec![EdgeId::derive(&[b"instance"])];
    store.apply_delta(delta)?;
    let retraction_range = syntaxmesh_inference::DiscoveryRange {
        from: first,
        generations: 2,
        versions: 100,
    };
    if !Query::new(store, retracted)
        .infer_discovered_rules(
            &retraction_range,
            std::slice::from_ref(&rule),
            &limits(),
            100,
            1,
        )?
        .is_empty()
    {
        return Err("range discovery retained a subsequently closed premise".into());
    }
    Ok((rule, declared, retracted))
}

pub(super) const fn limits() -> Limits {
    Limits {
        input_bytes: 16_384,
        facts: 20,
        edge_visits: 100,
        depth: 2,
        claims: 10,
    }
}

pub(super) fn verify_restoration<S: GraphStore>(
    store: &mut S,
    rule: &Rule,
    retained: GenerationId,
    current: GenerationId,
) -> Result<(), Box<dyn Error>> {
    let original = Query::new(store, retained).infer(rule, &limits(), 100)?;
    let dependencies = syntaxmesh_inference::PremiseDependencyIndex::build(&original, 10, 100)?;
    let snapshot = store.historical_snapshot(retained)?;
    let declaration = snapshot
        .edges
        .into_iter()
        .find(|edge| edge.relation == rule.declaration_relation)
        .ok_or("missing declaration")?;
    let manifest = store.manifest(current)?;
    let restored = GenerationId::derive(&[b"restored-declaration"]);
    store.apply_delta(GraphDelta {
        repository: manifest.repository,
        worktree: manifest.worktree,
        run_id: IndexRunId::derive(&[b"restore-run"]),
        expected_base: Some(current),
        next_generation: restored,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: vec![declaration.clone()],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let fresh = Query::new(store, restored).infer(rule, &limits(), 100)?;
    let new_version = syntaxmesh_core::FactVersionRef {
        fact: syntaxmesh_core::FactRef::Edge(declaration.id),
        valid_from: restored,
    };
    let changed = std::collections::BTreeSet::from([new_version]);
    let mut invalidation_limits = syntaxmesh_inference::InvalidationLimits {
        generations: 2,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let restored_query = Query::new(store, restored);
    let range = syntaxmesh_inference::DiscoveryRange {
        from: retained,
        generations: 2,
        versions: 100,
    };
    if restored_query.infer_discovered_rules(
        &range,
        std::slice::from_ref(rule),
        &limits(),
        100,
        1,
    )? != fresh
    {
        return Err("restored exact premise did not discover a fresh witness".into());
    }
    for bounded in [
        syntaxmesh_inference::DiscoveryRange {
            generations: 0,
            ..range
        },
        syntaxmesh_inference::DiscoveryRange {
            versions: 0,
            ..range
        },
        syntaxmesh_inference::DiscoveryRange {
            generations: 1,
            ..range
        },
        syntaxmesh_inference::DiscoveryRange {
            versions: 1,
            ..range
        },
    ] {
        if restored_query
            .infer_discovered_rules(&bounded, std::slice::from_ref(rule), &limits(), 100, 1)
            .is_ok()
        {
            return Err("range discovery truncated exhausted canonical feed bounds".into());
        }
    }
    let same = syntaxmesh_inference::DiscoveryRange {
        from: restored,
        ..range
    };
    if !restored_query
        .infer_discovered_rules(&same, std::slice::from_ref(rule), &limits(), 100, 1)?
        .is_empty()
        || Query::new(store, retained)
            .infer_discovered_rules(&same, std::slice::from_ref(rule), &limits(), 100, 1)
            .is_ok()
    {
        return Err("range discovery confused same-generation or reversed ranges".into());
    }
    if restored_query.inference_invalidations(&dependencies, retained, &invalidation_limits)?
        != original
            .iter()
            .map(|original_claim| original_claim.id)
            .collect()
    {
        return Err("restoration incorrectly revived an old invalidated witness".into());
    }
    invalidation_limits.generations = 1;
    if restored_query
        .inference_invalidations(&dependencies, retained, &invalidation_limits)
        .is_ok()
    {
        return Err("invalidation range silently truncated an exhausted generation budget".into());
    }
    invalidation_limits.generations = 2;
    invalidation_limits.versions = 1;
    if restored_query
        .inference_invalidations(&dependencies, retained, &invalidation_limits)
        .is_ok()
    {
        return Err("invalidation range silently truncated an exhausted version budget".into());
    }
    invalidation_limits.versions = 100;
    invalidation_limits.postings = 1;
    if restored_query
        .inference_invalidations(&dependencies, retained, &invalidation_limits)
        .is_ok()
    {
        return Err("invalidation range silently truncated an exhausted posting budget".into());
    }
    if fresh.len() != 1
        || !fresh
            .first()
            .is_some_and(|claim| claim.premises.contains(&new_version))
        || !dependencies.affected(&changed, 100, 10)?.is_empty()
        || syntaxmesh_inference::PremiseDependencyIndex::build(&fresh, 10, 100)?
            .affected(&changed, 100, 10)?
            .len()
            != 1
        || Query::new(store, retained).infer(rule, &limits(), 100)? != original
    {
        return Err("restored identity confused old and new premise versions".into());
    }
    let empty = GenerationId::derive(&[b"empty-followup"]);
    store.apply_delta(GraphDelta {
        repository: manifest.repository,
        worktree: manifest.worktree,
        run_id: IndexRunId::derive(&[b"empty-followup-run"]),
        expected_base: Some(restored),
        next_generation: empty,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let exact = syntaxmesh_inference::InvalidationLimits {
        generations: 3,
        versions: 2,
        postings: 100,
        claims: 10,
    };
    let accepted_manifest = store.manifest(empty)?;
    store.set_generation_status(
        empty,
        syntaxmesh_core::GenerationStatus::VerificationPending,
    )?;
    let mut status_updated_manifest = store.manifest(empty)?;
    status_updated_manifest.status = accepted_manifest.status;
    if status_updated_manifest != accepted_manifest {
        return Err("operational status rewrote accepted generation coordinates".into());
    }
    if Query::new(store, empty).inference_invalidations(&dependencies, retained, &exact)?
        != original
            .iter()
            .map(|original_claim| original_claim.id)
            .collect()
    {
        return Err("empty accepted transition consumed a nonexistent fact-version budget".into());
    }
    Ok(())
}

pub(super) fn verify<S: GraphStore>(
    store: &S,
    rule: &Rule,
    retained: GenerationId,
    current: GenerationId,
) -> Result<(), Box<dyn Error>> {
    let historical = Query::new(store, retained).infer(rule, &limits(), 100)?;
    let retained_query = Query::new(store, retained);
    let claim = historical.first().ok_or("missing retained claim")?;
    let witness = retained_query.inference_premises(claim, 10)?;
    retained_query.verify_inference_claim(claim, rule, 10, 2)?;
    let explanation = retained_query.explain_inference_claim(claim, rule, 10, 2, 64 * 1024)?;
    if explanation.schema_version != 1
        || explanation.selection.generation != retained
        || explanation.claim.id != claim.id
        || explanation.premises.len() != claim.premises.len()
        || explanation
            .premises
            .iter()
            .zip(&claim.premises)
            .any(|(evidence, reference)| {
                evidence.reference != *reference
                    || evidence.edge.id
                        != match reference.fact {
                            syntaxmesh_core::FactRef::Edge(edge) => edge,
                            syntaxmesh_core::FactRef::File(_)
                            | syntaxmesh_core::FactRef::Provenance(_)
                            | syntaxmesh_core::FactRef::Node(_) => return true,
                        }
                    || evidence.provenance.id != evidence.edge.provenance
            })
    {
        return Err("verified inference explanation lost its ordered claim evidence".into());
    }
    if retained_query
        .explain_inference_claim(claim, rule, 10, 2, 1)
        .is_ok()
    {
        return Err("inference explanation exceeded its serialized output bound".into());
    }
    let mut altered = claim.clone();
    altered.id = StableId::derive("altered-claim", &[b"forged"]);
    if retained_query
        .verify_inference_claim(&altered, rule, 10, 2)
        .is_ok()
    {
        return Err("historical rule proof accepted altered claim identity".into());
    }
    if witness
        .iter()
        .map(|entry| syntaxmesh_core::FactVersionRef {
            fact: entry.fact,
            valid_from: entry.version.valid_from,
        })
        .collect::<Vec<_>>()
        != claim.premises
        || witness
            .iter()
            .any(|entry| entry.version.valid_until.is_some())
        || retained_query.inference_premises(claim, 1).is_ok()
        || retained_query.inference_premises(claim, 0).is_ok()
    {
        return Err("retained premise reader lost exact ordered witness or snapshot bounds".into());
    }
    let mut foreign = claim.clone();
    foreign.repository = RepositoryId::derive(&[b"foreign-claim"]);
    let mut stale = claim.clone();
    stale.generation = current;
    if retained_query.inference_premises(&foreign, 10).is_ok()
        || Query::new(store, current)
            .inference_premises(&stale, 10)
            .is_ok()
    {
        return Err("premise reader accepted foreign or closed witness".into());
    }
    let mut second = rule.clone();
    second.id = "example:second-write-path".to_owned();
    let rules = [rule.clone(), second];
    let combined = Query::new(store, retained).infer_rules(&rules, &limits(), 100, 2)?;
    let mut registered_rules = rules.clone();
    for configured in &mut registered_rules {
        configured
            .ontology_versions
            .insert("example".to_owned(), "1".to_owned());
    }
    let registry = syntaxmesh_inference::OntologyRegistry::new(
        &[("example".to_owned(), "1".to_owned())],
        1,
        4096,
    )?;
    let schema_relation = |name: &str, subject_domain: Vec<NodeId>, object_range: Vec<NodeId>| {
        syntaxmesh_inference::OntologyRelationDefinition {
            name: name.to_owned(),
            subject_domain,
            object_range,
        }
    };
    let source_concept = NodeId::derive(&[b"source-concept"]);
    let schema = syntaxmesh_inference::OntologyDefinition {
        schema_version: syntaxmesh_inference::ONTOLOGY_DEFINITION_SCHEMA_VERSION,
        namespace: "example".to_owned(),
        version: "1".to_owned(),
        concepts: vec![registered_rules[0].concept, source_concept],
        instance_relation: "instance_of".to_owned(),
        relations: vec![
            schema_relation(
                "writes",
                vec![source_concept],
                vec![registered_rules[0].concept],
            ),
            schema_relation(
                "instance_of",
                Vec::new(),
                vec![registered_rules[0].concept, source_concept],
            ),
            schema_relation(
                "participates",
                Vec::new(),
                vec![registered_rules[0].concept],
            ),
        ],
    };
    let mut invalid_membership = schema.clone();
    if let Some(writes) = invalid_membership
        .relations
        .iter_mut()
        .find(|relation| relation.name == "writes")
    {
        writes.subject_domain = vec![registered_rules[0].concept];
    }
    let invalid_membership_registry =
        syntaxmesh_inference::OntologyRegistry::from_definitions(&[invalid_membership], 11, 4096)?;
    let schema_registry = syntaxmesh_inference::OntologyRegistry::from_definitions(
        std::slice::from_ref(&schema),
        11,
        4096,
    )?;
    let schema_bound_rules =
        schema_registry.bind_rules(&registered_rules, 2, 2, limits().input_bytes)?;
    let mut invalid_schema = schema;
    invalid_schema.relations.pop();
    let invalid_schema_registry =
        syntaxmesh_inference::OntologyRegistry::from_definitions(&[invalid_schema], 10, 4096)?;
    let missing = syntaxmesh_inference::OntologyRegistry::new(&[], 1, 4096)?;
    let query = Query::new(store, retained);
    let version_only =
        query.infer_registered(&registered_rules, &registry, &limits(), 100, 2, 2)?;
    let schema_bound =
        query.infer_registered(&registered_rules, &schema_registry, &limits(), 100, 2, 2)?;
    let schema_discovered = query.infer_discovered_bound_rules(
        &syntaxmesh_inference::DiscoveryRange {
            from: store
                .manifest(retained)?
                .parent
                .ok_or("missing registered discovery parent")?,
            generations: 1,
            versions: 100,
        },
        &schema_registry,
        &schema_bound_rules,
        &limits(),
        syntaxmesh_inference::RegisteredDiscoveryBudgets {
            history_scan: 100,
            rules: 2,
            ontology_bindings: 2,
        },
    )?;
    let ordinary = query.infer_rules(&registered_rules, &limits(), 100, 2)?;
    if version_only != ordinary
        || schema_bound.len() != ordinary.len()
        || schema_discovered != schema_bound
        || schema_bound.iter().zip(&ordinary).any(|(bound, plain)| {
            bound.source != plain.source
                || bound.target != plain.target
                || bound.relation != plain.relation
                || bound.premises != plain.premises
                || bound.id == plain.id
                || bound.configuration_fingerprint == plain.configuration_fingerprint
        })
        || query
            .infer_registered(
                &registered_rules,
                &invalid_membership_registry,
                &limits(),
                100,
                2,
                2,
            )
            .is_ok()
        || query
            .infer_registered(
                &registered_rules,
                &invalid_schema_registry,
                &limits(),
                100,
                2,
                2,
            )
            .is_ok()
        || query
            .infer_registered(&registered_rules, &missing, &limits(), 100, 2, 2)
            .is_ok()
        || query
            .infer_registered(&registered_rules, &registry, &limits(), 100, 2, 1)
            .is_ok()
    {
        return Err("registered query accepted unavailable versions or exhausted bindings".into());
    }
    if combined.len() != 2
        || !combined
            .iter()
            .any(|combined_claim| historical.contains(combined_claim))
        || !Query::new(store, current)
            .infer_rules(&rules, &limits(), 100, 2)?
            .is_empty()
    {
        return Err("rule-set selection lost historical/current isolation".into());
    }
    let mut conflicting = rules;
    for (index, configured) in conflicting.iter_mut().enumerate() {
        configured
            .ontology_versions
            .insert("example".to_owned(), index.to_string());
    }
    if Query::new(store, retained)
        .infer_rules(&conflicting, &limits(), 100, 2)
        .is_ok()
    {
        return Err("retained query accepted conflicting ontology versions".into());
    }
    let selected = store.manifest(retained)?;
    let scope = syntaxmesh_inference::ClaimSelection {
        repository: selected.repository,
        worktree: selected.worktree,
        generation: retained,
    };
    let dependencies =
        syntaxmesh_inference::PremiseDependencyIndex::build_scoped(scope, &historical, 10, 100)?;
    let mut closed = std::collections::BTreeSet::new();
    let mut cursor = None;
    let mut remaining = 100_usize;
    loop {
        remaining = remaining
            .checked_sub(1)
            .ok_or("change feed did not terminate")?;
        let page = store.fact_version_changes_at_page(current, cursor, 1)?;
        for entry in page.items {
            if entry.version.valid_until == Some(current) {
                closed.insert(syntaxmesh_core::FactVersionRef {
                    fact: entry.fact,
                    valid_from: entry.version.valid_from,
                });
            }
        }
        let Some(next) = page.next_cursor else {
            break;
        };
        if cursor == Some(next) {
            return Err("change feed repeated its continuation".into());
        }
        cursor = Some(next);
    }
    let affected = dependencies.affected_scoped(scope, &closed, 100, 10)?;
    let invalidation_limits = syntaxmesh_inference::InvalidationLimits {
        generations: 10,
        versions: 100,
        postings: 100,
        claims: 10,
    };
    let explanations = Query::new(store, current).inference_invalidation_explanations(
        &dependencies,
        retained,
        &invalidation_limits,
        10,
    )?;
    let closures =
        Query::new(store, current).inference_premise_closures(retained, &invalidation_limits)?;
    let closure_sequence = store.generation_sequence(current)?;
    if closures
        .keys()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        != closed
        || closures.values().any(|closure| {
            closure.generation != current || closure.generation_sequence != closure_sequence
        })
    {
        return Err("closure coordinates differ from canonical closed-version feed".into());
    }
    if explanations != dependencies.explain_affected_scoped(scope, &closed, 100, 10, 10)?
        || explanations
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            != affected
        || Query::new(store, current)
            .inference_invalidation_explanations(&dependencies, retained, &invalidation_limits, 0)
            .is_ok()
    {
        return Err("range explanation lost exact closed-premise evidence".into());
    }
    if Query::new(store, current).inference_invalidations(
        &dependencies,
        retained,
        &invalidation_limits,
    )? != affected
        || !Query::new(store, retained)
            .inference_invalidations(&dependencies, retained, &invalidation_limits)?
            .is_empty()
    {
        return Err(
            "generation range invalidation disagreed with exact closed-version oracle".into(),
        );
    }
    let expected = historical
        .iter()
        .map(|retained_claim| retained_claim.id)
        .collect();
    if closed.len() != 1 || affected != expected {
        return Err("closed-version feed failed to identify the retained dependent claim".into());
    }
    let latest = Query::new(store, current);
    if historical.len() != 1
        || !latest.infer(rule, &limits(), 100)?.is_empty()
        || latest.infer(rule, &limits(), 1).is_ok()
        || latest.infer(rule, &limits(), 0).is_ok()
        || store
            .historical_snapshot(current)?
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Calls)
            .count()
            != 1
    {
        return Err(
            "retained inference or independent source facts changed after retraction".into(),
        );
    }
    Ok(())
}
