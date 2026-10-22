use super::*;
use crate::evaluator::tests::{fixture, selection};
use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};

#[test]
fn operation_identity_binds_scope_generation_and_rules_not_execution_limits()
-> Result<(), InferenceError> {
    let (nodes, premises, rule, _limits) = fixture();
    let selected = selection(&nodes, &premises);
    let scope = ClaimSelection {
        repository: selected.repository,
        worktree: selected.worktree,
        generation: selected.generation,
    };
    let rules = [rule.clone()];
    let original = materialization_identity(scope, &rules, 1, 16_384)?;
    if materialization_identity(scope, &rules, 2, 32_768)? != original {
        return Err(InferenceError::InvalidInput);
    }
    for altered in [
        ClaimSelection {
            repository: RepositoryId::derive(&[b"foreign"]),
            ..scope
        },
        ClaimSelection {
            worktree: WorktreeId::derive(&[b"foreign"]),
            ..scope
        },
        ClaimSelection {
            generation: GenerationId::derive(&[b"next"]),
            ..scope
        },
    ] {
        if materialization_identity(altered, &rules, 1, 16_384)? == original {
            return Err(InferenceError::InvalidInput);
        }
    }
    let mut changed = rule;
    changed.version = "2".to_owned();
    if materialization_identity(scope, &[changed], 1, 16_384)? == original
        || materialization_identity(scope, &rules, 0, 16_384).is_ok()
        || materialization_identity(scope, &rules, 1, 1).is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}
