use crate::{Claim, InferenceError, Limits, Rule, Selection};
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{FactVersionRef, NodeId, RelationKind, StableId};

pub const ONTOLOGY_DEFINITION_SCHEMA_VERSION: u32 = 3;

/// Independent admission bounds for registered ontology definitions and rule binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredInferenceAdmission {
    pub registry_entries: usize,
    pub registry_bytes: usize,
    pub bindings: usize,
}

/// A namespaced relation and its optional subject/object concept constraints.
/// Empty constraints leave that endpoint unrestricted.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyRelationDefinition {
    pub name: String,
    pub subject_domain: Vec<NodeId>,
    pub object_range: Vec<NodeId>,
}

/// Caller-supplied, versioned declarations for one ontology revision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyDefinition {
    pub schema_version: u32,
    pub namespace: String,
    pub version: String,
    pub concepts: Vec<NodeId>,
    pub instance_relation: String,
    pub relations: Vec<OntologyRelationDefinition>,
}

/// Result of comparing an older ontology definition with a newer revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyCompatibility {
    /// The newer declaration preserves the older vocabulary and constraints.
    BackwardCompatible,
    /// The first deterministic schema change that narrows or removes meaning.
    Incompatible(OntologyCompatibilityIssue),
}

/// A schema-level reason an ontology revision is not backward-compatible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyCompatibilityIssue {
    ConceptRemoved(NodeId),
    RelationRemoved(String),
    InstanceRelationChanged { from: String, to: String },
    SubjectDomainNarrowed(String),
    ObjectRangeNarrowed(String),
}

type RelationIdentity = (String, String);

#[derive(Clone, Copy)]
struct RelationSchema<'schema> {
    ontology: &'schema OntologyDefinition,
    relation: &'schema OntologyRelationDefinition,
}

struct OntologyFactTypes<'schema> {
    relations: BTreeMap<RelationIdentity, RelationSchema<'schema>>,
    memberships: BTreeMap<NodeId, BTreeSet<NodeId>>,
}

impl OntologyFactTypes<'_> {
    fn has_any_type(
        &self,
        node: NodeId,
        accepted: &[NodeId],
        remaining: &mut usize,
    ) -> Result<bool, InferenceError> {
        let Some(actual) = self.memberships.get(&node) else {
            return Ok(false);
        };
        for concept in accepted {
            consume(remaining, 1)?;
            if actual.contains(concept) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn validate_claims(
        &self,
        claims: &[Claim],
        remaining: &mut usize,
    ) -> Result<(), InferenceError> {
        for claim in claims {
            consume(remaining, 1)?;
            let RelationKind::External {
                namespace,
                relation,
            } = &claim.relation
            else {
                return Err(InferenceError::InvalidInput);
            };
            let schema = self
                .relations
                .get(&(namespace.clone(), relation.clone()))
                .ok_or(InferenceError::InvalidInput)?;
            if (!schema.relation.subject_domain.is_empty()
                && !self.has_any_type(claim.source, &schema.relation.subject_domain, remaining)?)
                || (!schema.relation.object_range.is_empty()
                    && !self.has_any_type(
                        claim.target,
                        &schema.relation.object_range,
                        remaining,
                    )?)
            {
                return Err(InferenceError::InvalidInput);
            }
        }
        Ok(())
    }
}

fn consume(remaining: &mut usize, amount: usize) -> Result<(), InferenceError> {
    *remaining = remaining
        .checked_sub(amount)
        .ok_or(InferenceError::BudgetExhausted)?;
    Ok(())
}

fn relation_schema<'schema>(
    relations: &'schema BTreeMap<RelationIdentity, RelationSchema<'schema>>,
    relation: &RelationKind,
) -> Option<&'schema RelationSchema<'schema>> {
    let RelationKind::External {
        namespace,
        relation,
    } = relation
    else {
        return None;
    };
    relations.get(&(namespace.clone(), relation.clone()))
}

/// Explicit available versions; registration does not prove semantic compatibility.
pub struct OntologyRegistry {
    versions: BTreeMap<String, BTreeSet<String>>,
    definitions: BTreeMap<(String, String), OntologyDefinition>,
    fingerprints: BTreeMap<(String, String), StableId>,
}

