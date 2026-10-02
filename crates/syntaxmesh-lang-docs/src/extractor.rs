use std::collections::BTreeMap;
use std::path::{Component, Path};

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, Node, NodeId, NodeKind, Provenance, ProvenanceId, RelationKind,
    SourceLocation, SourceSpan,
};
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, Reference,
    SOURCE_ANCHOR_NAMESPACE, SourceFile,
};

const PRODUCER_NAMESPACE: &str = "syntaxmesh.lang.documentation";
const EXTRACTOR_VERSION: &str = "8";
const MARKDOWN_VERSION: &str = "0.13.4";
const DOCUMENTATION_RELATION_NAMESPACE: &str = "syntaxmesh.documentation";

#[derive(Debug, Clone, Copy, Default)]
pub struct DocumentationExtractor;

impl LanguageExtractor for DocumentationExtractor {
    fn language(&self) -> &'static str {
        "documentation"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new(
            PRODUCER_NAMESPACE,
            format!(
                "{};pulldown-cmark={MARKDOWN_VERSION};github-slugger=0.1.0;extractor={EXTRACTOR_VERSION}",
                env!("CARGO_PKG_VERSION")
            ),
        )
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        let extension = Path::new(&source.file.normalized_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| ExtractorError::UnsupportedFile(source.file.normalized_path.clone()))?;
        let is_markdown = matches!(extension.as_str(), "md" | "markdown");
        let is_plain_text = matches!(
            extension.as_str(),
            "txt" | "text" | "rst" | "adoc" | "asciidoc"
        );
        if !is_markdown && !is_plain_text {
            return Err(ExtractorError::UnsupportedFile(
                source.file.normalized_path.clone(),
            ));
        }

        let identity = self.producer_identity();
        let provenance = provenance(source, identity)?;
        let document_id = NodeId::derive(&[&source.file.file_id.0.0, b"documentation-root"]);
        let mut nodes = vec![make_node(
            source,
            provenance.id,
            document_id,
            NodeKind::Document,
            &source.file.normalized_path,
            0..source.content.len(),
        )?];
        let mut edges = Vec::new();
        let mut references = Vec::new();
        let mut paragraph_ranges = Vec::new();
        let mut paragraph_start = None;
        let mut nonparagraph_block_start = None;
        let mut section_owners = Vec::new();

