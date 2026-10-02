use std::error::Error;
use std::fs;

use syntaxmesh_core::{NodeKind, RelationKind};
use syntaxmesh_store::GraphStore;

#[path = "support/semantic_store.rs"]
mod store_host;
use store_host::StoreHost;

const LOCAL: &str = "fn outer() { fn inner() { utility(); } inner(); } fn utility() {}";
const SCOPED_IMPL: &str = "trait Contract { fn run(); } fn outer() { struct Local; impl Contract for Local { fn run() { utility(); } } } fn utility() {}";
const SCOPED_IMPL_EDIT: &str = "trait Contract { fn run(); } fn added() { struct Local; impl Contract for Local { fn run() {} } } fn outer() { struct Local; impl Contract for Local { fn run() { missing(); } } } fn utility() {}";

#[test]
fn file_scoped_impl_identity_and_calls_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::File,
        SCOPED_IMPL,
        SCOPED_IMPL_EDIT,
        (1, 0, 1, 1),
        (0, 1, 0, 2),
    )
}

#[test]
fn verified_turso_scoped_impl_identity_and_calls_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::TursoVerified,
        SCOPED_IMPL,
        SCOPED_IMPL_EDIT,
        (1, 0, 1, 1),
        (0, 1, 0, 2),
    )
}
const LOCAL_EDIT: &str = "fn outer() { fn inner() { missing(); } inner(); } fn utility() {}";

const LOCAL_MODULE: &str =
    "fn outer() { mod local { use crate::utility; fn inner() { utility(); } } } fn utility() {}";
const LOCAL_MODULE_EDIT: &str =
    "fn outer() { mod local { use crate::missing; fn inner() { missing(); } } } fn utility() {}";

#[test]
fn file_local_module_imports_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::File,
        LOCAL_MODULE,
        LOCAL_MODULE_EDIT,
        (1, 0, 1, 0),
        (0, 1, 0, 0),
    )
}

#[test]
fn verified_turso_local_module_imports_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::TursoVerified,
        LOCAL_MODULE,
        LOCAL_MODULE_EDIT,
        (1, 0, 1, 0),
        (0, 1, 0, 0),
    )
}

#[test]
fn file_local_function_calls_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::File,
        LOCAL,
        LOCAL_EDIT,
        (2, 0, 2, 0),
        (1, 1, 1, 0),
    )
}

#[test]
fn verified_turso_local_function_calls_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::TursoVerified,
        LOCAL,
        LOCAL_EDIT,
        (2, 0, 2, 0),
        (1, 1, 1, 0),
    )
}

const IMPLEMENTATION: &str = "trait Contract {} struct Service; impl Contract for Service { fn run() { utility(); } } fn utility() {}";
const INHERENT: &str =
    "trait Contract {} struct Service; impl Service { fn run() { utility(); } } fn utility() {}";

#[test]
fn file_trait_implementation_retraction_retains_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::File,
        IMPLEMENTATION,
        INHERENT,
        (1, 0, 1, 1),
        (1, 0, 1, 0),
    )
}

#[test]
fn verified_turso_trait_implementation_retraction_retains_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::TursoVerified,
        IMPLEMENTATION,
        INHERENT,
        (1, 0, 1, 1),
        (1, 0, 1, 0),
    )
}

const QUALIFIED: &str = "struct Service; trait Contract {} impl Service { fn build() {} } fn helper() {} fn caller() { helper(); Service::build(); <Service as Contract>::helper(); <Service>::helper(); ::helper(); }";
const BARE: &str = "struct Service; trait Contract {} impl Service { fn build() {} } fn helper() {} fn caller() { helper(); Service::build(); helper(); helper(); helper(); }";
const RECEIVERS: &str = "struct Service; impl Service { fn run(&self) { self.other(); client.build(); } fn other(&self) {} } fn build() {} fn caller() { client.build(); build(); }";
const KNOWN_CALLS: &str = "struct Service; impl Service { fn run(&self) { self.other(); build(); } fn other(&self) {} } fn build() {} fn caller() { build(); build(); }";
const TRAITS: &str = "trait Service { fn required(&self); fn run(&self) { utility(); self.required(); } } fn utility() {}";
const TRAIT_EDIT: &str =
    "trait Service { fn required(&self); fn run(&self) { utility(); utility(); } } fn utility() {}";

#[test]
fn file_trait_default_calls_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::File,
        TRAITS,
        TRAIT_EDIT,
        (1, 1, 1, 0),
        (2, 0, 1, 0),
    )
}

#[test]
fn verified_turso_trait_default_calls_retain_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::TursoVerified,
        TRAITS,
        TRAIT_EDIT,
        (1, 1, 1, 0),
        (2, 0, 1, 0),
    )
}

#[test]
fn file_preserves_rust_call_qualification_and_history() -> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::File, QUALIFIED, BARE, (2, 3, 2, 0), (5, 0, 2, 0))
}