/// Immutable rules bound to the exact ontology definitions supplied by a registry.
///
/// The contained rules carry their effective schema-bound configuration
/// fingerprints while preserving the serialized `Rule` layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundRuleSet {
    base_rules: Vec<Rule>,
    rules: Vec<Rule>,
    bindings: BTreeMap<(String, String), StableId>,
}

impl BoundRuleSet {
    /// Rules whose configuration fingerprints include their effective ontology definitions.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
}

fn definition_fingerprint(definition: &OntologyDefinition) -> Result<StableId, InferenceError> {
    let mut concepts = definition.concepts.clone();
    concepts.sort_unstable();
    let mut relations = definition
        .relations
        .iter()
        .map(|relation| {
            let mut subject_domain = relation.subject_domain.clone();
            let mut object_range = relation.object_range.clone();
            subject_domain.sort_unstable();
            object_range.sort_unstable();
            (relation.name.as_str(), subject_domain, object_range)
        })
        .collect::<Vec<_>>();
    relations.sort_unstable_by(|left, right| left.0.cmp(right.0));
    let encoded = serde_json::to_vec(&(
        definition.schema_version,
        &definition.namespace,
        &definition.version,
        concepts,
        &definition.instance_relation,
        relations,
    ))
    .map_err(|_encoding| InferenceError::IdentityEncoding)?;
    Ok(StableId::derive("ontology-definition-v1", &[&encoded]))
}

fn bind_configuration_fingerprint(
    rule: &Rule,
    fingerprints: &BTreeMap<String, StableId>,
) -> Result<StableId, InferenceError> {
    if fingerprints.is_empty() {
        return Ok(rule.configuration_fingerprint);
    }
    let encoded = serde_json::to_vec(&(rule.configuration_fingerprint, fingerprints))
        .map_err(|_encoding| InferenceError::IdentityEncoding)?;
    Ok(StableId::derive(
        "inference-ontology-bound-rule-config-v1",
        &[&encoded],
    ))
}

impl OntologyRegistry {
    /// Register a bounded complete collection of available namespace/version pairs.
    ///
    /// # Errors
    /// Rejects zero bounds, oversized collections, empty identifiers or duplicates.
    pub fn new(
        versions: &[(String, String)],
        entry_budget: usize,
        byte_budget: usize,
    ) -> Result<Self, InferenceError> {
        if entry_budget == 0 {
            return Err(InferenceError::InvalidInput);
        }
        if versions.len() > entry_budget {
            return Err(InferenceError::BudgetExhausted);
        }
        crate::input_budget::validate_encoded(versions, byte_budget)?;
        let mut registered = BTreeMap::<String, BTreeSet<String>>::new();
        for (namespace, version) in versions {
            if namespace.trim().is_empty()
                || version.trim().is_empty()
                || !registered
                    .entry(namespace.clone())
                    .or_default()
                    .insert(version.clone())
            {
                return Err(InferenceError::InvalidInput);
            }
        }
        Ok(Self {
            versions: registered,
            definitions: BTreeMap::new(),
            fingerprints: BTreeMap::new(),
        })
    }

