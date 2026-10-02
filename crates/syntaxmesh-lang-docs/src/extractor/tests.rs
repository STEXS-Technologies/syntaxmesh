use syntaxmesh_core::{FileId, FileVersion};

use super::*;

#[test]
fn heading_anchors_use_document_wide_slug_collisions_and_exact_fragments()
-> Result<(), ExtractorError> {
    let content = "# Guide\n\n## Hello `World`!\n\n## Hello World-1\n\n### Hello World\n\n## Résumé\n\n## !!!\n\n[Local](#hello-world) [Unicode](#r%C3%A9sum%C3%A9) [Missing](#missing) [Case](#Hello-world) [Remote](//example.test/doc.md#x) [Invalid](#%FF)\n";
    let input = source("docs/guide.md", content);
    let extracted = DocumentationExtractor.extract(&input)?;
    let anchors = extracted
        .nodes
        .iter()
        .filter(|node| syntaxmesh_language_sdk::is_source_anchor(&node.kind))
        .collect::<Vec<_>>();
    if anchors
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>()
        != [
            "docs/guide.md#guide",
            "docs/guide.md#hello-world",
            "docs/guide.md#hello-world-1",
            "docs/guide.md#hello-world-2",
            "docs/guide.md#résumé",
        ]
        || extracted
            .references
            .iter()
            .map(|reference| reference.target.as_str())
            .collect::<Vec<_>>()
            != [
                "docs/guide.md#hello-world",
                "docs/guide.md#résumé",
                "docs/guide.md#missing",
                "docs/guide.md#Hello-world",
            ]
    {
        return Err(ExtractorError::InvalidInput(
            "anchor profile or fragment spelling differs".to_owned(),
        ));
    }
    for anchor in &anchors {
        let location = anchor
            .source
            .as_ref()
            .ok_or_else(|| ExtractorError::InvalidInput("anchor source missing".to_owned()))?;
        let start = usize::try_from(location.span.start_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = usize::try_from(location.span.end_byte)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        if !content
            .get(start..end)
            .is_some_and(|heading| heading.starts_with('#'))
            || !extracted.edges.iter().any(|edge| {
                edge.target == anchor.id
                    && edge.relation == RelationKind::Contains
                    && extracted.nodes.iter().any(|section| {
                        section.id == edge.source && section.kind == NodeKind::Section
                    })
            })
        {
            return Err(ExtractorError::InvalidInput(
                "anchor lacks its authored heading/section".to_owned(),
            ));
        }
    }
    let shifted = DocumentationExtractor.extract(&source(
        "docs/guide.md",
        &format!("Unrelated prose.\n\n{content}"),
    ))?;
    let shifted_ids = shifted
        .nodes
        .iter()
        .filter(|node| syntaxmesh_language_sdk::is_source_anchor(&node.kind))
        .map(|node| node.id)
        .collect::<Vec<_>>();
    if shifted_ids != anchors.iter().map(|node| node.id).collect::<Vec<_>>() {
        return Err(ExtractorError::InvalidInput(
            "anchor identities depend on line offsets".to_owned(),
        ));
    }
    Ok(())
}

fn source(path: &str, content: &str) -> SourceFile {
    SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
        },
        content: content.to_owned(),
    }
}