#[test]
fn verified_turso_preserves_rust_call_qualification_and_history() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::TursoVerified,
        QUALIFIED,
        BARE,
        (2, 3, 2, 0),
        (5, 0, 2, 0),
    )
}

#[test]
fn file_unknown_rust_receivers_do_not_bind_free_functions() -> Result<(), Box<dyn Error>> {
    lifecycle(
        StoreHost::File,
        RECEIVERS,
        KNOWN_CALLS,
        (2, 2, 2, 0),
        (4, 0, 3, 0),
    )
}

#[test]
fn verified_turso_unknown_rust_receivers_do_not_bind_free_functions() -> Result<(), Box<dyn Error>>
{
    lifecycle(
        StoreHost::TursoVerified,
        RECEIVERS,
        KNOWN_CALLS,
        (2, 2, 2, 0),
        (4, 0, 3, 0),
    )
}

fn require_resolution(
    store: &dyn GraphStore,
    generation: syntaxmesh_core::GenerationId,
    expected: (usize, usize, usize, usize),
) -> Result<(), Box<dyn Error>> {
    let (resolved, unresolved, calls, implementations) = expected;
    let snapshot = store.historical_snapshot(generation)?;
    if let Some(child) = snapshot
        .nodes
        .iter()
        .find(|node| node.name == "outer::inner")
    {
        let owner = snapshot
            .nodes
            .iter()
            .find(|node| node.name == "outer")
            .ok_or_else(|| std::io::Error::other("missing local function owner"))?;
        if !snapshot.edges.iter().any(|edge| {
            edge.source == owner.id
                && edge.target == child.id
                && edge.relation == RelationKind::Contains
        }) {
            return Err(std::io::Error::other("missing local function containment").into());
        }
    }
    let actual_calls = snapshot
        .edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::Calls)
        .count();
    let actual_implementations = snapshot
        .edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::Implements)
        .count();
    let nodes = snapshot.nodes;
    let actual_resolved = nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::Reference {
                    relation: RelationKind::Calls
                }
            )
        })
        .count();
    let actual_unresolved = nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::UnresolvedReference {
                    relation: RelationKind::Calls
                }
            )
        })
        .count();
    // Direct semantic edges collapse repeated calls to the same target; the
    // occurrence nodes retain all independently evidenced call sites.
    if actual_calls != calls
        || actual_resolved != resolved
        || actual_unresolved != unresolved
        || actual_implementations != implementations
    {
        return Err(std::io::Error::other(format!(
            "Rust call resolution differs: edges={actual_calls}, resolved={actual_resolved}, unresolved={actual_unresolved}"
        ))
        .into());
    }
    Ok(())
}

fn lifecycle(
    host: StoreHost,
    before: &str,
    after: &str,
    first_resolution: (usize, usize, usize, usize),
    current_resolution: (usize, usize, usize, usize),
) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    host.prepare(root)?;
    let source = root.join("lib.rs");
    fs::write(&source, before)?;
    let initial = host.run(root, None, &[])?;
    if !initial.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&initial.stderr)).into());
    }
    let initial_store = host.open(root)?;
    let first = host.latest(initial_store.as_ref())?.generation;
    require_resolution(initial_store.as_ref(), first, first_resolution)?;
    let retained = initial_store.historical_snapshot(first)?;
    drop(initial_store);

    let repeated = host.run(root, None, &[])?;
    if !repeated.status.success()
        || !String::from_utf8_lossy(&repeated.stdout).contains("already up to date")
    {
        return Err(std::io::Error::other("unchanged Rust call paths were not cached").into());
    }
    fs::write(&source, after)?;
    let edited = host.run(root, None, &[])?;
    if !edited.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&edited.stderr)).into());
    }
    let reopened = host.open(root)?;
    let current = host.latest(reopened.as_ref())?.generation;
    if current == first {
        return Err(std::io::Error::other("Rust call edit did not publish").into());
    }
    require_resolution(reopened.as_ref(), current, current_resolution)?;
    let current_snapshot = reopened.historical_snapshot(current)?;
    for edge in &retained.edges {
        if edge.relation == RelationKind::Contains
            && retained
                .nodes
                .iter()
                .any(|node| node.id == edge.source && node.name == "outer")
            && retained.nodes.iter().any(|node| {
                node.id == edge.target
                    && matches!(&node.kind, NodeKind::External { namespace, kind }
                    if namespace == "syntaxmesh.lang.rust" && kind == "implementation")
            })
            && !current_snapshot.edges.iter().any(|current_edge| {
                current_edge.id == edge.id
                    && current_edge.source == edge.source
                    && current_edge.target == edge.target
            })
        {
            return Err(std::io::Error::other(
                "unrelated declaration changed scoped impl identity",
            )
            .into());
        }
    }
    require_resolution(reopened.as_ref(), first, first_resolution)?;
    if reopened.historical_snapshot(first)? != retained {
        return Err(std::io::Error::other("Rust call edit changed retained evidence").into());
    }
    Ok(())
}