    /// Register complete bounded definitions for exact namespace/version pairs.
    ///
    /// # Errors
    /// Rejects empty/duplicate declarations, unsupported schemas, undeclared
    /// signature concepts and exhausted entry or encoded-byte bounds.
    pub fn from_definitions(
        definitions: &[OntologyDefinition],
        entry_budget: usize,
        byte_budget: usize,
    ) -> Result<Self, InferenceError> {
        if definitions.is_empty() || entry_budget == 0 {
            return Err(InferenceError::InvalidInput);
        }
        crate::input_budget::validate_encoded(definitions, byte_budget)?;
        let mut total_entries = 0_usize;
        let mut versions = BTreeMap::<String, BTreeSet<String>>::new();
        let mut registered = BTreeMap::new();
        let mut fingerprints = BTreeMap::new();
        for definition in definitions {
            if definition.schema_version != ONTOLOGY_DEFINITION_SCHEMA_VERSION
                || definition.namespace.trim().is_empty()
                || definition.version.trim().is_empty()
                || definition.instance_relation.trim().is_empty()
                || definition.concepts.is_empty()
            {
                return Err(InferenceError::InvalidInput);
            }
            Self::admit_entries(&mut total_entries, 1, entry_budget)?;
            let mut concepts = BTreeSet::new();
            for concept in &definition.concepts {
                Self::admit_entries(&mut total_entries, 1, entry_budget)?;
                if !concepts.insert(*concept) {
                    return Err(InferenceError::InvalidInput);
                }
            }
            let mut relations = BTreeSet::new();
            for relation in &definition.relations {
                Self::admit_entries(&mut total_entries, 1, entry_budget)?;
                if relation.name.trim().is_empty() || !relations.insert(&relation.name) {
                    return Err(InferenceError::InvalidInput);
                }
                for constraints in [&relation.subject_domain, &relation.object_range] {
                    let mut unique = BTreeSet::new();
                    for concept in constraints {
                        Self::admit_entries(&mut total_entries, 1, entry_budget)?;
                        if !unique.insert(*concept) || !concepts.contains(concept) {
                            return Err(InferenceError::InvalidInput);
                        }
                    }
                }
            }
            let instance = definition
                .relations
                .iter()
                .find(|relation| relation.name == definition.instance_relation)
                .ok_or(InferenceError::InvalidInput)?;
            let instance_range = instance
                .object_range
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            if instance_range != concepts {
                return Err(InferenceError::InvalidInput);
            }
            let identity = (definition.namespace.clone(), definition.version.clone());
            let fingerprint = definition_fingerprint(definition)?;
            if registered
                .insert(identity.clone(), definition.clone())
                .is_some()
                || !versions
                    .entry(definition.namespace.clone())
                    .or_default()
                    .insert(definition.version.clone())
            {
                return Err(InferenceError::InvalidInput);
            }
            if fingerprints.insert(identity, fingerprint).is_some() {
                return Err(InferenceError::InvalidInput);
            }
        }
        Ok(Self {
            versions,
            definitions: registered,
            fingerprints,
        })
    }

