use std::fs;
use std::process::Command;

use syntaxmesh_core::{ImportKind, NodeKind, RelationKind};
use syntaxmesh_store::{FileGraphStore, GraphStore};

#[test]
fn resolves_node_and_python_imports_across_monorepo_packages()
-> Result<(), Box<dyn std::error::Error>> {
    let repository = tempfile::tempdir()?;
    let write = |relative: &str, contents: &str| -> Result<(), std::io::Error> {
        let path = repository.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    };

    write(
        "packages/web/src/main.ts",
        "import defaultValue, { publicValue as localValue, absent as localAbsent, Merged } from '@/shared/src/value';\nimport { chainValue as localChainValue } from '@/shared/barrel';\nimport * as barrelNamespace from '@/shared/barrel';\nimport * as ambiguousNamespace from '@/shared/ambiguous';\nimport { choice as localChoice } from '@/shared/shadow';\nimport { dual as localDual } from '@/shared/ambiguous';\nimport { cyclicMissing as localCyclicMissing } from '@/shared/cycle';\nimport defaultFromStar from '@/shared/default-barrel';\nexport const main = [defaultValue, localValue, localAbsent, Merged, localChainValue, barrelNamespace['chainValue'], ambiguousNamespace?.['dual'], localChoice, localDual, localCyclicMissing, defaultFromStar];\n",
    )?;
    write(
        "packages/shared/src/value.ts",
        "const value = 1;\nexport { value as publicValue };\nexport { value as chainValue };\nexport default value;\nexport class Merged {}\nexport interface Merged {}\n",
    )?;
    write(
        "packages/shared/barrel/index.ts",
        "export * from './inner';\n",
    )?;
    write(
        "packages/shared/barrel/inner.ts",
        "export * from '../src/value';\n",
    )?;
    write(
        "packages/shared/shadow/index.ts",
        "export { choice } from './one';\nexport * from './two';\n",
    )?;
    write(
        "packages/shared/shadow/one.ts",
        "export const choice = 'explicit';\n",
    )?;
    write(
        "packages/shared/shadow/two.ts",
        "export const choice = 'star';\n",
    )?;
    write(
        "packages/shared/ambiguous/index.ts",
        "export * from './one';\nexport * from './two';\n",
    )?;
    write(
        "packages/shared/ambiguous/one.ts",
        "export const dual = 1;\n",
    )?;
    write(
        "packages/shared/ambiguous/two.ts",
        "export const dual = 2;\n",
    )?;
    write("packages/shared/cycle/index.ts", "export * from './one';\n")?;
    write("packages/shared/cycle/one.ts", "export * from './two';\n")?;
    write("packages/shared/cycle/two.ts", "export * from './one';\n")?;
    write(
        "packages/shared/default-barrel/index.ts",
        "export * from '../src/value';\n",
    )?;
    write("packages/py_app/__init__.py", "")?;
    write(
        "packages/py_app/app.py",
        "from py_shared import utility\nvalue = utility.value\n",
    )?;
    write("packages/py_shared/__init__.py", "")?;
    write("packages/py_shared/utility.py", "value = 1\n")?;
    write(
        "packages/native/src/lib.rs",
        "use std::fmt::Debug;\npub fn render(_: impl Debug) {}\n",
    )?;
    write(
        "syntaxmesh.toml",
        "[module_resolution]\nprofiles = [\"node\", \"python\"]\nsource_roots = [\"packages\"]\n",
    )?;
    write(
        "tsconfig.json",
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["packages/*"]}}}"#,
    )?;

    let snapshot = repository.path().join("graph.snapshot");
    let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
        .arg("index")
        .arg(repository.path())
        .arg(&snapshot)
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "monorepo indexing failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }

    let store = FileGraphStore::open(&snapshot)?;
    let generation = store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("monorepo indexing published no generation"))?;
    let nodes = store.nodes(generation.generation)?;
    let edges = store.edges(generation.generation)?;
    let imports = nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Import {
                specifier, kind, ..
            } => Some((node, specifier.as_str(), kind)),
            NodeKind::Repository
            | NodeKind::File
            | NodeKind::Module
            | NodeKind::Function
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::Test
            | NodeKind::External { .. }
            | NodeKind::Reference { .. }
            | NodeKind::UnresolvedReference { .. }
            | NodeKind::AmbiguousReference { .. }
            | NodeKind::RuntimeObservation
            | NodeKind::Class
            | NodeKind::Export { .. }
            | NodeKind::ModuleResolutionDiagnostic { .. }
            | NodeKind::Script
            | NodeKind::Document
            | NodeKind::Section
            | NodeKind::DocumentChunk => None,
        })
        .collect::<Vec<_>>();

    for (specifier, expected_path) in [
        ("@/shared/src/value", "packages/shared/src/value.ts"),
        ("@/shared/barrel", "packages/shared/barrel/index.ts"),
        ("@/shared/shadow", "packages/shared/shadow/index.ts"),
        ("@/shared/ambiguous", "packages/shared/ambiguous/index.ts"),
        ("@/shared/cycle", "packages/shared/cycle/index.ts"),
        (
            "@/shared/default-barrel",
            "packages/shared/default-barrel/index.ts",
        ),
        ("py_shared", "packages/py_shared/__init__.py"),
    ] {
        let import = imports
            .iter()
            .find(|(_, actual, _)| *actual == specifier)
            .map(|(node, _, _)| *node)
            .ok_or_else(|| std::io::Error::other(format!("missing import {specifier}")))?;
        let resolved_targets = edges
            .iter()
            .filter(|edge| edge.source == import.id && edge.relation == RelationKind::ResolvesTo)
            .filter_map(|edge| nodes.iter().find(|node| node.id == edge.target))
            .filter(|node| node.kind == NodeKind::Module)
            .collect::<Vec<_>>();
        let [resolved_target] = resolved_targets.as_slice() else {
            return Err(std::io::Error::other(format!(
                "monorepo import {specifier} did not resolve uniquely to {expected_path}: {resolved_targets:?}"
            ))
            .into());
        };
        if resolved_target.kind != NodeKind::Module || resolved_target.name != expected_path {
            return Err(std::io::Error::other(format!(
                "monorepo import {specifier} resolved to the wrong module: {resolved_target:?}"
            ))
            .into());
        }
    }

    for (import_specifier, imported_name, local_name, exported_name) in [
        (
            "@/shared/src/value",
            "publicValue",
            "localValue",
            "publicValue",
        ),
        ("@/shared/src/value", "default", "defaultValue", "default"),
        (
            "@/shared/barrel",
            "chainValue",
            "localChainValue",
            "chainValue",
        ),
        ("@/shared/shadow", "choice", "localChoice", "choice"),
    ] {
        let import = nodes
            .iter()
            .find(|node| {
                matches!(
                    &node.kind,
                    NodeKind::Import {
                        specifier,
                        kind: ImportKind::Named | ImportKind::Default,
                        imported_name: Some(actual_imported),
                        local_name: Some(actual_local),
                        ..
                    } if specifier == import_specifier
                        && actual_imported == imported_name
                        && actual_local == local_name
                )
            })
            .ok_or_else(|| {
                std::io::Error::other(format!(
                    "missing aliased TypeScript import {imported_name} as {local_name}"
                ))
            })?;
        let targets = edges
            .iter()
            .filter(|edge| edge.source == import.id && edge.relation == RelationKind::ResolvesTo)
            .filter_map(|edge| nodes.iter().find(|node| node.id == edge.target))
            .filter(|node| {
                matches!(
                    &node.kind,
                    NodeKind::Export {
                        exported_name: Some(actual_name),
                        ..
                    } if actual_name == exported_name
                )
            })
            .collect::<Vec<_>>();
        if targets.len() != 1 {
            return Err(std::io::Error::other(format!(
                "import {imported_name} as {local_name} did not resolve to exactly one export {exported_name}: {targets:?}"
            ))
            .into());
        }
        if import_specifier == "@/shared/shadow" {
            let direct_module_owner = edges
                .iter()
                .filter(|edge| {
                    edge.source == import.id && edge.relation == RelationKind::ResolvesTo
                })
                .filter_map(|edge| nodes.iter().find(|node| node.id == edge.target))
                .find(|node| node.kind == NodeKind::Module)
                .and_then(|node| node.owner_file);
            if targets.first().and_then(|node| node.owner_file) != direct_module_owner {
                return Err(std::io::Error::other(
                    "explicit re-export did not shadow the star branch",
                )
                .into());
            }
        }
    }

    for (specifier, imported_name, local_name) in [
        ("@/shared/src/value", "absent", "localAbsent"),
        ("@/shared/src/value", "Merged", "Merged"),
        ("@/shared/ambiguous", "dual", "localDual"),
        ("@/shared/cycle", "cyclicMissing", "localCyclicMissing"),
        ("@/shared/default-barrel", "default", "defaultFromStar"),
    ] {
        let import = nodes
            .iter()
            .find(|node| {
                matches!(
                    &node.kind,
                    NodeKind::Import {
                        specifier: actual_specifier,
                        imported_name: Some(actual_name),
                        local_name: Some(actual_local),
                        ..
                    } if actual_specifier == specifier
                        && actual_name == imported_name
                        && actual_local == local_name
                )
            })
            .ok_or_else(|| std::io::Error::other(format!("missing import {imported_name}")))?;
        let target_kinds = edges
            .iter()
            .filter(|edge| edge.source == import.id && edge.relation == RelationKind::ResolvesTo)
            .filter_map(|edge| nodes.iter().find(|node| node.id == edge.target))
            .filter(|node| {
                matches!(
                    &node.kind,
                    NodeKind::Export {
                        exported_name: Some(actual_name),
                        ..
                    } if actual_name == imported_name
                )
            })
            .count();
        if target_kinds != 0 {
            return Err(std::io::Error::other(format!(
                "non-unique or missing export {imported_name} unexpectedly received a binding edge"
            ))
            .into());
        }
        if !edges.iter().any(|edge| {
            edge.source == import.id
                && edge.relation == RelationKind::ResolvesTo
                && nodes
                    .iter()
                    .any(|node| node.id == edge.target && node.kind == NodeKind::Module)
        }) {
            return Err(std::io::Error::other(format!(
                "missing or ambiguous export {imported_name} suppressed the valid module edge"
            ))
            .into());
        }
    }

    let rust_import = imports
        .iter()
        .find(|(_, specifier, kind)| {
            **kind == ImportKind::RustUse && *specifier == "std::fmt::Debug"
        })
        .ok_or_else(|| std::io::Error::other("monorepo Rust import was not indexed"))?;
    if edges.iter().any(|edge| {
        edge.source == rust_import.0.id
            && matches!(
                edge.relation,
                RelationKind::ResolvesTo | RelationKind::HasResolutionDiagnostic
            )
    }) {
        return Err(std::io::Error::other(
            "Node or Python resolution profile claimed the Rust import",
        )
        .into());
    }

    let aliased_import = nodes
        .iter()
        .find(|node| {
            matches!(
                &node.kind,
                NodeKind::Import {
                    imported_name: Some(imported_name),
                    local_name: Some(local_name),
                    ..
                } if imported_name == "publicValue" && local_name == "localValue"
            )
        })
        .ok_or_else(|| std::io::Error::other("aliased import disappeared before re-indexing"))?;
    let aliased_import = aliased_import.clone();
    let aliased_import_id = aliased_import.id;
    let star_import_id = nodes
        .iter()
        .find(|node| {
            matches!(
                &node.kind,
                NodeKind::Import {
                    specifier,
                    imported_name: Some(imported_name),
                    local_name: Some(local_name),
                    ..
                } if specifier == "@/shared/barrel"
                    && imported_name == "chainValue"
                    && local_name == "localChainValue"
            )
        })
        .map(|node| node.id)
        .ok_or_else(|| std::io::Error::other("star-chain import disappeared"))?;
    let chain_export_id = nodes
        .iter()
        .find(|node| {
            matches!(
                &node.kind,
                NodeKind::Export {
                    exported_name: Some(exported_name),
                    ..
                } if exported_name == "chainValue"
            )
        })
        .map(|node| node.id)
        .ok_or_else(|| std::io::Error::other("star-chain target export disappeared"))?;
    if !edges.iter().any(|edge| {
        edge.source == star_import_id
            && edge.target == chain_export_id
            && edge.relation == RelationKind::ResolvesTo
    }) {
        return Err(std::io::Error::other(
            "multi-hop import did not bind to the exact downstream export occurrence",
        )
        .into());
    }
    let namespace_member_id = nodes
        .iter()
        .find(|node| {
            matches!(
                &node.kind,
                NodeKind::Import {
                    specifier,
                    kind: ImportKind::NamespaceMember,
                    imported_name: Some(imported_name),
                    local_name: Some(local_name),
                    ..
                } if specifier == "@/shared/barrel"
                    && imported_name == "chainValue"
                    && local_name == "barrelNamespace.chainValue"
            )
        })
        .map(|node| node.id)
        .ok_or_else(|| std::io::Error::other("namespace member occurrence missing"))?;
    let namespace_member = nodes
        .iter()
        .find(|node| node.id == namespace_member_id)
        .ok_or_else(|| std::io::Error::other("namespace member node disappeared"))?
        .clone();
    if !edges.iter().any(|edge| {
        edge.source == namespace_member_id
            && edge.target == chain_export_id
            && edge.relation == RelationKind::ResolvesTo
    }) {
        return Err(std::io::Error::other(
            "namespace member did not bind through the multi-hop star chain",
        )
        .into());
    }
    let ambiguous_namespace_member_id = nodes
        .iter()
        .find(|node| {
            matches!(
                &node.kind,
                NodeKind::Import {
                    specifier,
                    kind: ImportKind::NamespaceMember,
                    imported_name: Some(imported_name),
                    local_name: Some(local_name),
                    ..
                } if specifier == "@/shared/ambiguous"
                    && imported_name == "dual"
                    && local_name == "ambiguousNamespace.dual"
            )
        })
        .map(|node| node.id)
        .ok_or_else(|| std::io::Error::other("ambiguous namespace member occurrence missing"))?;
    if edges.iter().any(|edge| {
        edge.source == ambiguous_namespace_member_id
            && edge.relation == RelationKind::ResolvesTo
            && nodes.iter().any(|node| {
                node.id == edge.target
                    && matches!(
                        &node.kind,
                        NodeKind::Export {
                            exported_name: Some(name),
                            ..
                        } if name == "dual"
                    )
            })
    }) || !edges.iter().any(|edge| {
        edge.source == ambiguous_namespace_member_id
            && edge.relation == RelationKind::ResolvesTo
            && nodes
                .iter()
                .any(|node| node.id == edge.target && node.kind == NodeKind::Module)
    }) {
        return Err(std::io::Error::other(
            "ambiguous namespace member did not retain only its module resolution",
        )
        .into());
    }
    drop(store);

    let index_again = || {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg("index")
            .arg(repository.path())
            .arg(&snapshot)
            .output()
    };
    write(
        "packages/shared/src/value.ts",
        "const value = 1;\nexport default value;\nexport class Merged {}\nexport interface Merged {}\n",
    )?;
    let changed_output = index_again()?;
    if !changed_output.status.success() {
        return Err(std::io::Error::other(format!(
            "re-index after export removal failed: {}",
            String::from_utf8_lossy(&changed_output.stderr)
        ))
        .into());
    }
    let changed_store = FileGraphStore::open(&snapshot)?;
    let changed_generation = changed_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("re-index after export removal lost its generation"))?
        .generation;
    let changed_nodes = changed_store.nodes(changed_generation)?;
    let changed_edges = changed_store.edges(changed_generation)?;
    let retained_import = changed_nodes
        .iter()
        .find(|node| node.id == aliased_import_id)
        .ok_or_else(|| std::io::Error::other("source import identity changed after target edit"))?;
    if retained_import != &aliased_import
        || changed_edges.iter().any(|edge| {
            edge.source == aliased_import_id
                && edge.relation == RelationKind::ResolvesTo
                && changed_nodes.iter().any(|node| {
                    node.id == edge.target
                        && matches!(
                            &node.kind,
                            NodeKind::Export {
                                exported_name: Some(exported_name),
                                ..
                            } if exported_name == "publicValue"
                        )
                })
        })
        || !changed_edges.iter().any(|edge| {
            edge.source == aliased_import_id
                && edge.relation == RelationKind::ResolvesTo
                && changed_nodes
                    .iter()
                    .any(|node| node.id == edge.target && node.kind == NodeKind::Module)
        })
        || changed_edges.iter().any(|edge| {
            edge.source == star_import_id
                && edge.relation == RelationKind::ResolvesTo
                && changed_nodes.iter().any(|node| {
                    node.id == edge.target
                        && matches!(
                            &node.kind,
                            NodeKind::Export {
                                exported_name: Some(exported_name),
                                ..
                            } if exported_name == "chainValue"
                        )
                })
        })
        || !changed_edges.iter().any(|edge| {
            edge.source == star_import_id
                && edge.relation == RelationKind::ResolvesTo
                && changed_nodes
                    .iter()
                    .any(|node| node.id == edge.target && node.kind == NodeKind::Module)
        })
    {
        return Err(std::io::Error::other(
            "source evidence changed, stale binding survived, or module resolution was lost after export removal",
        )
        .into());
    }
    let retained_namespace_member = changed_nodes
        .iter()
        .find(|node| node.id == namespace_member_id)
        .ok_or_else(|| {
            std::io::Error::other("namespace member occurrence disappeared after target edit")
        })?;
    if retained_namespace_member != &namespace_member
        || changed_edges.iter().any(|edge| {
            edge.source == namespace_member_id
                && edge.relation == RelationKind::ResolvesTo
                && changed_nodes.iter().any(|node| {
                    node.id == edge.target
                        && matches!(
                            &node.kind,
                            NodeKind::Export {
                                exported_name: Some(name),
                                ..
                            } if name == "chainValue"
                        )
                })
        })
        || !changed_edges.iter().any(|edge| {
            edge.source == namespace_member_id
                && edge.relation == RelationKind::ResolvesTo
                && changed_nodes
                    .iter()
                    .any(|node| node.id == edge.target && node.kind == NodeKind::Module)
        })
    {
        return Err(std::io::Error::other(
            "namespace member source fact or module edge changed incorrectly after target edit",
        )
        .into());
    }
    drop(changed_store);

    write(
        "packages/shared/src/value.ts",
        "const value = 1;\nexport { value as publicValue };\nexport { value as chainValue };\nexport default value;\nexport class Merged {}\nexport interface Merged {}\n",
    )?;
    let restored_output = index_again()?;
    if !restored_output.status.success() {
        return Err(std::io::Error::other(format!(
            "re-index after export restoration failed: {}",
            String::from_utf8_lossy(&restored_output.stderr)
        ))
        .into());
    }
    let restored_store = FileGraphStore::open(&snapshot)?;
    let restored_generation = restored_store
        .latest_generation()
        .ok_or_else(|| std::io::Error::other("export restoration lost its generation"))?
        .generation;
    let restored_nodes = restored_store.nodes(restored_generation)?;
    let restored_edges = restored_store.edges(restored_generation)?;
    if !restored_edges.iter().any(|edge| {
        edge.source == aliased_import_id
            && edge.relation == RelationKind::ResolvesTo
            && restored_nodes.iter().any(|node| {
                node.id == edge.target
                    && matches!(
                        &node.kind,
                        NodeKind::Export {
                            exported_name: Some(exported_name),
                            ..
                        } if exported_name == "publicValue"
                    )
            })
    }) {
        return Err(std::io::Error::other(
            "unique export restoration did not restore the binding edge",
        )
        .into());
    }
    if !restored_edges.iter().any(|edge| {
        edge.source == namespace_member_id
            && edge.relation == RelationKind::ResolvesTo
            && restored_nodes.iter().any(|node| {
                node.id == edge.target
                    && matches!(
                        &node.kind,
                        NodeKind::Export {
                            exported_name: Some(name),
                            ..
                        } if name == "chainValue"
                    )
            })
    }) {
        return Err(std::io::Error::other(
            "namespace member binding was not restored with the unique export",
        )
        .into());
    }
    if !restored_edges.iter().any(|edge| {
        edge.source == star_import_id
            && edge.relation == RelationKind::ResolvesTo
            && restored_nodes.iter().any(|node| {
                node.id == edge.target
                    && matches!(
                        &node.kind,
                        NodeKind::Export {
                            exported_name: Some(exported_name),
                            ..
                        } if exported_name == "chainValue"
                    )
            })
    }) {
        return Err(std::io::Error::other(
            "unique star-chain export restoration did not restore its binding edge",
        )
        .into());
    }

    Ok(())
}
