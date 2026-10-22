use super::*;
use std::error::Error;
use syntaxmesh_store::{FileGraphStore, InMemoryGraphStore};

mod fixture;
mod invalidation_feed;
use fixture::{fixture, limits, verify};

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