    /// Compare two registered definitions in the same namespace.
    ///
    /// Compatibility is directional: additions and widened relation
    /// signatures are allowed, while removed vocabulary or narrowed
    /// signatures are not. The work budget covers all definition entries
    /// inspected before comparison begins.
    ///
    /// # Errors
    /// Rejects empty identifiers, unknown definitions, version-only registries,
    /// zero budgets, and comparisons whose bounded entry count exceeds the
    /// supplied work budget.
    pub fn compare_versions(
        &self,
        namespace: &str,
        from_version: &str,
        to_version: &str,
        work_budget: usize,
    ) -> Result<OntologyCompatibility, InferenceError> {
        if namespace.trim().is_empty()
            || from_version.trim().is_empty()
            || to_version.trim().is_empty()
            || work_budget == 0
        {
            return Err(InferenceError::InvalidInput);
        }
        let from = self
            .definitions
            .get(&(namespace.to_owned(), from_version.to_owned()))
            .ok_or(InferenceError::InvalidInput)?;
        let to = self
            .definitions
            .get(&(namespace.to_owned(), to_version.to_owned()))
            .ok_or(InferenceError::InvalidInput)?;
        let required_work = definition_entry_count(from)?
            .checked_add(definition_entry_count(to)?)
            .ok_or(InferenceError::BudgetExhausted)?;
        if required_work > work_budget {
            return Err(InferenceError::BudgetExhausted);
        }

        if from.instance_relation != to.instance_relation {
            return Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::InstanceRelationChanged {
                    from: from.instance_relation.clone(),
                    to: to.instance_relation.clone(),
                },
            ));
        }

        let from_concepts = from.concepts.iter().copied().collect::<BTreeSet<_>>();
        let to_concepts = to.concepts.iter().copied().collect::<BTreeSet<_>>();
        if let Some(removed) = from_concepts.difference(&to_concepts).next() {
            return Ok(OntologyCompatibility::Incompatible(
                OntologyCompatibilityIssue::ConceptRemoved(*removed),
            ));
        }

        let to_relations = to
            .relations
            .iter()
            .map(|relation| (relation.name.as_str(), relation))
            .collect::<BTreeMap<_, _>>();
        let mut from_relations = from.relations.iter().collect::<Vec<_>>();
        from_relations.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        for previous in from_relations {
            let Some(next) = to_relations.get(previous.name.as_str()) else {
                return Ok(OntologyCompatibility::Incompatible(
                    OntologyCompatibilityIssue::RelationRemoved(previous.name.clone()),
                ));
            };
            if !constraint_widens(&previous.subject_domain, &next.subject_domain) {
                return Ok(OntologyCompatibility::Incompatible(
                    OntologyCompatibilityIssue::SubjectDomainNarrowed(previous.name.clone()),
                ));
            }
            if !constraint_widens(&previous.object_range, &next.object_range) {
                return Ok(OntologyCompatibility::Incompatible(
                    OntologyCompatibilityIssue::ObjectRangeNarrowed(previous.name.clone()),
                ));
            }
        }
        Ok(OntologyCompatibility::BackwardCompatible)
    }

    fn admit_entries(
        total_entries: &mut usize,
        added: usize,
        entry_budget: usize,
    ) -> Result<(), InferenceError> {
        *total_entries = total_entries
            .checked_add(added)
            .ok_or(InferenceError::BudgetExhausted)?;
        if *total_entries > entry_budget {
            return Err(InferenceError::BudgetExhausted);
        }
        Ok(())
    }

    /// Bind rules to the exact registered ontology definitions.
    ///
    /// The opaque result exposes rules suitable for claim, rule-set and
    /// durable-operation identity without changing their serialized layout.
    /// For the legacy version-only registry it validates versions and leaves
    /// rule configuration fingerprints unchanged.
    ///
    /// # Errors
    /// Rejects unknown versions, undeclared rule relations/concepts, malformed
    /// existing bindings or exhausted rule, byte and binding budgets.
    pub fn bind_rules(
        &self,
        rules: &[Rule],
        rule_budget: usize,
        binding_budget: usize,
        byte_budget: usize,
    ) -> Result<BoundRuleSet, InferenceError> {
        self.validate(rules, rule_budget, binding_budget, byte_budget)?;
        let mut bindings = BTreeMap::new();
        for rule in rules {
            for (namespace, version) in &rule.ontology_versions {
                if !self.definitions.is_empty() {
                    let identity = (namespace.clone(), version.clone());
                    let fingerprint = self
                        .fingerprints
                        .get(&identity)
                        .copied()
                        .ok_or(InferenceError::InvalidInput)?;
                    bindings.insert(identity, fingerprint);
                }
            }
        }
        let mut bound = rules.to_vec();
        for rule in &mut bound {
            let effective = rule
                .ontology_versions
                .iter()
                .filter_map(|(namespace, version)| {
                    bindings
                        .get(&(namespace.clone(), version.clone()))
                        .map(|fingerprint| (namespace.clone(), *fingerprint))
                })
                .collect::<BTreeMap<_, _>>();
            rule.configuration_fingerprint = bind_configuration_fingerprint(rule, &effective)?;
        }
        Ok(BoundRuleSet {
            base_rules: rules.to_vec(),
            rules: bound,
            bindings,
        })
    }

    /// Verify a bound rule set belongs to this exact registry before data access.
    ///
    /// # Errors
    /// Rejects a set bound to other ontology definitions, invalid rule
    /// declarations, or exhausted rule, byte and binding budgets.
    pub fn validate_bound_rules(
        &self,
        bound: &BoundRuleSet,
        rule_budget: usize,
        binding_budget: usize,
        byte_budget: usize,
    ) -> Result<(), InferenceError> {
        self.validate(&bound.base_rules, rule_budget, binding_budget, byte_budget)?;
        self.validate_bound_identity(bound, rule_budget, binding_budget, byte_budget)
    }

    /// Verify the rule/schema identity without repeating declaration admission.
    ///
    /// `BoundRuleSet` values can only be constructed by this crate's successful
    /// `bind_rules` path; this check confirms exact registry ownership and bounds.
    ///
    /// # Errors
    /// Rejects a foreign schema binding or exhausted identity bounds.
    pub fn validate_bound_identity(
        &self,
        bound: &BoundRuleSet,
        rule_budget: usize,
        binding_budget: usize,
        byte_budget: usize,
    ) -> Result<(), InferenceError> {
        if rule_budget == 0 || bound.base_rules.is_empty() || binding_budget == 0 {
            return Err(InferenceError::InvalidInput);
        }
        if bound.base_rules.len() > rule_budget {
            return Err(InferenceError::BudgetExhausted);
        }
        crate::input_budget::validate_encoded(&bound.base_rules, byte_budget)?;
        self.validate_bound_binding(bound, binding_budget)
    }

    fn validate_bound_binding(
        &self,
        bound: &BoundRuleSet,
        binding_budget: usize,
    ) -> Result<(), InferenceError> {
        let expected = self.expected_bindings(&bound.base_rules)?;
        let binding_count = bound.base_rules.iter().try_fold(0_usize, |total, rule| {
            total
                .checked_add(rule.ontology_versions.len())
                .ok_or(InferenceError::BudgetExhausted)
        })?;
        if binding_count > binding_budget {
            return Err(InferenceError::BudgetExhausted);
        }
        if expected != bound.bindings {
            return Err(InferenceError::InvalidInput);
        }
        let mut expected_rules = bound.base_rules.clone();
        for rule in &mut expected_rules {
            let effective = rule
                .ontology_versions
                .iter()
                .filter_map(|(namespace, version)| {
                    expected
                        .get(&(namespace.clone(), version.clone()))
                        .map(|fingerprint| (namespace.clone(), *fingerprint))
                })
                .collect::<BTreeMap<_, _>>();
            rule.configuration_fingerprint = bind_configuration_fingerprint(rule, &effective)?;
        }
        if expected_rules != bound.rules {
            return Err(InferenceError::InvalidInput);
        }
        Ok(())
    }

    /// Evaluate an immutable bound rule set after checking it belongs to this registry.
    ///
    /// # Errors
    /// Rejects a set bound to other ontology definitions, invalid retained fact
    /// typing, incompatible declared ranges or exhausted budgets.
    pub fn evaluate_bound(
        &self,
        selection: &Selection<'_>,
        bound: &BoundRuleSet,
        limits: &Limits,
        rule_budget: usize,
        binding_budget: usize,
    ) -> Result<Vec<Claim>, InferenceError> {
        self.evaluate_bound_seeded(selection, bound, limits, rule_budget, binding_budget, None)
    }

    /// Validate the full selected schema fact set while discovering only witnesses
    /// containing exact opened premise versions.
    ///
    /// # Errors
    /// Rejects invalid schema facts, inactive seeds or exhausted bounds.
    pub fn evaluate_bound_discovered(
        &self,
        selection: &Selection<'_>,
        bound: &BoundRuleSet,
        seeds: &[FactVersionRef],
        limits: &Limits,
        rule_budget: usize,
        binding_budget: usize,
    ) -> Result<Vec<Claim>, InferenceError> {
        self.evaluate_bound_seeded(
            selection,
            bound,
            limits,
            rule_budget,
            binding_budget,
            Some(seeds),
        )
    }

    /// Is this exact external relation the instance-membership predicate of a
    /// definition in this bounded registry?
    #[must_use]
    pub fn is_instance_membership_relation(&self, relation: &RelationKind) -> bool {
        let RelationKind::External {
            namespace,
            relation,
        } = relation
        else {
            return false;
        };
        self.definitions.values().any(|definition| {
            definition.namespace == *namespace && definition.instance_relation == *relation
        })
    }

    fn evaluate_bound_seeded(
        &self,
        selection: &Selection<'_>,
        bound: &BoundRuleSet,
        limits: &Limits,
        rule_budget: usize,
        binding_budget: usize,
        seeds: Option<&[FactVersionRef]>,
    ) -> Result<Vec<Claim>, InferenceError> {
        self.validate_bound_identity(bound, rule_budget, binding_budget, limits.input_bytes)?;
        if self.definitions.is_empty() {
            if let Some(seeds) = seeds {
                return crate::discover_rules(selection, bound.rules(), seeds, limits, rule_budget);
            }
            return crate::evaluate_rules(selection, bound.rules(), limits, rule_budget);
        }
        crate::input_budget::validate(selection, bound.rules(), limits)?;
        let (fact_types, remaining) =
            self.validate_facts(selection, bound.rules(), limits.edge_visits)?;
        let (claims, mut remaining) = if let Some(seeds) = seeds {
            crate::discovery::discover_rules_with_budget(
                selection,
                bound.rules(),
                seeds,
                limits,
                rule_budget,
                remaining,
            )?
        } else {
            crate::rule_set::evaluate_rules_with_budget(
                selection,
                bound.rules(),
                limits,
                rule_budget,
                remaining,
            )?
        };
        fact_types.validate_claims(&claims, &mut remaining)?;
        Ok(claims)
    }

    fn expected_bindings(
        &self,
        rules: &[Rule],
    ) -> Result<BTreeMap<(String, String), StableId>, InferenceError> {
        let mut bindings = BTreeMap::new();
        if self.definitions.is_empty() {
            return Ok(bindings);
        }
        for rule in rules {
            for (namespace, version) in &rule.ontology_versions {
                let identity = (namespace.clone(), version.clone());
                let fingerprint = self
                    .fingerprints
                    .get(&identity)
                    .copied()
                    .ok_or(InferenceError::InvalidInput)?;
                bindings.insert(identity, fingerprint);
            }
        }
        Ok(bindings)
    }

    fn validate_facts<'schema>(
        &'schema self,
        selection: &Selection<'_>,
        rules: &[Rule],
        edge_visit_budget: usize,
    ) -> Result<(OntologyFactTypes<'schema>, usize), InferenceError> {
        let mut remaining = edge_visit_budget;
        let mut bound = BTreeSet::new();
        for rule in rules {
            for (namespace, version) in &rule.ontology_versions {
                bound.insert((namespace.clone(), version.clone()));
            }
        }
        let mut relations = BTreeMap::new();
        for identity in bound {
            let definition = self
                .definitions
                .get(&identity)
                .ok_or(InferenceError::InvalidInput)?;
            for concept in &definition.concepts {
                consume(&mut remaining, 1)?;
                if !selection.nodes.contains(concept) {
                    return Err(InferenceError::InvalidInput);
                }
            }
            for relation in &definition.relations {
                consume(&mut remaining, 1)?;
                let key = (definition.namespace.clone(), relation.name.clone());
                let schema = RelationSchema {
                    ontology: definition,
                    relation,
                };
                if relations.insert(key, schema).is_some_and(|previous| {
                    previous.ontology != definition || previous.relation != relation
                }) {
                    return Err(InferenceError::InvalidInput);
                }
            }
        }

        let mut memberships = BTreeMap::<NodeId, BTreeSet<NodeId>>::new();
        for premise in selection.premises {
            consume(&mut remaining, 1)?;
            let Some(schema) = relation_schema(&relations, &premise.edge.relation) else {
                continue;
            };
            if schema.ontology.instance_relation == schema.relation.name {
                if !schema.relation.object_range.contains(&premise.edge.target) {
                    return Err(InferenceError::InvalidInput);
                }
                memberships
                    .entry(premise.edge.source)
                    .or_default()
                    .insert(premise.edge.target);
            }
        }

        let facts = OntologyFactTypes {
            relations,
            memberships,
        };
        for premise in selection.premises {
            consume(&mut remaining, 1)?;
            let Some(schema) = relation_schema(&facts.relations, &premise.edge.relation) else {
                continue;
            };
            let is_instance_relation = schema.ontology.instance_relation == schema.relation.name;
            if (!schema.relation.subject_domain.is_empty()
                && !facts.has_any_type(
                    premise.edge.source,
                    &schema.relation.subject_domain,
                    &mut remaining,
                )?)
                || (!is_instance_relation
                    && !schema.relation.object_range.is_empty()
                    && !facts.has_any_type(
                        premise.edge.target,
                        &schema.relation.object_range,
                        &mut remaining,
                    )?)
            {
                return Err(InferenceError::InvalidInput);
            }
        }
        Ok((facts, remaining))
    }

    /// Verify explicit rule version bindings against available versions.
    ///
    /// # Errors
    /// Rejects unknown versions, undeclared relations/concepts, incompatible target ranges, zero bounds
    /// or exhausted rule/binding budgets.
    pub fn validate(
        &self,
        rules: &[Rule],
        rule_budget: usize,
        binding_budget: usize,
        byte_budget: usize,
    ) -> Result<(), InferenceError> {
        if rule_budget == 0 || binding_budget == 0 || rules.is_empty() {
            return Err(InferenceError::InvalidInput);
        }
        if rules.len() > rule_budget {
            return Err(InferenceError::BudgetExhausted);
        }
        crate::input_budget::validate_encoded(rules, byte_budget)?;
        let mut remaining = binding_budget;
        for rule in rules {
            for (namespace, version) in &rule.ontology_versions {
                remaining = remaining
                    .checked_sub(1)
                    .ok_or(InferenceError::BudgetExhausted)?;
                if !self
                    .versions
                    .get(namespace)
                    .is_some_and(|versions| versions.contains(version))
                {
                    return Err(InferenceError::InvalidInput);
                }
            }
        }
        if !self.definitions.is_empty() {
            self.validate_declared_rules(rules)?;
        }
        Ok(())
    }

    fn validate_declared_rules(&self, rules: &[Rule]) -> Result<(), InferenceError> {
        for rule in rules {
            self.validate_declared_relation(rule, &rule.path_relation)?;
            self.validate_declared_relation(rule, &rule.join_relation)?;
            let declaration = self.validate_declared_relation(rule, &rule.declaration_relation)?;
            let output = self.validate_declared_relation(rule, &rule.output_relation)?;
            let Some((declaration_ontology, declaration_schema)) = declaration else {
                return Err(InferenceError::InvalidInput);
            };
            if declaration_ontology.instance_relation != declaration_schema.name {
                return Err(InferenceError::InvalidInput);
            }
            let concept_is_declared = rule.ontology_versions.iter().any(|(namespace, version)| {
                self.definitions
                    .get(&(namespace.clone(), version.clone()))
                    .is_some_and(|definition| definition.concepts.contains(&rule.concept))
            });
            if !concept_is_declared {
                return Err(InferenceError::InvalidInput);
            }
            for (_, relation) in [declaration, output].into_iter().flatten() {
                if !relation.object_range.is_empty()
                    && !relation.object_range.contains(&rule.concept)
                {
                    return Err(InferenceError::InvalidInput);
                }
            }
        }
        Ok(())
    }

    fn validate_declared_relation(
        &self,
        rule: &Rule,
        relation: &RelationKind,
    ) -> Result<Option<(&OntologyDefinition, &OntologyRelationDefinition)>, InferenceError> {
        let RelationKind::External {
            namespace,
            relation,
        } = relation
        else {
            return Ok(None);
        };
        let Some(version) = rule.ontology_versions.get(namespace) else {
            return Err(InferenceError::InvalidInput);
        };
        let Some(definition) = self.definitions.get(&(namespace.clone(), version.clone())) else {
            return Err(InferenceError::InvalidInput);
        };
        let declared = definition
            .relations
            .iter()
            .find(|declared| declared.name == *relation)
            .ok_or(InferenceError::InvalidInput)?;
        Ok(Some((definition, declared)))
    }
}

