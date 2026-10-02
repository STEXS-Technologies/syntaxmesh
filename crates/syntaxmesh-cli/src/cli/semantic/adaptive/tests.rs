use std::cell::RefCell;

use super::{PromptError, extract};
use syntaxmesh_semantic::{SemanticClaim, SemanticEvidence, SemanticOutput};

#[test]
fn successful_prompt_is_executed_once() {
    let calls = RefCell::new(Vec::new());
    let output = extract(&[1, 2, 3], &|items| {
        calls.borrow_mut().push(items.to_vec());
        Ok(SemanticOutput::default())
    });
    assert!(output.is_ok());
    assert_eq!(*calls.borrow(), vec![vec![1, 2, 3]]);
}

#[test]
fn truncation_splits_in_order_without_dropping_requests() {
    let calls = RefCell::new(Vec::new());
    let output = extract(&[1, 2, 3], &|items| {
        calls.borrow_mut().push(items.to_vec());
        if items.len() > 1 {
            Err(PromptError::Truncated)
        } else {
            Ok(SemanticOutput::default())
        }
    });
    assert!(output.is_ok());
    assert_eq!(
        *calls.borrow(),
        vec![vec![1, 2, 3], vec![1], vec![2, 3], vec![2], vec![3]]
    );
}

#[test]
fn ordinary_failures_and_singletons_do_not_fan_out() {
    for truncated in [false, true] {
        let calls = RefCell::new(0);
        let output = extract(&[1], &|_items| {
            *calls.borrow_mut() += 1;
            Err(if truncated {
                PromptError::Truncated
            } else {
                PromptError::Failed("invalid evidence".to_owned())
            })
        });
        assert!(output.is_err());
        assert_eq!(*calls.borrow(), 1);
    }
}

#[test]
fn split_depth_is_bounded() {
    let calls = RefCell::new(Vec::new());
    let output = extract(&[0; 22], &|items| {
        calls.borrow_mut().push(items.len());
        Err(PromptError::Truncated)
    });
    assert!(output.is_err());
    assert_eq!(
        *calls.borrow(),
        vec![22, 11, 5, 2, 3, 6, 3, 3, 11, 5, 2, 3, 6, 3, 3]
    );
}

#[test]
fn independent_results_preserve_successes_on_either_side_of_failure() {
    for failing in [1, 2] {
        let outputs = super::extract_many(&[1, 2], &|items| {
            if items.len() > 1 {
                Err(PromptError::Truncated)
            } else if items.first() == Some(&failing) {
                Err(PromptError::Failed("failed leaf".to_owned()))
            } else {
                Ok(vec![SemanticOutput::default()])
            }
        });
        assert_eq!(outputs.len(), 2);
        for (position, result) in [1, 2].into_iter().zip(outputs) {
            assert_eq!(result.is_err(), position == failing);
        }
    }
}

#[test]
fn incorrect_leaf_cardinality_rejects_all_affected_requests() {
    let outputs = super::extract_many(&[1, 2], &|_items| Ok(vec![SemanticOutput::default()]));
    assert_eq!(outputs.len(), 2);
    assert!(outputs.iter().all(Result::is_err));
}

#[test]
fn full_recovery_tree_executes_at_most_fifteen_prompts() {
    let calls = RefCell::new(0_usize);
    let recovered = RefCell::new(0_usize);
    let output = extract(&[0; 22], &|items| {
        *calls.borrow_mut() += 1;
        if items.len() > 3 {
            Err(PromptError::Truncated)
        } else {
            *recovered.borrow_mut() += items.len();
            Ok(SemanticOutput::default())
        }
    });
    assert!(output.is_ok());
    assert_eq!(*calls.borrow(), 15);
    assert_eq!(*recovered.borrow(), 22);
}

#[test]
fn malformed_multi_document_output_is_not_split() {
    let calls = RefCell::new(0_usize);
    let output = extract(&[1, 2, 3], &|_items| {
        *calls.borrow_mut() += 1;
        Err(PromptError::Failed("invalid JSON".to_owned()))
    });
    assert!(output.is_err_and(|message| message == "invalid JSON"));
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn equal_assertions_remain_independently_supported_until_ownership_validation() {
    let output = extract(&[1_u8, 2], &|items| {
        if items.len() > 1 {
            return Err(PromptError::Truncated);
        }
        let hash = items.first().copied().unwrap_or_default();
        Ok(SemanticOutput {
            claims: vec![SemanticClaim {
                subject: "engine".to_owned(),
                relation: "uses".to_owned(),
                object: "Penelope".to_owned(),
                evidence: vec![SemanticEvidence {
                    chunk_content_hash: [hash; 32],
                    quote: "Engine uses Penelope.".to_owned(),
                }],
            }],
        })
    });
    assert!(output.is_ok_and(|output| output.claims.len() == 2
        && output.claims.iter().all(|claim| claim.evidence.len() == 1)));
}
