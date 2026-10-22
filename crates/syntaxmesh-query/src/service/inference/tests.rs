use super::*;
use std::error::Error;
use syntaxmesh_core::{GenerationId, NodeId, RelationKind};
use syntaxmesh_inference::{ContractOrigin, ContractOutcome, RelationExpectation};
use syntaxmesh_store::{FileGraphStore, InMemoryGraphStore};

mod fixture;
mod invalidation_feed;
use fixture::{fixture, limits, verify};

#[test]
fn contract_evaluation_uses_exact_facts_from_the_pinned_generation() -> Result<(), Box<dyn Error>> {
    let mut store = InMemoryGraphStore::new();
    let (_rule, retained, retracted) = fixture(&mut store)?;
    let entry = NodeId::derive(&[b"entry"]);
    let writer = NodeId::derive(&[b"writer"]);
    let target = NodeId::derive(&[b"target"]);
    let concept = NodeId::derive(&[b"concept"]);
    let contracts = [
        Contract {
            id: "entry-calls-writer".to_owned(),
            version: "1".to_owned(),
            origin: ContractOrigin::Declared,
            source: entry,
            target: writer,
            relation: RelationKind::Calls,
            expectation: RelationExpectation::Required,
        },
        Contract {
            id: "target-not-in-concept".to_owned(),
            version: "1".to_owned(),
            origin: ContractOrigin::Declared,
            source: target,
            target: concept,
            relation: RelationKind::External {
                namespace: "example".to_owned(),
                relation: "instance_of".to_owned(),
            },
            expectation: RelationExpectation::Forbidden,
        },
    ];
    let contract_limits = ContractLimits {
        contracts: 2,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 2,
    };
    let old = Query::new(&store, retained).evaluate_contracts(&contracts, &contract_limits, 100)?;
    let current =
        Query::new(&store, retracted).evaluate_contracts(&contracts, &contract_limits, 100)?;
    let required_old = old.first().ok_or("missing required-contract result")?;
    let forbidden_old = old.get(1).ok_or("missing forbidden-contract result")?;
    let required_current = current
        .first()
        .ok_or("missing current required-contract result")?;
    let forbidden_current = current
        .get(1)
        .ok_or("missing current forbidden-contract result")?;
    if required_old.outcome != ContractOutcome::Supported
        || required_old.generation != retained
        || required_old.evidence.map(|version| version.valid_from)
            != Some(GenerationId::derive(&[b"first"]))
        || forbidden_old.outcome != ContractOutcome::Contradicted
        || forbidden_old.evidence.map(|version| version.valid_from)
            != Some(GenerationId::derive(&[b"declared"]))
        || required_current.outcome != ContractOutcome::Supported
        || required_current.generation != retracted
        || forbidden_current.outcome != ContractOutcome::InsufficientEvidence
        || forbidden_current.evidence.is_some()
    {
        return Err("contract query lost exact generation-scoped evidence".into());
    }
    if Query::new(&store, retained)
        .evaluate_contracts(&contracts, &contract_limits, 1)
        .is_ok()
    {
        return Err("contract query exceeded the history scan budget".into());
    }
    Ok(())
}

#[test]
fn malformed_history_pages_reject_before_inference_selection() -> Result<(), Box<dyn Error>> {
    use syntaxmesh_core::{FactRef, GenerationId, NodeId};
    use syntaxmesh_store::FactHistoryCursor;
    let mut store = InMemoryGraphStore::new();
    let (_rule, retained, _current) = fixture(&mut store)?;
    let original = store.fact_history_page(retained, None, 100)?;
    let sequence = store.generation_sequence(retained)?;
    let validate = |page: &syntaxmesh_store::FactHistoryPage, limit| {
        super::history_page::validate_page(page, retained, sequence, limit, None, None)
    };
    validate(&original, 100)?;
    for case in 0..8 {
        let mut changed = original.clone();
        let first = changed.items.first_mut().ok_or("empty history fixture")?;
        match case {
            0 => first.fact = FactRef::Node(NodeId::derive(&[b"foreign"])),
            1 => first.valid_from_sequence = 0,
            2 => first.valid_from_sequence = sequence.saturating_add(1),
            3 => first.valid_until_sequence = Some(first.valid_from_sequence),
            4 => first.version.valid_until = Some(retained),
            5 => changed.items.reverse(),
            6 => {
                changed.next_cursor = Some(FactHistoryCursor {
                    as_of_generation: GenerationId::derive(&[b"foreign"]),
                    after_fact: first.fact,
                    after_valid_from: first.version.valid_from,
                });
            }
            _ => {
                let duplicate = first.clone();
                changed.items.insert(0, duplicate);
            }
        }
        if validate(&changed, 100).is_ok() {
            return Err("malformed history page accepted".into());
        }
    }
    if validate(&original, 1).is_ok() || validate(&original, 0).is_ok() {
        return Err("oversized or zero-budget page accepted".into());
    }
    let first = original.items.first().ok_or("empty history fixture")?;
    let empty_continuation = syntaxmesh_store::FactHistoryPage {
        items: Vec::new(),
        next_cursor: Some(FactHistoryCursor {
            as_of_generation: retained,
            after_fact: first.fact,
            after_valid_from: first.version.valid_from,
        }),
    };
    if validate(&empty_continuation, 100).is_ok()
        || super::history_page::validate_page(
            &original,
            retained,
            sequence,
            100,
            None,
            Some((first.fact, first.valid_from_sequence)),
        )
        .is_ok()
    {
        return Err("empty or repeated continuation accepted".into());
    }
    Ok(())
}

#[test]
fn in_memory_inference_preserves_actual_versions_across_retraction() -> Result<(), Box<dyn Error>> {
    let mut store = InMemoryGraphStore::new();
    let (rule, retained, current) = fixture(&mut store)?;
    verify(&store, &rule, retained, current)?;
    fixture::verify_restoration(&mut store, &rule, retained, current)
}

#[test]
fn file_inference_preserves_retained_witnesses_after_reopen() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.snapshot");
    let mut store = FileGraphStore::open(&path)?;
    let (rule, retained, current) = fixture(&mut store)?;
    let before = Query::new(&store, retained).infer(&rule, &limits(), 100)?;
    drop(store);
    let mut reopened = FileGraphStore::open(path)?;
    if Query::new(&reopened, retained).infer(&rule, &limits(), 100)? != before {
        return Err("retained derivation identity changed on reopen".into());
    }
    verify(&reopened, &rule, retained, current)?;
    fixture::verify_restoration(&mut reopened, &rule, retained, current)
}