fn definition_entry_count(definition: &OntologyDefinition) -> Result<usize, InferenceError> {
    definition
        .concepts
        .len()
        .checked_add(
            definition
                .relations
                .iter()
                .try_fold(0_usize, |total, relation| {
                    total
                        .checked_add(1)
                        .and_then(|value| value.checked_add(relation.subject_domain.len()))
                        .and_then(|value| value.checked_add(relation.object_range.len()))
                        .ok_or(InferenceError::BudgetExhausted)
                })?,
        )
        .ok_or(InferenceError::BudgetExhausted)
}

fn constraint_widens(previous: &[NodeId], next: &[NodeId]) -> bool {
    if previous.is_empty() {
        return next.is_empty();
    }
    if next.is_empty() {
        return true;
    }
    let previous = previous.iter().copied().collect::<BTreeSet<_>>();
    let next = next.iter().copied().collect::<BTreeSet<_>>();
    previous.is_subset(&next)
}

/// Evaluate only rules whose explicit ontology versions are registered.
///
/// # Errors
/// Rejects unknown bindings, invalid retained fact typing, incompatible
/// declared ranges or exhausted validation/inference budgets.
pub fn evaluate_registered(
    selection: &Selection<'_>,
    rules: &[Rule],
    registry: &OntologyRegistry,
    limits: &Limits,
    rule_budget: usize,
    binding_budget: usize,
) -> Result<Vec<Claim>, InferenceError> {
    let bound = registry.bind_rules(rules, rule_budget, binding_budget, limits.input_bytes)?;
    registry.evaluate_bound(selection, &bound, limits, rule_budget, binding_budget)
}

#[cfg(test)]
mod tests;
