use std::error::Error;
use std::fs;

use syntaxmesh_core::{NodeKind, RelationKind};

#[path = "support/semantic_store.rs"]
mod store_host;
use store_host::StoreHost;

#[test]
fn file_shadow_constraints_survive_unchanged_caller_reresolution() -> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::File)
}

#[test]
fn verified_turso_shadow_constraints_survive_unchanged_caller_reresolution()
-> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::TursoVerified)
}

fn lifecycle(host: StoreHost) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    host.prepare(root)?;
    fs::write(
        root.join("caller.rs"),
        "fn caller(helper: fn()) { helper(); } fn plain() { helper(); } fn constant() { helper(); const helper: fn() = target; helper(); }",
    )?;
    fs::write(root.join("definition.rs"), "fn helper() {}")?;
    let initial = host.run(root, None, &[])?;
    if !initial.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&initial.stderr)).into());
    }
    let store = host.open(root)?;
    let first = host.latest(store.as_ref())?.generation;
    let retained = store.historical_snapshot(first)?;
    let constrained = retained
        .nodes
        .iter()
        .filter(|node| {
            node.name == "helper"
                && matches!(
                    node.kind,
                    NodeKind::UnresolvedReference {
                        relation: RelationKind::Calls
                    }
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    if constrained.len() != 3
        || constrained.iter().any(|node| {
            node.extension_payload.as_ref().is_none_or(|payload| {
                payload.namespace != "syntaxmesh.reference-resolution"
                    || payload.schema_version != 1
                    || payload.bytes != [1]
            })
        })
    {
        return Err(std::io::Error::other("missing persisted binding policy").into());
    }
    if retained
        .edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::Calls)
        .count()
        != 1
    {
        return Err(std::io::Error::other("initial binding resolution differs").into());
    }
    drop(store);
    fs::write(root.join("definition.rs"), "fn helper() { missing(); }")?;
    let edited = host.run(root, None, &[])?;
    if !edited.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&edited.stderr)).into());
    }
    let reopened = host.open(root)?;
    let current = host.latest(reopened.as_ref())?.generation;
    let snapshot = reopened.historical_snapshot(current)?;
    if current == first
        || constrained.iter().any(|node| {
            snapshot
                .nodes
                .iter()
                .find(|current_node| current_node.id == node.id)
                != Some(node)
        })
        || snapshot
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Calls)
            .count()
            != 1
        || snapshot.edges.iter().any(|edge| {
            constrained.iter().any(|node| node.id == edge.source)
                && edge.relation == RelationKind::ResolvesTo
        })
        || reopened.historical_snapshot(first)? != retained
    {
        return Err(std::io::Error::other(
            "restart or re-resolution lost binding constraint/history",
        )
        .into());
    }
    drop(reopened);
    let repeated = host.run(root, None, &[])?;
    if !repeated.status.success()
        || !String::from_utf8_lossy(&repeated.stdout).contains("already up to date")
    {
        return Err(std::io::Error::other("unchanged binding facts were not cached").into());
    }
    Ok(())
}
