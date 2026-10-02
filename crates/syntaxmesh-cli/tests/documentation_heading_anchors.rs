use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use syntaxmesh_core::{GraphSnapshot, NodeId, NodeKind, RelationKind};
use syntaxmesh_language_sdk::is_source_anchor;

#[path = "support/semantic_store.rs"]
mod semantic_store;
use semantic_store::StoreHost;

const GUIDE: &str =
    "# Résumé\n\n## Decision\n\nKeep source evidence.\n\n## Decision\n\nKeep history too.\n";

fn index(host: StoreHost, root: &Path) -> Result<(), Box<dyn Error>> {
    let output = host.run(root, None, &[])?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "heading index failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}

fn resolved(snapshot: &GraphSnapshot, name: &str, anchor: bool) -> Result<NodeId, Box<dyn Error>> {
    let occurrence = snapshot
        .nodes
        .iter()
        .find(|node| {
            node.name == name
                && node.kind
                    == (NodeKind::Reference {
                        relation: RelationKind::References,
                    })
        })
        .ok_or_else(|| {
            std::io::Error::other(format!("resolved heading occurrence missing: {name}"))
        })?;
    let target = snapshot
        .nodes
        .iter()
        .find(|node| {
            node.name == name
                && if anchor {
                    is_source_anchor(&node.kind)
                } else {
                    node.kind == NodeKind::Document
                }
        })
        .ok_or_else(|| std::io::Error::other(format!("heading target missing: {name}")))?;
    if !snapshot.edges.iter().any(|edge| {
        edge.source == occurrence.id
            && edge.target == target.id
            && edge.relation == RelationKind::ResolvesTo
    }) {
        return Err("heading occurrence lost its exact target edge".into());
    }
    Ok(occurrence.id)
}

fn unresolved(snapshot: &GraphSnapshot, name: &str) -> Result<(), Box<dyn Error>> {
    let occurrence = snapshot
        .nodes
        .iter()
        .find(|node| {
            node.name == name
                && node.kind
                    == (NodeKind::UnresolvedReference {
                        relation: RelationKind::References,
                    })
        })
        .ok_or_else(|| {
            std::io::Error::other(format!("unresolved heading occurrence missing: {name}"))
        })?;
    if snapshot
        .edges
        .iter()
        .any(|edge| edge.source == occurrence.id && edge.relation == RelationKind::ResolvesTo)
    {
        return Err("missing heading incorrectly fell back to another target".into());
    }
    Ok(())
}

#[test]
fn file_heading_links_rebind_and_retain_history_without_ai() -> Result<(), Box<dyn Error>> {
    scenario(StoreHost::File)
}

#[test]
fn verified_turso_heading_links_rebind_and_retain_history_without_ai() -> Result<(), Box<dyn Error>>
{
    scenario(StoreHost::TursoVerified)
}

fn scenario(host: StoreHost) -> Result<(), Box<dyn Error>> {
    for (path, link) in [
        ("docs/guide.md", "docs/guide.md"),
        ("docs/design résumé.md", "docs/design%20r%C3%A9sum%C3%A9.md"),
    ] {
        path_scenario(host, path, link)?;
    }
    Ok(())
}

fn path_scenario(host: StoreHost, path: &str, link: &str) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    host.prepare(root)?;
    fs::create_dir(root.join("docs"))?;
    fs::write(root.join(path), GUIDE)?;
    fs::write(
        root.join("README.md"),
        format!(
            "# Entry\n\n[Unicode]({link}#r%C3%A9sum%C3%A9) [Duplicate]({link}#decision-1) [Missing]({link}#missing) [Case]({link}#Decision) [Local](#entry) [Root]({link})\n"
        ),
    )?;
    index(host, root)?;
    let original_store = host.open(root)?;
    let original_generation = host.latest(original_store.as_ref())?.generation;
    let original = original_store.historical_snapshot(original_generation)?;
    resolved(&original, &format!("{path}#résumé"), true)?;
    let occurrence_id = resolved(&original, &format!("{path}#decision-1"), true)?;
    resolved(&original, "README.md#entry", true)?;
    resolved(&original, path, false)?;
    unresolved(&original, &format!("{path}#missing"))?;
    unresolved(&original, &format!("{path}#Decision"))?;
    drop(original_store);

    fs::write(
        root.join(path),
        "# Résumé\n\n## Replacement\n\nKeep source evidence.\n\n## Replacement\n\nKeep history too.\n",
    )?;
    index(host, root)?;
    let changed_store = host.open(root)?;
    let changed_generation = host.latest(changed_store.as_ref())?.generation;
    let changed = changed_store.historical_snapshot(changed_generation)?;
    unresolved(&changed, &format!("{path}#decision-1"))?;
    resolved(&changed, &format!("{path}#résumé"), true)?;
    if changed_generation == original_generation
        || changed_store.historical_snapshot(original_generation)? != original
        || changed
            .nodes
            .iter()
            .any(|node| is_source_anchor(&node.kind) && node.name == format!("{path}#decision-1"))
    {
        return Err("heading edit left a stale anchor or damaged historical graph".into());
    }
    drop(changed_store);

    fs::write(root.join(path), GUIDE)?;
    index(host, root)?;
    let restored_store = host.open(root)?;
    let restored_generation = host.latest(restored_store.as_ref())?.generation;
    let restored = restored_store.historical_snapshot(restored_generation)?;
    if resolved(&restored, &format!("{path}#decision-1"), true)? != occurrence_id
        || restored_store.historical_snapshot(changed_generation)? != changed
        || restored_store.historical_snapshot(original_generation)? != original
    {
        return Err(
            "restored heading failed to rebind the same occurrence or retain prior history".into(),
        );
    }
    drop(restored_store);
    if matches!(host, StoreHost::TursoVerified) {
        let audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("statechronicle-verify-turso")
            .arg(host.path(root))
            .output()?;
        if !audit.status.success()
            || !String::from_utf8_lossy(&audit.stdout).contains("statechronicle_history=verified")
        {
            return Err("heading-link verified history audit failed".into());
        }
    }
    Ok(())
}
