use std::error::Error;
use std::fs;

use syntaxmesh_core::{Edge, Node, NodeKind, RelationKind};

#[path = "support/semantic_store.rs"]
mod store_host;
use store_host::StoreHost;

#[test]
fn file_receiver_scope_retracts_calls_and_retains_history() -> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::File)
}

#[test]
fn verified_turso_receiver_scope_retracts_calls_and_retains_history() -> Result<(), Box<dyn Error>>
{
    lifecycle(StoreHost::TursoVerified)
}

fn require_calls(
    nodes: &[Node],
    edges: &[Edge],
    calls: usize,
    unresolved: usize,
) -> Result<(), Box<dyn Error>> {
    let actual_calls = edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::Calls)
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
    if actual_calls != calls || actual_unresolved != unresolved {
        return Err(std::io::Error::other("persisted this receiver resolution differs").into());
    }
    Ok(())
}

fn lifecycle(host: StoreHost) -> Result<(), Box<dyn Error>> {
    for file in ["main.ts", "main.js"] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path();
        host.prepare(root)?;
        let path = root.join(file);
        fs::write(
            &path,
            "class Service { build() {} run() { this.build(); const lexical = () => this.build(); const dynamic = function() { this.build(); }; } }",
        )?;
        let first_output = host.run(root, None, &[])?;
        if !first_output.status.success() {
            return Err(
                std::io::Error::other(String::from_utf8_lossy(&first_output.stderr)).into(),
            );
        }
        let first_store = host.open(root)?;
        let first = host.latest(first_store.as_ref())?.generation;
        let first_nodes = first_store.nodes(first)?;
        let first_edges = first_store.edges(first)?;
        require_calls(&first_nodes, &first_edges, 2, 1)?;
        drop(first_store);

        let repeat = host.run(root, None, &[])?;
        if !repeat.status.success()
            || !String::from_utf8_lossy(&repeat.stdout).contains("already up to date")
        {
            return Err(std::io::Error::other("receiver scope repeat was not cached").into());
        }
        let repeated_store = host.open(root)?;
        if host.latest(repeated_store.as_ref())?.generation != first {
            return Err(
                std::io::Error::other("unchanged receiver fixture published a generation").into(),
            );
        }
        drop(repeated_store);

        fs::write(
            &path,
            "class Service { build() {} run() { const lexical = function() { this.build(); }; const dynamic = function() { this.build(); }; } }",
        )?;
        let edited = host.run(root, None, &[])?;
        if !edited.status.success() {
            return Err(std::io::Error::other(String::from_utf8_lossy(&edited.stderr)).into());
        }
        let reopened = host.open(root)?;
        let current = host.latest(reopened.as_ref())?.generation;
        if current == first {
            return Err(
                std::io::Error::other("receiver edit did not publish a new generation").into(),
            );
        }
        require_calls(&reopened.nodes(current)?, &reopened.edges(current)?, 0, 2)?;
        let historical = reopened.historical_snapshot(first)?;
        if historical.nodes != first_nodes || historical.edges != first_edges {
            return Err(
                std::io::Error::other("receiver edit changed retained call evidence").into(),
            );
        }
    }
    Ok(())
}