        if is_markdown {
            let architecture_document = architecture_document_kind(&source.file.normalized_path);
            let mut heading: Option<HeadingBuilder> = None;
            let mut slugger = github_slugger::Slugger::default();
            let mut ancestors: Vec<(u8, NodeId)> = Vec::new();
            let mut section_titles = BTreeMap::<NodeId, String>::new();
            let mut duplicate_counts: BTreeMap<(NodeId, String), usize> = BTreeMap::new();
            let mut link_counts: BTreeMap<(NodeId, String), usize> = BTreeMap::new();
            let options = Options::ENABLE_TABLES;
            for (event, range) in Parser::new_ext(&source.content, options).into_offset_iter() {
                match event {
                    Event::Start(Tag::Paragraph) => paragraph_start = Some(range.start),
                    Event::End(TagEnd::Paragraph) => {
                        if let Some(start) = paragraph_start.take() {
                            paragraph_ranges.push(start..range.end);
                        }
                    }
                    Event::Start(Tag::CodeBlock(_) | Tag::Table(_) | Tag::HtmlBlock) => {
                        nonparagraph_block_start = Some(range.start);
                    }
                    Event::End(TagEnd::CodeBlock | TagEnd::Table | TagEnd::HtmlBlock) => {
                        if let Some(start) = nonparagraph_block_start.take() {
                            paragraph_ranges.push(start..range.end);
                        }
                    }
                    Event::Start(Tag::Link { dest_url, .. }) => {
                        if let Some(target) = normalized_local_indexed_file_target(
                            &source.file.normalized_path,
                            &dest_url,
                        ) {
                            let status_relation = explicit_supersession_relation(
                                &source.file.normalized_path,
                                &source.content,
                                range.start,
                                &target,
                            );
                            let owner = if status_relation.is_some() {
                                document_id
                            } else {
                                ancestors.last().map_or(document_id, |(_, id)| *id)
                            };
                            let occurrence =
                                link_counts.entry((owner, target.clone())).or_default();
                            let ordinal = *occurrence;
                            *occurrence = occurrence.checked_add(1).ok_or_else(|| {
                                ExtractorError::InvalidInput("link count overflow".to_owned())
                            })?;
                            let relation = status_relation
                                .or_else(|| {
                                    ancestors
                                        .last()
                                        .and_then(|(_, section)| section_titles.get(section))
                                        .and_then(|title| {
                                            architecture_relation(
                                                architecture_document,
                                                title,
                                                &target,
                                            )
                                        })
                                })
                                .unwrap_or(RelationKind::References);
                            references.push(Reference {
                                resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
                                id: NodeId::derive(&[
                                    &owner.0.0,
                                    b"markdown-document-link",
                                    target.as_bytes(),
                                    &ordinal.to_le_bytes(),
                                ]),
                                source: owner,
                                target,
                                relation,
                                source_location: make_location(source, range)?,
                                provenance: provenance.id,
                            });
                        }
                    }
                    Event::Start(Tag::Heading { level, .. }) => {
                        heading = Some(HeadingBuilder {
                            level: heading_level(level),
                            title: String::new(),
                            start: range.start,
                        });
                    }
                    Event::Text(text) | Event::Code(text) => {
                        if let Some(current) = &mut heading {
                            current.title.push_str(&text);
                        }
                    }
                    Event::End(TagEnd::Heading(level)) => {
                        let Some(current) = heading.take() else {
                            return Err(ExtractorError::InvalidInput(
                                "Markdown heading end without a start".to_owned(),
                            ));
                        };
                        let title = current.title.trim().to_owned();
                        if title.is_empty() {
                            continue;
                        }
                        while ancestors
                            .last()
                            .is_some_and(|(ancestor_level, _)| *ancestor_level >= current.level)
                        {
                            ancestors.pop();
                        }
                        let parent = ancestors.last().map_or(document_id, |(_, id)| *id);
                        let duplicate_index =
                            duplicate_counts.entry((parent, title.clone())).or_default();
                        let ordinal = *duplicate_index;
                        *duplicate_index = duplicate_index.checked_add(1).ok_or_else(|| {
                            ExtractorError::InvalidInput("heading count overflow".to_owned())
                        })?;
                        let id = NodeId::derive(&[
                            &parent.0.0,
                            b"markdown-section",
                            title.as_bytes(),
                            &ordinal.to_le_bytes(),
                        ]);
                        let end = range.end.max(current.start);
                        nodes.push(make_node(
                            source,
                            provenance.id,
                            id,
                            NodeKind::Section,
                            &title,
                            current.start..end,
                        )?);
                        let slug = slugger.slug(&title);
                        if !slug.is_empty() {
                            let name = format!("{}#{slug}", source.file.normalized_path);
                            let anchor_id = NodeId::derive(&[
                                &source.file.file_id.0.0,
                                b"source-anchor-v1",
                                name.as_bytes(),
                            ]);
                            nodes.push(make_node(
                                source,
                                provenance.id,
                                anchor_id,
                                NodeKind::External {
                                    namespace: SOURCE_ANCHOR_NAMESPACE.to_owned(),
                                    kind: "anchor".to_owned(),
                                },
                                &name,
                                current.start..end,
                            )?);
                            edges.push(contains_edge(id, anchor_id, provenance.id));
                        }
                        section_titles.insert(id, title);
                        section_owners.push((current.start, id));
                        edges.push(contains_edge(parent, id, provenance.id));
                        ancestors.push((heading_level(level), id));
                    }
                    Event::Start(_)
                    | Event::End(_)
                    | Event::InlineMath(_)
                    | Event::DisplayMath(_)
                    | Event::Html(_)
                    | Event::InlineHtml(_)
                    | Event::FootnoteReference(_)
                    | Event::Rule
                    | Event::TaskListMarker(_) => {}
                    Event::SoftBreak | Event::HardBreak => {
                        if let Some(current) = &mut heading {
                            current.title.push(' ');
                        }
                    }
                }
            }
            if heading.is_some() {
                return Err(ExtractorError::InvalidInput(
                    "Markdown heading was not closed".to_owned(),
                ));
            }
        } else {
            paragraph_ranges.extend(plain_text_paragraph_ranges(&source.content));
        }

