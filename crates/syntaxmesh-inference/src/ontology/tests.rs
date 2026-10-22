use super::*;
use crate::evaluator::tests::{fixture, selection};

fn relation(name: &str, object_range: Vec<NodeId>) -> OntologyRelationDefinition {
    OntologyRelationDefinition {
        name: name.to_owned(),
        subject_domain: Vec::new(),
        object_range,
    }
}

fn versioned_definition(
    version: &str,
    concepts: Vec<NodeId>,
    instance_relation: &str,
    mut relations: Vec<OntologyRelationDefinition>,
) -> OntologyDefinition {
    relations.push(OntologyRelationDefinition {
        name: instance_relation.to_owned(),
        subject_domain: Vec::new(),
        object_range: concepts.clone(),
    });
    OntologyDefinition {
        schema_version: ONTOLOGY_DEFINITION_SCHEMA_VERSION,
        namespace: "example".to_owned(),
        version: version.to_owned(),
        concepts,
        instance_relation: instance_relation.to_owned(),
        relations,
    }
}

#[test]
fn ontology_version_comparison_is_directional_bounded_and_structural() -> Result<(), InferenceError>
{
    let alpha = NodeId::derive(&[b"compat-alpha"]);
    let beta = NodeId::derive(&[b"compat-beta"]);
    let gamma = NodeId::derive(&[b"compat-gamma"]);
    let old = versioned_definition(
        "1",
        vec![alpha, beta],
        "instance_of",
        vec![OntologyRelationDefinition {
            name: "uses".to_owned(),
            subject_domain: vec![alpha],
            object_range: vec![beta],
        }],
    );
    let compatible = versioned_definition(
        "2",
        vec![alpha, beta, gamma],
        "instance_of",
        vec![
            OntologyRelationDefinition {
                name: "uses".to_owned(),
                subject_domain: vec![alpha, gamma],
                object_range: vec![beta, gamma],
            },
            relation("added_relation", vec![gamma]),
        ],
    );
    let mut narrowed = compatible.clone();
    narrowed.version = "3".to_owned();
    narrowed
        .relations
        .iter_mut()
        .find(|item| item.name == "uses")
        .ok_or(InferenceError::InvalidInput)?
        .subject_domain = vec![alpha];
    let mut narrowed_range = compatible.clone();
    narrowed_range.version = "8".to_owned();
    narrowed_range
        .relations
        .iter_mut()
        .find(|item| item.name == "uses")
        .ok_or(InferenceError::InvalidInput)?
        .object_range = vec![beta];
    let widened_unrestricted = versioned_definition(
        "7",
        vec![alpha, beta],
        "instance_of",
        vec![OntologyRelationDefinition {
            name: "uses".to_owned(),
            subject_domain: Vec::new(),
            object_range: Vec::new(),
        }],
    );
    let mut removed_concept = old.clone();
    removed_concept.version = "4".to_owned();
    removed_concept.concepts = vec![alpha];
    removed_concept
        .relations
        .iter_mut()
        .find(|item| item.name == "instance_of")
        .ok_or(InferenceError::InvalidInput)?
        .object_range = vec![alpha];
    removed_concept
        .relations
        .iter_mut()
        .find(|item| item.name == "uses")
        .ok_or(InferenceError::InvalidInput)?
        .object_range = Vec::new();
    let removed_relation = versioned_definition("5", vec![alpha, beta], "instance_of", vec![]);
    let changed_instance = versioned_definition(
        "6",
        vec![alpha, beta],
        "type_of",
        vec![relation("instance_of", vec![alpha, beta])],
    );
    let definitions = vec![
        old.clone(),
        compatible.clone(),
        narrowed,
        narrowed_range,
        removed_concept,
        removed_relation,
        changed_instance,
        widened_unrestricted,
    ];
    let bytes =
        serde_json::to_vec(&definitions).map_err(|_encoding| InferenceError::IdentityEncoding)?;
    let registry = OntologyRegistry::from_definitions(&definitions, 128, bytes.len())?;
    let work = definition_entry_count(&old)? + definition_entry_count(&compatible)?;
    if registry.compare_versions("example", "1", "2", work)
        != Ok(OntologyCompatibility::BackwardCompatible)
        || registry.compare_versions("example", "2", "1", work)
            != Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::ConceptRemoved(gamma),
            ))
        || registry.compare_versions("example", "2", "3", 64)
            != Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::SubjectDomainNarrowed("uses".to_owned()),
            ))
        || registry.compare_versions("example", "2", "8", 64)
            != Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::ObjectRangeNarrowed("uses".to_owned()),
            ))
        || registry.compare_versions("example", "1", "7", 64)
            != Ok(OntologyCompatibility::BackwardCompatible)
        || registry.compare_versions("example", "1", "4", 64)
            != Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::ConceptRemoved(beta),
            ))
        || registry.compare_versions("example", "1", "5", 64)
            != Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::RelationRemoved("uses".to_owned()),
            ))
        || registry.compare_versions("example", "1", "6", 64)
            != Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::InstanceRelationChanged {
                    from: "instance_of".to_owned(),
                    to: "type_of".to_owned(),
                },
            ))
        || registry.compare_versions("example", "1", "2", work - 1)
            != Err(InferenceError::BudgetExhausted)
        || registry.compare_versions("example", "1", "missing", 64)
            != Err(InferenceError::InvalidInput)
        || OntologyRegistry::new(&[("example".to_owned(), "1".to_owned())], 1, 128)?
            .compare_versions("example", "1", "1", 64)
            != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn registry_rejects_unknown_versions_duplicates_and_exhausted_bounds() -> Result<(), InferenceError>
{
    let (nodes, premises, rule, limits) = fixture();
    let entries = vec![("example".to_owned(), "1".to_owned())];
    let registry = OntologyRegistry::new(&entries, 1, 4096)?;
    let selected = selection(&nodes, &premises);
    let rules = std::slice::from_ref(&rule);
    if evaluate_registered(&selected, rules, &registry, &limits, 1, 1)?
        != crate::evaluate(&selected, &rule, &limits)?
    {
        return Err(InferenceError::InvalidInput);
    }
    let mut unknown = rule.clone();
    unknown
        .ontology_versions
        .insert("example".to_owned(), "2".to_owned());
    if registry.validate(&[unknown], 1, 1, 4096) != Err(InferenceError::InvalidInput)
        || registry.validate(rules, 1, 0, 4096) != Err(InferenceError::InvalidInput)
        || registry.validate(&[rule.clone(), rule], 1, 2, 4096)
            != Err(InferenceError::BudgetExhausted)
        || OntologyRegistry::new(
            &[
                entries
                    .first()
                    .cloned()
                    .ok_or(InferenceError::InvalidInput)?,
                entries
                    .first()
                    .cloned()
                    .ok_or(InferenceError::InvalidInput)?,
            ],
            2,
            4096,
        )
        .is_ok()
        || OntologyRegistry::new(&entries, 0, 4096).is_ok()
        || OntologyRegistry::new(&[(" ".to_owned(), "1".to_owned())], 1, 4096).is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn registration_and_validation_enforce_exact_byte_boundaries() -> Result<(), InferenceError> {
    let entries = vec![("example".to_owned(), "1".to_owned())];
    let encoded =
        serde_json::to_vec(&entries).map_err(|_encoding| InferenceError::IdentityEncoding)?;
    let registry = OntologyRegistry::new(&entries, 1, encoded.len())?;
    if OntologyRegistry::new(&entries, 1, encoded.len().saturating_sub(1)).is_ok()
        || OntologyRegistry::new(&entries, 1, 0).is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    let (_nodes, _premises, rule, _limits) = fixture();
    let rules = [rule];
    let bytes = serde_json::to_vec(&rules).map_err(|_encoding| InferenceError::IdentityEncoding)?;
    registry.validate(&rules, 1, 1, bytes.len())?;
    if registry.validate(&rules, 1, 1, bytes.len().saturating_sub(1))
        != Err(InferenceError::BudgetExhausted)
        || registry.validate(&rules, 1, 1, 0) != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn schema_registry_checks_concepts_relations_versions_and_bounds() -> Result<(), InferenceError> {
    let (mut nodes, mut premises, rule, mut limits) = fixture();
    let source_concept = NodeId::derive(&[b"source-concept"]);
    let writer = premises
        .iter()
        .find(|premise| {
            premise.edge.relation
                == (RelationKind::External {
                    namespace: "example".to_owned(),
                    relation: "writes".to_owned(),
                })
        })
        .map(|premise| premise.edge.source)
        .ok_or(InferenceError::InvalidInput)?;
    let store = premises
        .iter()
        .find(|premise| {
            matches!(&premise.edge.relation,
                RelationKind::External { namespace, relation }
                    if namespace == "example" && relation == "writes")
        })
        .map(|premise| premise.edge.target)
        .ok_or(InferenceError::InvalidInput)?;
    let generation = premises
        .first()
        .map(|premise| premise.generation)
        .ok_or(InferenceError::InvalidInput)?;
    let provenance = premises
        .first()
        .map(|premise| premise.edge.provenance)
        .ok_or(InferenceError::InvalidInput)?;
    let writer_membership_edge = syntaxmesh_core::Edge {
        id: syntaxmesh_core::EdgeId::derive(&[b"writer-instance"]),
        source: writer,
        target: source_concept,
        relation: RelationKind::External {
            namespace: "example".to_owned(),
            relation: "instance_of".to_owned(),
        },
        provenance,
        extension_payload: None,
    };
    nodes.insert(source_concept);
    premises.push(crate::Premise {
        generation,
        valid_from: generation,
        edge: writer_membership_edge,
    });
    limits.facts = nodes.len() + premises.len();
    let definition = OntologyDefinition {
        schema_version: ONTOLOGY_DEFINITION_SCHEMA_VERSION,
        namespace: "example".to_owned(),
        version: "1".to_owned(),
        concepts: vec![rule.concept, source_concept],
        instance_relation: "instance_of".to_owned(),
        relations: vec![
            OntologyRelationDefinition {
                name: "writes".to_owned(),
                subject_domain: vec![source_concept],
                object_range: vec![rule.concept],
            },
            relation("instance_of", vec![rule.concept, source_concept]),
            relation("participates_in_write", vec![rule.concept]),
        ],
    };
    let definitions = [definition.clone()];
    let encoded =
        serde_json::to_vec(&definitions).map_err(|_encoding| InferenceError::IdentityEncoding)?;
    let registry = OntologyRegistry::from_definitions(&definitions, 11, encoded.len())?;
    let selected = selection(&nodes, &premises);
    let selected_scope = crate::ClaimSelection {
        repository: selected.repository,
        worktree: selected.worktree,
        generation: selected.generation,
    };
    let rules = std::slice::from_ref(&rule);
    let bound_rules = registry.bind_rules(rules, 1, 1, encoded.len())?;
    let reordered_definition = {
        let mut definition = definition.clone();
        definition.concepts.reverse();
        definition.relations.reverse();
        for relation in &mut definition.relations {
            relation.subject_domain.reverse();
            relation.object_range.reverse();
        }
        definition
    };
    let reordered_registry = OntologyRegistry::from_definitions(&[reordered_definition], 11, 4096)?;
    let reordered_rules = reordered_registry.bind_rules(rules, 1, 1, 4096)?;
    let mut changed_definition = definition.clone();
    if let Some(writes) = changed_definition
        .relations
        .iter_mut()
        .find(|relation| relation.name == "writes")
    {
        writes.subject_domain.clear();
    }
    let changed_registry = OntologyRegistry::from_definitions(&[changed_definition], 11, 4096)?;
    let changed_rules = changed_registry.bind_rules(rules, 1, 1, 4096)?;
    if bound_rules.rules() != reordered_rules.rules()
        || bound_rules.rules() == rules
        || bound_rules.rules() == changed_rules.rules()
        || crate::rule_set_identity(bound_rules.rules(), 1, 4096)?
            == crate::rule_set_identity(changed_rules.rules(), 1, 4096)?
        || crate::materialization_identity(selected_scope, bound_rules.rules(), 1, 4096)?
            == crate::materialization_identity(selected_scope, changed_rules.rules(), 1, 4096)?
        || registry.validate(rules, 1, 1, 4096).is_err()
        || changed_registry
            .evaluate_bound(&selected, &bound_rules, &limits, 1, 1)
            .is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    let registered = registry.evaluate_bound(&selected, &bound_rules, &limits, 1, 1)?;
    let ordinary = crate::evaluate(&selected, &rule, &limits)?;
    let changed = changed_registry.evaluate_bound(&selected, &changed_rules, &limits, 1, 1)?;
    if registered.iter().map(|claim| claim.id).collect::<Vec<_>>()
        == ordinary.iter().map(|claim| claim.id).collect::<Vec<_>>()
        || registered.iter().map(|claim| claim.id).collect::<Vec<_>>()
            == changed.iter().map(|claim| claim.id).collect::<Vec<_>>()
        || registered.first().is_none_or(|claim| {
            bound_rules.rules().first().is_none_or(|bound_rule| {
                claim.configuration_fingerprint != bound_rule.configuration_fingerprint
            })
        })
    {
        return Err(InferenceError::InvalidInput);
    }

    let mut missing_subject_type = premises.clone();
    missing_subject_type.retain(|premise| {
        premise.edge.id != syntaxmesh_core::EdgeId::derive(&[b"writer-instance"])
    });
    let missing_subject_selection = selection(&nodes, &missing_subject_type);
    let mut incorrect_object_type = premises.clone();
    if let Some(membership_premise) = incorrect_object_type.iter_mut().find(|premise| {
        premise.edge.source == store
            && matches!(&premise.edge.relation,
                    RelationKind::External { namespace, relation }
                        if namespace == "example" && relation == "instance_of")
    }) {
        membership_premise.edge.target = source_concept;
    }
    let incorrect_object_selection = selection(&nodes, &incorrect_object_type);
    let mut incorrect_instance_target = premises.clone();
    if let Some(membership_premise) = incorrect_instance_target
        .iter_mut()
        .find(|premise| premise.edge.id == syntaxmesh_core::EdgeId::derive(&[b"writer-instance"]))
    {
        membership_premise.edge.target = NodeId::derive(&[b"undeclared-instance"]);
    }
    let incorrect_instance_selection = selection(&nodes, &incorrect_instance_target);
    let mut exhausted = limits;
    exhausted.edge_visits = 1;
    let missing_subject_rejected =
        evaluate_registered(&missing_subject_selection, rules, &registry, &limits, 1, 1).is_err();
    let incorrect_object_rejected =
        evaluate_registered(&incorrect_object_selection, rules, &registry, &limits, 1, 1).is_err();
    let incorrect_instance_rejected = evaluate_registered(
        &incorrect_instance_selection,
        rules,
        &registry,
        &limits,
        1,
        1,
    )
    .is_err();
    let exhausted_rejected =
        evaluate_registered(&selected, rules, &registry, &exhausted, 1, 1).is_err();
    if !missing_subject_rejected
        || !incorrect_object_rejected
        || !incorrect_instance_rejected
        || !exhausted_rejected
    {
        return Err(InferenceError::InvalidInput);
    }

    let mut missing_relation = definition.clone();
    missing_relation.relations.pop();
    let missing_relation_registry =
        OntologyRegistry::from_definitions(&[missing_relation], 10, 4096)?;
    let mut missing_concept = definition.clone();
    missing_concept.concepts = vec![source_concept];
    if let Some(writes) = missing_concept
        .relations
        .iter_mut()
        .find(|relation| relation.name == "writes")
    {
        writes.object_range = vec![source_concept];
    }
    if let Some(instance) = missing_concept
        .relations
        .iter_mut()
        .find(|relation| relation.name == "instance_of")
    {
        instance.object_range = vec![source_concept];
    }
    if let Some(output) = missing_concept
        .relations
        .iter_mut()
        .find(|relation| relation.name == "participates_in_write")
    {
        output.object_range = vec![source_concept];
    }
    let missing_concept_registry = OntologyRegistry::from_definitions(&[missing_concept], 9, 4096)?;
    let mut wrong_version = rule.clone();
    wrong_version
        .ontology_versions
        .insert("example".to_owned(), "2".to_owned());
    let mut unbound_relation = rule.clone();
    unbound_relation.ontology_versions.clear();
    let mut incompatible_range = definition.clone();
    let other_concept = NodeId::derive(&[b"other-concept"]);
    incompatible_range.concepts.push(other_concept);
    if let Some(instance) = incompatible_range
        .relations
        .iter_mut()
        .find(|relation| relation.name == "instance_of")
    {
        instance.object_range.push(other_concept);
    }
    if let Some(output) = incompatible_range
        .relations
        .iter_mut()
        .find(|relation| relation.name == "participates_in_write")
    {
        output.object_range = vec![other_concept];
    }
    let incompatible_range_registry =
        OntologyRegistry::from_definitions(&[incompatible_range], 15, 4096)?;
    let mut incompatible_declaration_range = definition.clone();
    incompatible_declaration_range.concepts.push(other_concept);
    if let Some(declaration) = incompatible_declaration_range
        .relations
        .iter_mut()
        .find(|relation| relation.name == "instance_of")
    {
        declaration.object_range = vec![other_concept];
    }
    let incompatible_declaration_registry =
        OntologyRegistry::from_definitions(&[incompatible_declaration_range], 15, 4096);
    let mut invalid_namespace = definition.clone();
    invalid_namespace.namespace = " ".to_owned();
    let mut unsupported_schema = definition.clone();
    unsupported_schema.schema_version = ONTOLOGY_DEFINITION_SCHEMA_VERSION + 1;
    let mut empty_relation_name = definition.clone();
    if let Some(relation) = empty_relation_name.relations.first_mut() {
        relation.name = " ".to_owned();
    }
    if missing_relation_registry.validate(rules, 1, 1, 4096) != Err(InferenceError::InvalidInput)
        || missing_concept_registry.validate(rules, 1, 1, 4096) != Err(InferenceError::InvalidInput)
        || registry.validate(&[wrong_version], 1, 1, 4096) != Err(InferenceError::InvalidInput)
        || registry.validate(&[unbound_relation], 1, 1, 4096) != Err(InferenceError::InvalidInput)
        || incompatible_range_registry.validate(rules, 1, 1, 4096)
            != Err(InferenceError::InvalidInput)
        || incompatible_declaration_registry.is_ok()
        || OntologyRegistry::from_definitions(&[invalid_namespace], 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&[unsupported_schema], 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&[empty_relation_name], 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&definitions, 6, 4096).err()
            != Some(InferenceError::BudgetExhausted)
        || OntologyRegistry::from_definitions(&definitions, 11, encoded.len().saturating_sub(1))
            .err()
            != Some(InferenceError::BudgetExhausted)
    {
        return Err(InferenceError::InvalidInput);
    }
    let mut duplicate_concept = definition.clone();
    duplicate_concept.concepts.push(rule.concept);
    let mut duplicate_relation = definition.clone();
    duplicate_relation
        .relations
        .push(relation("writes", Vec::new()));
    let mut foreign_domain = definition.clone();
    if let Some(writes) = foreign_domain
        .relations
        .iter_mut()
        .find(|relation| relation.name == "writes")
    {
        writes.subject_domain.push(NodeId::derive(&[b"undeclared"]));
    }
    let mut duplicate_range = definition.clone();
    if let Some(output) = duplicate_range
        .relations
        .iter_mut()
        .find(|relation| relation.name == "participates_in_write")
    {
        output.object_range.push(rule.concept);
    }
    let duplicate_version = [definition.clone(), definition];
    if OntologyRegistry::from_definitions(&[duplicate_concept], 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&[duplicate_relation], 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&[foreign_domain], 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&[duplicate_range], 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&duplicate_version, 10, 4096).is_ok()
        || OntologyRegistry::from_definitions(&[], 10, 4096).is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}
