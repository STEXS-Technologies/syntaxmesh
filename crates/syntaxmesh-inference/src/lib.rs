//! Optional deterministic reasoning; never mutates canonical facts.

mod dependency_index;
mod discovery;
mod evaluator;
mod input_budget;
mod model;
mod ontology;
mod operation_identity;
mod rule_set;
mod witness;

pub use dependency_index::PremiseDependencyIndex;
pub use discovery::discover_rules;
pub use evaluator::evaluate;
pub use model::{
    Claim, ClaimSelection, DerivationClass, DiscoveryRange, InferenceError, InvalidationLimits,
    Limits, Premise, PremiseClosure, RegisteredDiscoveryBudgets, Rule, Selection,
};
pub use ontology::{
    BoundRuleSet, ONTOLOGY_DEFINITION_SCHEMA_VERSION, OntologyDefinition, OntologyRegistry,
    OntologyRelationDefinition, RegisteredInferenceAdmission, evaluate_registered,
};
pub use operation_identity::materialization_identity;
pub use rule_set::{evaluate_rules, rule_set_identity};
pub use witness::verify_witness;