        let mut chunk_counts: BTreeMap<(NodeId, String), usize> = BTreeMap::new();
        for range in paragraph_ranges {
            let Some(range) = trim_range(&source.content, range) else {
                continue;
            };
            let content = source.content[range.clone()].to_owned();
            let parent = section_owners
                .iter()
                .filter(|(start, _)| *start < range.start)
                .max_by_key(|(start, _)| *start)
                .map_or(document_id, |(_, id)| *id);
            let duplicate_index = chunk_counts.entry((parent, content.clone())).or_default();
            let ordinal = *duplicate_index;
            *duplicate_index = duplicate_index.checked_add(1).ok_or_else(|| {
                ExtractorError::InvalidInput("document chunk count overflow".to_owned())
            })?;
            let id = NodeId::derive(&[
                &parent.0.0,
                b"documentation-chunk",
                content.as_bytes(),
                &ordinal.to_le_bytes(),
            ]);
            nodes.push(make_node(
                source,
                provenance.id,
                id,
                NodeKind::DocumentChunk,
                &content,
                range,
            )?);
            edges.push(contains_edge(parent, id, provenance.id));
        }

        Ok(Extraction {
            provenance,
            nodes,
            edges,
            references,
            imports: Vec::new(),
            exports: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchitectureDocumentKind {
    Adr,
    Rfc,
}

fn architecture_document_kind(path: &str) -> Option<ArchitectureDocumentKind> {
    let path = Path::new(path);
    let file_name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let directories = path
        .parent()?
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if directories.iter().any(|component| component == "adr") || file_name.starts_with("adr-") {
        Some(ArchitectureDocumentKind::Adr)
    } else if directories
        .iter()
        .any(|component| matches!(component.as_str(), "rfc" | "rfcs"))
        || file_name.starts_with("rfc-")
    {
        Some(ArchitectureDocumentKind::Rfc)
    } else {
        None
    }
}

fn architecture_relation(
    document: Option<ArchitectureDocumentKind>,
    section_title: &str,
    target: &str,
) -> Option<RelationKind> {
    let extension = Path::new(target)
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "sh" | "bash"
    ) {
        return None;
    }
    let section = section_title.trim().to_ascii_lowercase();
    let relation = match document? {
        ArchitectureDocumentKind::Adr if section == "decision" => "decides",
        ArchitectureDocumentKind::Rfc
            if matches!(
                section.as_str(),
                "proposal" | "proposed api" | "api proposal"
            ) =>
        {
            "proposes"
        }
        ArchitectureDocumentKind::Adr | ArchitectureDocumentKind::Rfc => return None,
    };
    Some(RelationKind::External {
        namespace: DOCUMENTATION_RELATION_NAMESPACE.to_owned(),
        relation: relation.to_owned(),
    })
}

fn explicit_supersession_relation(
    source_path: &str,
    content: &str,
    link_start: usize,
    target: &str,
) -> Option<RelationKind> {
    if architecture_document_kind(source_path) != Some(ArchitectureDocumentKind::Adr)
        || architecture_document_kind(target) != Some(ArchitectureDocumentKind::Adr)
    {
        return None;
    }
    let before_link = content.get(..link_start)?;
    let line_start = match before_link.rfind('\n') {
        Some(newline) => newline.checked_add(1)?,
        None => 0,
    };
    let link_prefix = content
        .get(line_start..link_start)?
        .trim_start()
        .to_ascii_lowercase();
    let status = link_prefix
        .strip_prefix("- status:")
        .or_else(|| link_prefix.strip_prefix("status:"))?;
    if status.trim() != "superseded by" {
        return None;
    }
    Some(RelationKind::External {
        namespace: DOCUMENTATION_RELATION_NAMESPACE.to_owned(),
        relation: "superseded_by".to_owned(),
    })
}

fn trim_range(content: &str, range: std::ops::Range<usize>) -> Option<std::ops::Range<usize>> {
    let value = content.get(range.clone())?;
    let leading = value.len().checked_sub(value.trim_start().len())?;
    let trailing = value.trim_end().len();
    if leading >= trailing {
        return None;
    }
    Some(range.start.checked_add(leading)?..range.start.checked_add(trailing)?)
}

fn plain_text_paragraph_ranges(content: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (line_start, line) in content.split_inclusive('\n').scan(0usize, |offset, line| {
        let current_line_start = *offset;
        let next_offset = offset.checked_add(line.len())?;
        *offset = next_offset;
        Some((current_line_start, line))
    }) {
        if line.trim().is_empty() {
            if let Some(paragraph_start) = start.take() {
                ranges.push(paragraph_start..line_start);
            }
        } else if start.is_none() {
            start = Some(line_start);
        }
    }
    if let Some(paragraph_start) = start {
        ranges.push(paragraph_start..content.len());
    }
    ranges
}

fn normalized_local_indexed_file_target(source_path: &str, destination: &str) -> Option<String> {
    let destination = destination.trim();
    let (destination, fragment) = destination
        .split_once('#')
        .map_or((destination, None), |(path, fragment)| {
            (path, Some(fragment))
        });
    let destination = destination.split('?').next()?.trim();
    if destination.is_empty() {
        let extension = Path::new(source_path)
            .extension()?
            .to_str()?
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "md" | "markdown") {
            return None;
        }
        let fragment = decoded_fragment(fragment?)?;
        return Some(format!("{source_path}#{fragment}"));
    }
    let destination = percent_encoding::percent_decode_str(destination)
        .decode_utf8()
        .ok()?;
    if destination.chars().any(char::is_control)
        || destination.contains(['\\', '#', '?'])
        || destination.starts_with("//")
    {
        return None;
    }
    if destination.find(':').is_some_and(|colon| {
        destination[..colon]
            .find('/')
            .is_none_or(|slash| colon < slash)
    }) {
        return None;
    }
    let extension = Path::new(destination.as_ref())
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)?;
    if !matches!(
        extension.as_str(),
        "rs" | "py"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "sh"
            | "bash"
            | "md"
            | "markdown"
            | "txt"
            | "text"
            | "rst"
            | "adoc"
            | "asciidoc"
    ) {
        return None;
    }

