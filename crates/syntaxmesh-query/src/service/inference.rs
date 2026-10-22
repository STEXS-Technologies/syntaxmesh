use crate::{Query, QueryError};
use std::collections::BTreeSet;
use syntaxmesh_core::{FactPayload, FactRef, FactVersionRef};
use syntaxmesh_inference::{
    BoundRuleSet, Claim, Contract, ContractEvaluation, ContractLimits, DiscoveryRange, Limits,
    OntologyRegistry, Premise, RegisteredDiscoveryBudgets, Rule, Selection,
};
use syntaxmesh_store::GraphStore;

mod history_page;
mod invalidation;
mod witness;

#[cfg(test)]
mod tests;

impl<S: GraphStore + ?Sized> Query<'_, S> {
    /// Verify schema bindings before loading, then type-check retained facts.
    ///
    /// # Errors
    /// Rejects unknown versions, invalid fact typing and exhausted budgets.
    pub fn infer_registered(
        &self,
        rules: &[Rule],
        registry: &OntologyRegistry,
        limits: &Limits,
        history_scan_budget: usize,
        rule_budget: usize,
        binding_budget: usize,
    ) -> Result<Vec<Claim>, QueryError> {
        let bound_rules = registry
            .bind_rules(rules, rule_budget, binding_budget, limits.input_bytes)
            .map_err(|error| QueryError::Context(error.to_string()))?;
        self.inference_evaluate(
            bound_rules.rules(),
            limits,
            history_scan_budget,
            rule_budget,
            None,
            Some((registry, &bound_rules, binding_budget)),
        )
    }

    /// Evaluate an already bound rule set without rebinding ontology definitions.
    ///
    /// # Errors
    /// Rejects a rule set bound to another registry, invalid retained fact typing
    /// or exhausted history/evaluation bounds.
    pub fn infer_bound(
        &self,
        registry: &OntologyRegistry,
        bound_rules: &BoundRuleSet,
        limits: &Limits,
        history_scan_budget: usize,
        rule_budget: usize,
        binding_budget: usize,
    ) -> Result<Vec<Claim>, QueryError> {
        registry
            .validate_bound_identity(bound_rules, rule_budget, binding_budget, limits.input_bytes)
            .map_err(|error| QueryError::Context(error.to_string()))?;
        self.inference_evaluate(
            bound_rules.rules(),
            limits,
            history_scan_budget,
            rule_budget,
            None,
            Some((registry, bound_rules, binding_budget)),
        )
    }
    /// Evaluate rules using authoritative active versions at this selection.
    ///
    /// # Errors
    /// Rejects exhausted history/evaluation bounds and malformed pinned pages.
    pub fn infer(
        &self,
        rule: &Rule,
        limits: &Limits,
        history_scan_budget: usize,
    ) -> Result<Vec<Claim>, QueryError> {
        self.infer_rules(std::slice::from_ref(rule), limits, history_scan_budget, 1)
    }

    /// Evaluate explicit relation contracts over this query's pinned generation.
    ///
    /// The history scan bound limits canonical fact versions loaded; contract
    /// count, bytes, relation visits, and result count use independent limits.
    /// Missing relations are reported as insufficient evidence.
    ///
    /// # Errors
    /// Rejects malformed retained history, invalid contracts, or exhausted bounds.
    pub fn evaluate_contracts(
        &self,
        contracts: &[Contract],
        contract_limits: &ContractLimits,
        history_scan_budget: usize,
    ) -> Result<Vec<ContractEvaluation>, QueryError> {
        if history_scan_budget == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let (repository, worktree, nodes, premises) = self.load_inference_selection(
            contract_limits.input_bytes,
            history_scan_budget,
            history_scan_budget,
        )?;
        let selection = Selection {
            repository,
            worktree,
            generation: self.generation,
            nodes: &nodes,
            premises: &premises,
        };
        syntaxmesh_inference::evaluate_contracts(&selection, contracts, contract_limits)
            .map_err(|error| QueryError::Context(error.to_string()))
    }

    /// Evaluate a bounded rule set over one authoritative retained selection.
    ///
    /// # Errors
    /// Rejects malformed rules/pages or exhausted shared history/evaluation limits.
    pub fn infer_rules(
        &self,
        rules: &[Rule],
        limits: &Limits,
        history_scan_budget: usize,
        rule_budget: usize,
    ) -> Result<Vec<Claim>, QueryError> {
        self.inference_evaluate(rules, limits, history_scan_budget, rule_budget, None, None)
    }

    /// Discover witnesses enabled by active exact edge versions opened in a range.
    /// This is a delta, not a complete materialized claim selection.
    ///
    /// # Errors
    /// Rejects reversed/foreign/malformed ranges and exhausted range/history/work bounds.
    pub fn infer_discovered_rules(
        &self,
        range: &DiscoveryRange,
        rules: &[Rule],
        limits: &Limits,
        history_scan_budget: usize,
        rule_budget: usize,
    ) -> Result<Vec<Claim>, QueryError> {
        if history_scan_budget == 0 || rule_budget == 0 || rules.is_empty() {
            return Err(QueryError::InvalidLimit);
        }
        let transitions =
            self.inference_premise_transitions(range.from, range.generations, range.versions)?;
        let seeds = transitions
            .opened
            .into_iter()
            .filter(|reference| {
                matches!(reference.fact, FactRef::Edge(_))
                    && !transitions.closed.contains_key(reference)
            })
            .collect::<Vec<_>>();
        self.inference_evaluate(
            rules,
            limits,
            history_scan_budget,
            rule_budget,
            Some(&seeds),
            None,
        )
    }

    /// Validate all selected schema facts and discover only witnesses containing
    /// exact opened premise versions.
    ///
    /// # Errors
    /// Rejects invalid schema facts, malformed ranges and exhausted bounds.
    pub fn infer_discovered_bound_rules(
        &self,
        range: &DiscoveryRange,
        registry: &OntologyRegistry,
        bound_rules: &BoundRuleSet,
        limits: &Limits,
        budgets: RegisteredDiscoveryBudgets,
    ) -> Result<Vec<Claim>, QueryError> {
        if budgets.history_scan == 0 || budgets.rules == 0 {
            return Err(QueryError::InvalidLimit);
        }
        registry
            .validate_bound_identity(
                bound_rules,
                budgets.rules,
                budgets.ontology_bindings,
                limits.input_bytes,
            )
            .map_err(|error| QueryError::Context(error.to_string()))?;
        let transitions =
            self.inference_premise_transitions(range.from, range.generations, range.versions)?;
        let seeds = transitions
            .opened
            .into_iter()
            .filter(|reference| {
                matches!(reference.fact, FactRef::Edge(_))
                    && !transitions.closed.contains_key(reference)
            })
            .collect::<Vec<_>>();
        self.inference_evaluate(
            bound_rules.rules(),
            limits,
            budgets.history_scan,
            budgets.rules,
            Some(&seeds),
            Some((registry, bound_rules, budgets.ontology_bindings)),
        )
    }

    fn inference_evaluate(
        &self,
        rules: &[Rule],
        limits: &Limits,
        history_scan_budget: usize,
        rule_budget: usize,
        seeds: Option<&[FactVersionRef]>,
        registered: Option<(&OntologyRegistry, &BoundRuleSet, usize)>,
    ) -> Result<Vec<Claim>, QueryError> {
        if history_scan_budget == 0 || rule_budget == 0 || rules.is_empty() {
            return Err(QueryError::InvalidLimit);
        }
        if rules.len() > rule_budget {
            return Err(QueryError::Context(
                "inference rule budget exhausted".to_owned(),
            ));
        }
        let (repository, worktree, nodes, premises) =
            self.load_inference_selection(limits.input_bytes, limits.facts, history_scan_budget)?;
        let selection = Selection {
            repository,
            worktree,
            generation: self.generation,
            nodes: &nodes,
            premises: &premises,
        };
        let evaluation = if let Some((registry, bound_rules, binding_budget)) = registered {
            seeds.map_or_else(
                || {
                    registry.evaluate_bound(
                        &selection,
                        bound_rules,
                        limits,
                        rule_budget,
                        binding_budget,
                    )
                },
                |seeds| {
                    registry.evaluate_bound_discovered(
                        &selection,
                        bound_rules,
                        seeds,
                        limits,
                        rule_budget,
                        binding_budget,
                    )
                },
            )
        } else {
            seeds.map_or_else(
                || syntaxmesh_inference::evaluate_rules(&selection, rules, limits, rule_budget),
                |opened| {
                    syntaxmesh_inference::discover_rules(
                        &selection,
                        rules,
                        opened,
                        limits,
                        rule_budget,
                    )
                },
            )
        };
        evaluation.map_err(|error| QueryError::Context(error.to_string()))
    }

    fn load_inference_selection(
        &self,
        input_byte_budget: usize,
        fact_budget: usize,
        history_scan_budget: usize,
    ) -> Result<
        (
            syntaxmesh_core::RepositoryId,
            syntaxmesh_core::WorktreeId,
            BTreeSet<syntaxmesh_core::NodeId>,
            Vec<Premise>,
        ),
        QueryError,
    > {
        if history_scan_budget == 0 || input_byte_budget == 0 || fact_budget == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let manifest = self.manifest()?;
        let sequence = self.store.generation_sequence(self.generation)?;
        let mut nodes = BTreeSet::new();
        let mut premises = Vec::new();
        let mut cursor = None;
        let mut previous = None;
        let mut remaining = history_scan_budget;
        loop {
            if remaining == 0 {
                return Err(QueryError::Context(
                    "inference history budget exhausted".to_owned(),
                ));
            }
            let limit = remaining.min(100);
            let page = self
                .store
                .fact_history_page(self.generation, cursor, limit)?;
            previous = history_page::validate_page(
                &page,
                self.generation,
                sequence,
                limit,
                cursor,
                previous,
            )?;
            for entry in &page.items {
                remaining = remaining.checked_sub(1).ok_or(QueryError::InvalidLimit)?;
                if entry.valid_until_sequence.is_some() {
                    continue;
                }
                match &entry.version.payload {
                    FactPayload::Node(node) => {
                        if entry.fact != FactRef::Node(node.id) || !nodes.insert(node.id) {
                            return Err(QueryError::InvalidLimit);
                        }
                    }
                    FactPayload::Edge(edge) => {
                        if entry.fact != FactRef::Edge(edge.id) {
                            return Err(QueryError::InvalidLimit);
                        }
                        premises.push(Premise {
                            generation: self.generation,
                            valid_from: entry.version.valid_from,
                            edge: edge.clone(),
                        });
                    }
                    FactPayload::File(_) | FactPayload::Provenance(_) => {}
                }
                if nodes
                    .len()
                    .checked_add(premises.len())
                    .is_none_or(|count| count > fact_budget)
                {
                    return Err(QueryError::Context(
                        "inference fact budget exhausted".to_owned(),
                    ));
                }
            }
            let Some(next) = page.next_cursor else {
                break;
            };
            cursor = Some(next);
        }
        Ok((manifest.repository, manifest.worktree, nodes, premises))
    }
}
