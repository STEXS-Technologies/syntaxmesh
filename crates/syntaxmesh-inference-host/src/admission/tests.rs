use super::*;
use syntaxmesh_api_model::{InferencePageCursor, InferenceSelection};
use syntaxmesh_core::StableId;
use syntaxmesh_inference::ClaimSelection;
use syntaxmesh_inference_sqlite::Watermark;

fn origin() -> StreamOrigin {
    StreamOrigin {
        stream: StableId::derive("test-stream", &[b"stream"]),
        baseline: Watermark {
            operation: StableId::derive("test-operation", &[b"operation"]),
            selection: ClaimSelection {
                repository: RepositoryId::derive(&[b"repository"]),
                worktree: WorktreeId::derive(&[b"worktree"]),
                generation: GenerationId::derive(&[b"baseline"]),
            },
            generation_sequence: 1,
            configuration: StableId::derive("test-config", &[b"config"]),
            claim_count: 2,
        },
    }
}

fn limits() -> InferencePageLimits {
    InferencePageLimits {
        claims: 10,
        payload_bytes: 4096,
        premises: 100,
        tree_pages: 64,
    }
}

fn registry() -> Result<InferenceReadBindings, String> {
    let receipt = origin();
    let selection = receipt.baseline.selection;
    let mut registry =
        InferenceReadBindings::new(selection.repository, selection.worktree, limits())?;
    for identifier in ["rules", "alias"] {
        registry.insert(
            identifier.to_owned(),
            PathBuf::from("not-opened.sqlite"),
            receipt,
        )?;
    }
    Ok(registry)
}

fn request() -> InferencePageRequest {
    InferencePageRequest {
        schema_version: INFERENCE_READ_SCHEMA_VERSION,
        binding: "rules".to_owned(),
        generation: GenerationId::derive(&[b"selected"]),
        limits: limits(),
        cursor: None,
    }
}

#[test]
fn admits_exact_limits_without_opening_database() -> Result<(), String> {
    let registry = registry()?;
    let request = request();
    let admitted = registry.page(&request)?;
    if admitted.database != Path::new("not-opened.sqlite")
        || admitted.request.origin != origin()
        || admitted.request.generation != request.generation
        || admitted.request.claim_budget != 10
        || admitted.request.byte_budget != 4096
        || admitted.request.premise_budget != 100
        || admitted.request.page_budget != 64
    {
        return Err("admission changed independent limits or operator selection".to_owned());
    }
    Ok(())
}

#[test]
fn rejects_unknown_version_binding_and_each_invalid_budget() -> Result<(), String> {
    let registry = registry()?;
    let mut invalid_version = request();
    invalid_version.schema_version = INFERENCE_READ_SCHEMA_VERSION + 1;
    if registry.page(&invalid_version).is_ok() {
        return Err("unsupported version admitted".to_owned());
    }
    let mut unknown = request();
    unknown.binding = "unknown".to_owned();
    if registry.page(&unknown).is_ok() {
        return Err("unknown binding admitted".to_owned());
    }
    for invalid_limits in [
        InferencePageLimits {
            claims: 0,
            ..limits()
        },
        InferencePageLimits {
            payload_bytes: 0,
            ..limits()
        },
        InferencePageLimits {
            premises: 0,
            ..limits()
        },
        InferencePageLimits {
            tree_pages: 0,
            ..limits()
        },
        InferencePageLimits {
            claims: 11,
            ..limits()
        },
        InferencePageLimits {
            payload_bytes: 4097,
            ..limits()
        },
        InferencePageLimits {
            premises: 101,
            ..limits()
        },
        InferencePageLimits {
            tree_pages: 65,
            ..limits()
        },
    ] {
        let invalid = InferencePageRequest {
            limits: invalid_limits,
            ..request()
        };
        if registry.page(&invalid).is_ok() {
            return Err("out-of-policy independent budget admitted".to_owned());
        }
    }
    Ok(())
}

#[test]
fn rejects_cross_alias_cursor_and_foreign_operator_scope() -> Result<(), String> {
    let mut registry = registry()?;
    let mut page = request();
    let receipt = origin();
    page.cursor = Some(InferencePageCursor {
        binding: "alias".to_owned(),
        stream: receipt.stream,
        selection: InferenceSelection {
            repository: receipt.baseline.selection.repository,
            worktree: receipt.baseline.selection.worktree,
            generation: page.generation,
        },
        root: None,
        after: StableId::derive("test-after", &[b"after"]),
    });
    if registry.page(&page).is_ok() {
        return Err("cross-alias cursor admitted".to_owned());
    }
    let cursor = page.cursor.as_mut().ok_or("test cursor missing")?;
    cursor.binding = "rules".to_owned();
    registry.page(&page)?;
    for field in 0..4 {
        let mut substituted = page.clone();
        let substituted_cursor = substituted.cursor.as_mut().ok_or("test cursor missing")?;
        match field {
            0 => substituted_cursor.stream = StableId::derive("test-stream", &[b"foreign"]),
            1 => substituted_cursor.selection.repository = RepositoryId::derive(&[b"foreign"]),
            2 => substituted_cursor.selection.worktree = WorktreeId::derive(&[b"foreign"]),
            _ => substituted_cursor.selection.generation = GenerationId::derive(&[b"foreign"]),
        }
        if registry.page(&substituted).is_ok() {
            return Err("foreign cursor coordinate admitted".to_owned());
        }
    }
    let mut foreign = receipt;
    foreign.baseline.selection.worktree = WorktreeId::derive(&[b"foreign"]);
    if registry
        .insert(
            "foreign".to_owned(),
            PathBuf::from("foreign.sqlite"),
            foreign,
        )
        .is_ok()
        || registry
            .insert(
                "rules".to_owned(),
                PathBuf::from("replacement.sqlite"),
                receipt,
            )
            .is_ok()
    {
        return Err("foreign or duplicate binding admitted".to_owned());
    }
    Ok(())
}
