use std::fs;
use std::process::Command;

use syntaxmesh_core::{NodeKind, RelationKind};
use syntaxmesh_store::{FileGraphStore, GraphStore};

#[test]
fn markdown_resolves_only_indexed_supported_source_file_links()
-> Result<(), Box<dyn std::error::Error>> {
    let repository = tempfile::tempdir()?;
    fs::create_dir_all(repository.path().join("src"))?;
    fs::write(
        repository.path().join("README.md"),
        "# Architecture\n\nThe engine preserves historic rationale for later lookup.\n\n[Engine](src/lib.rs#engine) [Missing](src/missing.py) [External](https://example.test/src/lib.rs)\n",
    )?;
    fs::write(repository.path().join("src/lib.rs"), "pub fn engine() {}\n")?;

    let snapshot = repository.path().join("graph.snapshot");
    run_index(repository.path(), &snapshot)?;

    let store = FileGraphStore::open(&snapshot)?;
    let generation = store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("source-link indexing published no generation"))?;
    let nodes = store.nodes(generation.generation).map_err(|error| {
        std::io::Error::other(format!("reading first generation nodes: {error}"))
    })?;
    let edges = store.edges(generation.generation).map_err(|error| {
        std::io::Error::other(format!("reading first generation edges: {error}"))
    })?;
    let rationale = store
        .search_nodes(generation.generation, "historic rationale", 10)
        .map_err(|error| std::io::Error::other(format!("searching document content: {error}")))?;
    let rationale_chunk = rationale
        .iter()
        .find(|node| node.kind == NodeKind::DocumentChunk)
        .ok_or_else(|| {
            std::io::Error::other("document prose was not searchable as a graph fact")
        })?;
    if !rationale_chunk
        .name
        .contains("The engine preserves historic rationale for later lookup.")
        || !edges.iter().any(|edge| {
            edge.target == rationale_chunk.id && edge.relation == RelationKind::Contains
        })
    {
        return Err(std::io::Error::other(
            "searchable document prose is missing its source-structure edge",
        )
        .into());
    }
    let file = nodes
        .iter()
        .find(|node| node.kind == NodeKind::File && node.name == "src/lib.rs")
        .ok_or_else(|| std::io::Error::other("indexed Rust file node is missing"))?;
    let resolved = nodes
        .iter()
        .find(|node| {
            node.name == "src/lib.rs"
                && node.kind
                    == (NodeKind::Reference {
                        relation: RelationKind::References,
                    })
        })
        .ok_or_else(|| std::io::Error::other("Markdown source link was not resolved"))?;
    if !edges.iter().any(|edge| {
        edge.source == resolved.id
            && edge.target == file.id
            && edge.relation == RelationKind::ResolvesTo
    }) {
        return Err(std::io::Error::other(
            "resolved Markdown source link does not target the indexed file",
        )
        .into());
    }

    let missing = nodes
        .iter()
        .find(|node| {
            node.name == "src/missing.py"
                && node.kind
                    == (NodeKind::UnresolvedReference {
                        relation: RelationKind::References,
                    })
        })
        .ok_or_else(|| {
            std::io::Error::other("missing source link was not retained as unresolved")
        })?;
    if edges
        .iter()
        .any(|edge| edge.source == missing.id && edge.relation == RelationKind::ResolvesTo)
        || nodes
            .iter()
            .any(|node| node.name == "https://example.test/src/lib.rs")
    {
        return Err(std::io::Error::other(
            "missing or external Markdown link was incorrectly resolved",
        )
        .into());
    }

    let first_generation = generation.generation;
    let linked_file_id = file.id;
    let linked_reference_id = resolved.id;
    drop(store);
    fs::remove_file(repository.path().join("src/lib.rs"))?;
    run_index(repository.path(), &snapshot)?;

    let reopened = FileGraphStore::open(&snapshot).map_err(|error| {
        std::io::Error::other(format!(
            "reopening indexed snapshot after source deletion: {error}"
        ))
    })?;
    let second_generation = reopened
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("source removal published no generation"))?;
    let current_nodes = reopened
        .nodes(second_generation.generation)
        .map_err(|error| {
            std::io::Error::other(format!("reading updated generation nodes: {error}"))
        })?;
    let current_edges = reopened
        .edges(second_generation.generation)
        .map_err(|error| {
            std::io::Error::other(format!("reading updated generation edges: {error}"))
        })?;
    if second_generation.generation == first_generation
        || current_nodes.iter().any(|node| node.id == linked_file_id)
        || !current_nodes.iter().any(|node| {
            node.id == linked_reference_id
                && node.kind
                    == (NodeKind::UnresolvedReference {
                        relation: RelationKind::References,
                    })
        })
        || current_edges
            .iter()
            .any(|edge| edge.source == linked_reference_id || edge.target == linked_file_id)
    {
        return Err(std::io::Error::other(
            "removing a linked source file did not retract its file fact and preserve the link as unresolved",
        )
        .into());
    }
    Ok(())
}