#[test]
fn extracts_nested_markdown_sections_with_containment_and_source_spans()
-> Result<(), ExtractorError> {
    let content = "# Guide\n\n## Design `facts`\n\n### Storage\n\n## Design `facts`\n";
    let first = DocumentationExtractor.extract(&source("docs/guide.md", content))?;
    let sections = first
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Section)
        .collect::<Vec<_>>();
    if sections
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>()
        != ["Guide", "Design facts", "Storage", "Design facts"]
        || first
            .edges
            .iter()
            .filter(|edge| sections.iter().any(|section| section.id == edge.target))
            .count()
            != sections.len()
        || sections.iter().any(|node| node.source.is_none())
    {
        return Err(ExtractorError::InvalidInput(format!(
            "unexpected Markdown sections: {sections:?}"
        )));
    }
    let shifted = DocumentationExtractor.extract(&source(
        "docs/guide.md",
        "intro\n\n# Guide\n\n## Design `facts`\n\n### Storage\n\n## Design `facts`\n",
    ))?;
    let shifted_ids = shifted
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Section)
        .map(|node| node.id)
        .collect::<Vec<_>>();
    let section_ids = sections.iter().map(|node| node.id).collect::<Vec<_>>();
    if shifted_ids != section_ids {
        return Err(ExtractorError::InvalidInput(
            "heading identities changed after line offsets moved".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn extracts_plain_text_paragraphs_under_the_document_root() -> Result<(), ExtractorError> {
    let extraction =
        DocumentationExtractor.extract(&source("notes/todo.txt", "one\ntwo\n\nthree\n"))?;
    let chunks = extraction
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::DocumentChunk)
        .collect::<Vec<_>>();
    if chunks
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>()
        != ["one\ntwo", "three"]
        || extraction.edges.len() != chunks.len()
        || chunks.iter().any(|node| node.source.is_none())
    {
        return Err(ExtractorError::InvalidInput(
            "plain-text paragraphs should be source-backed children of the document".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn extracts_searchable_verbatim_paragraphs_under_nearest_heading() -> Result<(), ExtractorError> {
    let markdown = "# Decision\n\n## Context\n\nReplay is costly for historic requests.\n\n## Decision\n\nUse layered checkpoints for bounded historical lookup.\n";
    let extracted = DocumentationExtractor.extract(&source("docs/adr.md", markdown))?;
    let chunks = extracted
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::DocumentChunk)
        .collect::<Vec<_>>();
    let sections = extracted
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Section)
        .collect::<Vec<_>>();
    let context = chunks
        .iter()
        .find(|node| node.name.starts_with("Replay is costly"))
        .ok_or_else(|| ExtractorError::InvalidInput("missing context prose".to_owned()))?;
    let decision = chunks
        .iter()
        .find(|node| node.name.starts_with("Use layered checkpoints"))
        .ok_or_else(|| ExtractorError::InvalidInput("missing decision prose".to_owned()))?;
    let edge_targets = extracted
        .edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::Contains)
        .map(|edge| (edge.source, edge.target))
        .collect::<Vec<_>>();
    let expected_context_parent = sections
        .iter()
        .find(|section| section.name == "Context")
        .ok_or_else(|| ExtractorError::InvalidInput("missing Context heading".to_owned()))?;
    let expected_decision_parent = sections
        .iter()
        .find(|section| {
            section.name == "Decision"
                && section.source.as_ref().is_some_and(|location| {
                    location.span.start_byte
                        > expected_context_parent
                            .source
                            .as_ref()
                            .map_or(0, |parent| parent.span.start_byte)
                })
        })
        .ok_or_else(|| ExtractorError::InvalidInput("missing Decision heading".to_owned()))?;
    if !edge_targets.contains(&(expected_context_parent.id, context.id))
        || !edge_targets.contains(&(expected_decision_parent.id, decision.id))
        || context.source.as_ref().is_none_or(|location| {
            markdown.get(location.span.start_byte as usize..location.span.end_byte as usize)
                != Some(context.name.as_str())
        })
    {
        return Err(ExtractorError::InvalidInput(
            "prose should remain searchable and attached to its exact source heading".to_owned(),
        ));
    }
    let shifted = DocumentationExtractor.extract(&source(
        "docs/adr.md",
        &format!("Unrelated introduction.\n\n{markdown}"),
    ))?;
    let mut shifted_chunks = shifted
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::DocumentChunk)
        .map(|node| node.id)
        .collect::<Vec<_>>();
    let mut original_chunks = chunks.iter().map(|node| node.id).collect::<Vec<_>>();
    shifted_chunks.sort_unstable();
    original_chunks.sort_unstable();
    if original_chunks
        .iter()
        .any(|original| !shifted_chunks.contains(original))
    {
        return Err(ExtractorError::InvalidInput(
            "existing document chunk identities changed after unrelated prose was inserted"
                .to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn indexes_markdown_procedure_code_and_table_content() -> Result<(), ExtractorError> {
    let markdown = "## Procedure\n\nRun this command:\n\n```bash\ncargo make ci\n```\n\n| State | Action |\n| --- | --- |\n| stale | rebuild the source graph |\n";
    let extraction = DocumentationExtractor.extract(&source("docs/how-to.md", markdown))?;
    let chunks = extraction
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::DocumentChunk)
        .collect::<Vec<_>>();
    if !chunks
        .iter()
        .any(|node| node.name.contains("cargo make ci"))
        || !chunks
            .iter()
            .any(|node| node.name.contains("rebuild the source graph"))
        || chunks.len() != 3
        || chunks.iter().any(|node| {
            node.source.as_ref().is_none_or(|location| {
                usize::try_from(location.span.start_byte)
                    .ok()
                    .zip(usize::try_from(location.span.end_byte).ok())
                    .is_none_or(|(start, end)| markdown.get(start..end) != Some(node.name.as_str()))
            })
        })
    {
        return Err(ExtractorError::InvalidInput(format!(
            "Markdown procedure blocks were not indexed verbatim: {chunks:?}"
        )));
    }
    Ok(())
}

#[test]
fn extracts_only_safe_local_indexed_file_links_with_normalized_targets()
-> Result<(), ExtractorError> {
    let markdown = "# Guide\n\n[ADR](../adr/decision.md#context)\n[Notes](../notes.txt?raw#top)\n[Root](/docs/start.md)\n[Rust](../../src/lib.rs#Engine)\n[Python](../../tools/index.py)\n[Config](../../Cargo.toml)\n[Web](https://example.test/doc.md)\n[Escape](../../../outside.md)\n";
    let extraction = DocumentationExtractor.extract(&source("docs/nested/index.md", markdown))?;
    let targets = extraction
        .references
        .iter()
        .map(|reference| reference.target.as_str())
        .collect::<Vec<_>>();
    if targets
        != [
            "docs/adr/decision.md#context",
            "docs/notes.txt",
            "docs/start.md",
            "src/lib.rs",
            "tools/index.py",
        ]
        || extraction.references.iter().any(|reference| {
            reference.source_location.span.start_byte >= reference.source_location.span.end_byte
        })
    {
        return Err(ExtractorError::InvalidInput(format!(
            "unexpected Markdown link targets: {targets:?}"
        )));
    }
    let shifted = DocumentationExtractor.extract(&source(
        "docs/nested/index.md",
        &format!("intro\n\n{markdown}"),
    ))?;
    let ids = extraction
        .references
        .iter()
        .map(|reference| reference.id)
        .collect::<Vec<_>>();
    let shifted_ids = shifted
        .references
        .iter()
        .map(|reference| reference.id)
        .collect::<Vec<_>>();
    if ids != shifted_ids {
        return Err(ExtractorError::InvalidInput(
            "Markdown link identities changed after line offsets moved".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn decodes_local_paths_once_before_boundary_checks() {
    for (input, expected) in [
        (
            "../design%20notes.md#decision",
            Some("docs/design notes.md#decision"),
        ),
        ("../r%C3%A9sum%C3%A9.md", Some("docs/résumé.md")),
        ("../a+b.txt", Some("docs/a+b.txt")),
        ("../100%25.txt", Some("docs/100%.txt")),
        ("../%252e%252e.md", Some("docs/%2e%2e.md")),
        ("../literal%ZZ.txt", Some("docs/literal%ZZ.txt")),
        ("%2Fdocs%2Fstart.md", Some("docs/start.md")),
        ("%2e%2e/%2e%2e/src/engine%2Ers", Some("src/engine.rs")),
        ("%2e%2e/%2e%2e/%2e%2e/outside.md", None),
        ("https%3A//example.test/file.md", None),
        ("%2F%2Fexample.test/file.md", None),
        ("../bad%FF.md", None),
        ("../bad%00.md", None),
        ("../bad%0A.md", None),
        ("../bad%5Cname.md", None),
        ("../bad%23name.md", None),
        ("../bad%3Fname.md", None),
    ] {
        assert_eq!(
            normalized_local_indexed_file_target("docs/nested/index.md", input).as_deref(),
            expected,
            "destination: {input}"
        );
    }
}

#[test]
fn classifies_only_explicit_code_links_in_adr_decisions_and_rfc_proposals()
-> Result<(), ExtractorError> {
    let adr = "# ADR-0112\n\n## Context\n\nSee [prior work](../../src/old.rs).\n\n## Decision\n\nChoose [engine](../../src/engine.rs#Engine) and retain [related ADR](0110-links.md).\n";
    let adr_extraction =
        DocumentationExtractor.extract(&source("docs/adr/0112-rationale.md", adr))?;
    let decisions = adr_extraction
        .references
        .iter()
        .filter(|reference| reference.relation != RelationKind::References)
        .collect::<Vec<_>>();
    let decision = decisions.first().ok_or_else(|| {
        ExtractorError::InvalidInput("ADR Decision link was not classified".to_owned())
    })?;
    if decisions.len() != 1
        || decision.target != "src/engine.rs"
        || decision.relation
            != (RelationKind::External {
                namespace: DOCUMENTATION_RELATION_NAMESPACE.to_owned(),
                relation: "decides".to_owned(),
            })
        || decision.source_location.span.start_byte >= decision.source_location.span.end_byte
        || adr_extraction
            .references
            .iter()
            .filter(|reference| reference.target != "src/engine.rs")
            .any(|reference| reference.relation != RelationKind::References)
    {
        return Err(ExtractorError::InvalidInput(format!(
            "unexpected ADR rationale references: {:?}",
            adr_extraction.references
        )));
    }

    let rfc = "# RFC-12\n\n## Proposed API\n\nExpose [engine](../../src/engine.py).\n\n## Alternatives\n\nCompare [other](../../src/other.py).\n";
    let rfc_extraction = DocumentationExtractor.extract(&source("docs/rfcs/0012-api.md", rfc))?;
    let [proposal_link, alternative_link] = rfc_extraction.references.as_slice() else {
        return Err(ExtractorError::InvalidInput(format!(
            "unexpected RFC reference count: {:?}",
            rfc_extraction.references
        )));
    };
    if proposal_link.relation
        != (RelationKind::External {
            namespace: DOCUMENTATION_RELATION_NAMESPACE.to_owned(),
            relation: "proposes".to_owned(),
        })
        || alternative_link.relation != RelationKind::References
    {
        return Err(ExtractorError::InvalidInput(format!(
            "unexpected RFC proposal references: {:?}",
            rfc_extraction.references
        )));
    }

    let ordinary_doc = "## Decision\n\nSee [engine](src/engine.rs).\n";
    let guide_extraction =
        DocumentationExtractor.extract(&source("docs/guide.md", ordinary_doc))?;
    if guide_extraction
        .references
        .iter()
        .any(|reference| reference.relation != RelationKind::References)
    {
        return Err(ExtractorError::InvalidInput(
            "ordinary documentation was classified as an ADR/RFC relationship".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn extracts_supersession_only_from_an_explicit_linked_adr_status() -> Result<(), ExtractorError> {
    let content = "# ADR-0065: Old decision\n\n- Status: superseded by [ADR-0066](0066-new-decision.md)\n- Date: 2026-09-28\n\n## Context\n\nSee [related ADR](0010-related.md).\n";
    let source_file = source("docs/adr/0065-old-decision.md", content);
    let extraction = DocumentationExtractor.extract(&source_file)?;
    let [superseded, related] = extraction.references.as_slice() else {
        return Err(ExtractorError::InvalidInput(format!(
            "unexpected ADR link count: {:?}",
            extraction.references
        )));
    };
    let document = extraction
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Document)
        .ok_or_else(|| ExtractorError::InvalidInput("document node is missing".to_owned()))?;
    if superseded.target != "docs/adr/0066-new-decision.md"
        || superseded.source != document.id
        || superseded.relation
            != (RelationKind::External {
                namespace: DOCUMENTATION_RELATION_NAMESPACE.to_owned(),
                relation: "superseded_by".to_owned(),
            })
        || related.relation != RelationKind::References
        || content
            .get(
                superseded.source_location.span.start_byte as usize
                    ..superseded.source_location.span.end_byte as usize,
            )
            .is_none_or(|text| !text.contains("ADR-0066"))
    {
        return Err(ExtractorError::InvalidInput(format!(
            "ADR status supersession was not preserved as source-grounded metadata: {:?}",
            extraction.references
        )));
    }

    let non_status = "# ADR-0067: Current decision\n\n- Status: accepted; informed by [ADR-0066](0066-new-decision.md)\n";
    let non_status_extraction =
        DocumentationExtractor.extract(&source("docs/adr/0067-current-decision.md", non_status))?;
    if non_status_extraction
        .references
        .iter()
        .any(|reference| reference.relation != (RelationKind::References))
    {
        return Err(ExtractorError::InvalidInput(
            "non-supersession status link received a supersession relation".to_owned(),
        ));
    }
    Ok(())
}