    let mut components = Vec::new();
    if !destination.starts_with('/') {
        for component in Path::new(source_path).parent()?.components() {
            match component {
                Component::Normal(value) => components.push(value.to_str()?.to_owned()),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }
    }
    for component in destination.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            value => components.push(value.to_owned()),
        }
    }
    if components.is_empty() {
        return None;
    }
    let path = components.join("/");
    if matches!(extension.as_str(), "md" | "markdown")
        && fragment.is_some_and(|fragment| !fragment.is_empty())
    {
        Some(format!("{path}#{}", decoded_fragment(fragment?)?))
    } else {
        Some(path)
    }
}

fn decoded_fragment(fragment: &str) -> Option<String> {
    let decoded = percent_encoding::percent_decode_str(fragment)
        .decode_utf8()
        .ok()?;
    (!decoded.is_empty() && !decoded.chars().any(char::is_control)).then(|| decoded.into_owned())
}

struct HeadingBuilder {
    level: u8,
    title: String,
    start: usize,
}

const fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn provenance(
    source: &SourceFile,
    identity: ExtractorIdentity,
) -> Result<Provenance, ExtractorError> {
    let end = u64::try_from(source.content.len())
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let span =
        SourceSpan::new(0, end).map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    Ok(Provenance {
        id: ProvenanceId::derive(&[
            identity.namespace.as_bytes(),
            &source.file.file_id.0.0,
            source.file.content_hash.as_slice(),
        ]),
        producer_namespace: identity.namespace,
        producer_version: identity.version,
        evidence_class: EvidenceClass::SourceFact,
        source: Some(SourceLocation {
            file_id: source.file.file_id,
            content_hash: source.file.content_hash,
            span,
        }),
    })
}

fn make_node(
    source: &SourceFile,
    provenance: ProvenanceId,
    id: NodeId,
    kind: NodeKind,
    name: &str,
    byte_range: std::ops::Range<usize>,
) -> Result<Node, ExtractorError> {
    Ok(Node {
        id,
        kind,
        name: name.to_owned(),
        owner_file: Some(source.file.file_id),
        source: Some(make_location(source, byte_range)?),
        provenance,
        extension_payload: None,
    })
}

fn make_location(
    source: &SourceFile,
    byte_range: std::ops::Range<usize>,
) -> Result<SourceLocation, ExtractorError> {
    let start = u64::try_from(byte_range.start)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let end = u64::try_from(byte_range.end)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    Ok(SourceLocation {
        file_id: source.file.file_id,
        content_hash: source.file.content_hash,
        span: SourceSpan::new(start, end)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?,
    })
}

fn contains_edge(source: NodeId, target: NodeId, provenance: ProvenanceId) -> Edge {
    Edge {
        id: EdgeId::derive(&[&source.0.0, &target.0.0, b"contains"]),
        source,
        target,
        relation: RelationKind::Contains,
        provenance,
        extension_payload: None,
    }
}

#[cfg(test)]
mod tests;