#[test]
fn adr_and_rfc_content_links_create_source_grounded_rationale_edges()
-> Result<(), Box<dyn std::error::Error>> {
    let repository = tempfile::tempdir()?;
    fs::create_dir_all(repository.path().join("docs/adr"))?;
    fs::create_dir_all(repository.path().join("docs/rfcs"))?;
    fs::create_dir_all(repository.path().join("src"))?;
    fs::write(
        repository.path().join("docs/adr/0001-engine.md"),
        "# ADR-0001: Engine\n\n- Status: superseded by [ADR-0002](0002-engine-successor.md)\n\n## Context\n\nEarlier constraints are in [old](../../src/context.rs).\n\n## Decision\n\nUse the [engine](../../src/engine.rs#Engine). A missing [future module](../../src/future.rs) stays explicit.\n",
    )?;
    fs::write(
        repository.path().join("docs/adr/0002-engine-successor.md"),
        "# ADR-0002: Engine successor\n\n- Status: accepted\n",
    )?;
    fs::write(
        repository.path().join("docs/rfcs/0001-api.md"),
        "# RFC-0001: API\n\n## Proposed API\n\nExpose [public API](../../src/api.py).\n\n## Alternatives\n\nCompare [other option](../../src/other.py).\n",
    )?;
    fs::write(
        repository.path().join("src/engine.rs"),
        "pub fn engine() {}\n",
    )?;
    fs::write(
        repository.path().join("src/context.rs"),
        "pub fn context() {}\n",
    )?;
    fs::write(
        repository.path().join("src/api.py"),
        "def api():\n    pass\n",
    )?;

    let snapshot = repository.path().join("graph.snapshot");
    run_index(repository.path(), &snapshot)?;
    let store = FileGraphStore::open(&snapshot)?;
    let generation = store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("rationale indexing published no generation"))?;
    let nodes = store.nodes(generation.generation).map_err(|error| {
        std::io::Error::other(format!("reading rationale graph nodes: {error}"))
    })?;
    let edges = store.edges(generation.generation).map_err(|error| {
        std::io::Error::other(format!("reading rationale graph edges: {error}"))
    })?;

    let linked = [
        ("Decision", "src/engine.rs", "decides"),
        ("Proposed API", "src/api.py", "proposes"),
    ];
    for (section_name, target_path, relation_name) in linked {
        let section = nodes
            .iter()
            .find(|node| node.kind == NodeKind::Section && node.name == section_name)
            .ok_or_else(|| std::io::Error::other(format!("missing {section_name} section")))?;
        let target = nodes
            .iter()
            .find(|node| node.kind == NodeKind::File && node.name == target_path)
            .ok_or_else(|| std::io::Error::other(format!("missing linked file {target_path}")))?;
        let semantic_relation = RelationKind::External {
            namespace: "syntaxmesh.documentation".to_owned(),
            relation: relation_name.to_owned(),
        };
        if !edges.iter().any(|edge| {
            edge.source == section.id
                && edge.target == target.id
                && edge.relation == semantic_relation
        }) {
            return Err(std::io::Error::other(format!(
                "{section_name} did not link to {target_path} with {relation_name}"
            ))
            .into());
        }
    }

    let old_adr = nodes
        .iter()
        .find(|node| node.kind == NodeKind::Document && node.name == "docs/adr/0001-engine.md")
        .ok_or_else(|| std::io::Error::other("missing superseded ADR document"))?;
    let successor_adr = nodes
        .iter()
        .find(|node| {
            node.kind == NodeKind::Document && node.name == "docs/adr/0002-engine-successor.md"
        })
        .ok_or_else(|| std::io::Error::other("missing ADR successor document"))?;
    if !edges.iter().any(|edge| {
        edge.source == old_adr.id
            && edge.target == successor_adr.id
            && edge.relation
                == (RelationKind::External {
                    namespace: "syntaxmesh.documentation".to_owned(),
                    relation: "superseded_by".to_owned(),
                })
    }) {
        return Err(std::io::Error::other(
            "explicit ADR status did not link the old and successor documents",
        )
        .into());
    }

    let context = nodes
        .iter()
        .find(|node| node.kind == NodeKind::Section && node.name == "Context")
        .ok_or_else(|| std::io::Error::other("missing Context section"))?;
    let context_file = nodes
        .iter()
        .find(|node| node.kind == NodeKind::File && node.name == "src/context.rs")
        .ok_or_else(|| std::io::Error::other("missing Context link target"))?;
    if !edges.iter().any(|edge| {
        edge.source == context.id
            && edge.target == context_file.id
            && edge.relation == RelationKind::References
    }) {
        return Err(std::io::Error::other("Context link was not kept as References").into());
    }

    let missing = nodes
        .iter()
        .find(|node| {
            node.name == "src/future.rs"
                && node.kind
                    == (NodeKind::UnresolvedReference {
                        relation: RelationKind::External {
                            namespace: "syntaxmesh.documentation".to_owned(),
                            relation: "decides".to_owned(),
                        },
                    })
        })
        .ok_or_else(|| std::io::Error::other("missing decision target lost its relation"))?;
    let missing_reference_id = missing.id;
    let decision_section_id = nodes
        .iter()
        .find(|node| node.kind == NodeKind::Section && node.name == "Decision")
        .ok_or_else(|| std::io::Error::other("missing Decision section"))?
        .id;
    if edges
        .iter()
        .any(|edge| edge.source == missing.id && edge.relation == RelationKind::ResolvesTo)
    {
        return Err(std::io::Error::other("missing decision target resolved unexpectedly").into());
    }

    let rationale = store
        .search_nodes(generation.generation, "Use the", 10)
        .map_err(|error| std::io::Error::other(format!("searching rationale: {error}")))?;
    if !rationale
        .iter()
        .any(|node| node.kind == NodeKind::DocumentChunk && node.name.contains("Use the"))
    {
        return Err(std::io::Error::other(
            "decision prose is not searchable as source-backed graph content",
        )
        .into());
    }

    drop(store);
    fs::write(
        repository.path().join("src/future.rs"),
        "pub fn future() {}\n",
    )?;
    run_index(repository.path(), &snapshot)?;
    let with_target = FileGraphStore::open(&snapshot)?;
    let linked_generation = with_target
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("adding target published no generation"))?;
    let linked_nodes = with_target
        .nodes(linked_generation.generation)
        .map_err(|error| {
            std::io::Error::other(format!("reading linked generation nodes: {error}"))
        })?;
    let linked_edges = with_target
        .edges(linked_generation.generation)
        .map_err(|error| {
            std::io::Error::other(format!("reading linked generation edges: {error}"))
        })?;
    let future_file = linked_nodes
        .iter()
        .find(|node| node.kind == NodeKind::File && node.name == "src/future.rs")
        .ok_or_else(|| std::io::Error::other("new decision target file was not indexed"))?;
    if !linked_nodes.iter().any(|node| {
        node.id == missing_reference_id
            && node.kind
                == (NodeKind::Reference {
                    relation: RelationKind::External {
                        namespace: "syntaxmesh.documentation".to_owned(),
                        relation: "decides".to_owned(),
                    },
                })
    }) || !linked_edges.iter().any(|edge| {
        edge.source == missing_reference_id
            && edge.target == future_file.id
            && edge.relation == RelationKind::ResolvesTo
    }) || !linked_edges.iter().any(|edge| {
        edge.source == decision_section_id
            && edge.target == future_file.id
            && edge.relation
                == (RelationKind::External {
                    namespace: "syntaxmesh.documentation".to_owned(),
                    relation: "decides".to_owned(),
                })
    }) {
        return Err(std::io::Error::other(
            "adding the linked source file did not resolve the existing decision relation",
        )
        .into());
    }

    drop(with_target);
    fs::remove_file(repository.path().join("src/future.rs"))?;
    fs::write(
        repository.path().join("docs/after-removal.txt"),
        "This unrelated note gives removal its own source snapshot.\n",
    )?;
    run_index(repository.path(), &snapshot)?;
    let without_target = FileGraphStore::open(&snapshot)?;
    let unresolved_generation = without_target
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("removing target published no generation"))?;
    let unresolved_nodes = without_target
        .nodes(unresolved_generation.generation)
        .map_err(|error| std::io::Error::other(format!("reading unresolved nodes: {error}")))?;
    let unresolved_edges = without_target
        .edges(unresolved_generation.generation)
        .map_err(|error| std::io::Error::other(format!("reading unresolved edges: {error}")))?;
    if !unresolved_nodes.iter().any(|node| {
        node.id == missing_reference_id
            && node.kind
                == (NodeKind::UnresolvedReference {
                    relation: RelationKind::External {
                        namespace: "syntaxmesh.documentation".to_owned(),
                        relation: "decides".to_owned(),
                    },
                })
    }) || unresolved_edges.iter().any(|edge| {
        edge.source == missing_reference_id && edge.relation == RelationKind::ResolvesTo
    }) || unresolved_edges.iter().any(|edge| {
        edge.source == decision_section_id
            && edge.target == future_file.id
            && edge.relation
                == (RelationKind::External {
                    namespace: "syntaxmesh.documentation".to_owned(),
                    relation: "decides".to_owned(),
                })
    }) {
        return Err(std::io::Error::other(
            "removing the linked source file did not preserve the unresolved decision fact",
        )
        .into());
    }
    Ok(())
}

fn run_index(
    repository: &std::path::Path,
    snapshot: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(repository)
        .arg(snapshot)
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "source-link indexing failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}
