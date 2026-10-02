use std::collections::BTreeMap;
use std::path::Path;

use syntaxmesh_core::{
    Edge, EvidenceClass, Node, NodeId, NodeKind, Provenance, ProvenanceId, RelationKind,
    SourceLocation, SourceSpan,
};
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, Reference, SourceFile,
};
use tree_sitter::{Node as SyntaxNode, Parser};

const PRODUCER_NAMESPACE: &str = "syntaxmesh.lang.bash";
const EXTRACTOR_VERSION: &str = "1";
const GRAMMAR_VERSION: &str = "0.25.1";

#[derive(Debug, Clone, Copy, Default)]
pub struct BashExtractor;

impl LanguageExtractor for BashExtractor {
    fn language(&self) -> &'static str {
        "bash"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new(
            PRODUCER_NAMESPACE,
            format!(
                "{};tree-sitter-bash={GRAMMAR_VERSION};extractor={EXTRACTOR_VERSION}",
                env!("CARGO_PKG_VERSION")
            ),
        )
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        if !Path::new(&source.file.normalized_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("sh") || extension.eq_ignore_ascii_case("bash")
            })
        {
            return Err(ExtractorError::UnsupportedFile(
                source.file.normalized_path.clone(),
            ));
        }
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let tree = parser
            .parse(source.content.as_bytes(), None)
            .ok_or_else(|| ExtractorError::InvalidInput("Bash parse was cancelled".to_owned()))?;
        if tree.root_node().has_error() {
            return Err(ExtractorError::SyntaxError(
                "Bash source contains syntax errors".to_owned(),
            ));
        }

        let identity = self.producer_identity();
        let provenance = provenance(source, identity)?;
        let script_id = NodeId::derive(&[&source.file.file_id.0.0, b"bash-script"]);
        let mut builder = BashExtraction {
            source,
            provenance: provenance.id,
            nodes: vec![node(
                source,
                provenance.id,
                script_id,
                NodeKind::Script,
                &source.file.normalized_path,
                tree.root_node().byte_range(),
            )?],
            edges: Vec::new(),
            references: Vec::new(),
            function_counts: BTreeMap::new(),
            reference_counts: BTreeMap::new(),
        };
        builder.visit(tree.root_node(), script_id)?;
        Ok(Extraction {
            provenance,
            nodes: builder.nodes,
            edges: builder.edges,
            references: builder.references,
            imports: Vec::new(),
            exports: Vec::new(),
        })
    }
}

struct BashExtraction<'source> {
    source: &'source SourceFile,
    provenance: ProvenanceId,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    references: Vec<Reference>,
    function_counts: BTreeMap<String, usize>,
    reference_counts: BTreeMap<(NodeId, String), usize>,
}

impl BashExtraction<'_> {
    fn visit(&mut self, syntax: SyntaxNode<'_>, owner: NodeId) -> Result<(), ExtractorError> {
        if syntax.kind() == "function_definition" {
            let Some(name_node) = syntax.child_by_field_name("name") else {
                return Err(ExtractorError::InvalidInput(
                    "Bash function has no name node".to_owned(),
                ));
            };
            let name = text(self.source, name_node)?;
            let duplicate_index = self.function_counts.entry(name.clone()).or_default();
            let ordinal = *duplicate_index;
            *duplicate_index = duplicate_index.checked_add(1).ok_or_else(|| {
                ExtractorError::InvalidInput("function count overflow".to_owned())
            })?;
            let id = NodeId::derive(&[
                &self.source.file.file_id.0.0,
                b"bash-function",
                name.as_bytes(),
                &ordinal.to_le_bytes(),
            ]);
            self.nodes.push(node(
                self.source,
                self.provenance,
                id,
                NodeKind::Function,
                &name,
                syntax.byte_range(),
            )?);
            self.edges
                .push(edge(owner, id, RelationKind::Contains, self.provenance));
            let mut cursor = syntax.walk();
            for child in syntax.children(&mut cursor) {
                self.visit(child, id)?;
            }
            return Ok(());
        }

        if syntax.kind() == "command"
            && let Some(name_node) = syntax.child_by_field_name("name")
            && name_node.named_child_count() == 1
        {
            let mut cursor = name_node.walk();
            if let Some(word) = name_node.named_children(&mut cursor).next() {
                let target = text(self.source, word)?;
                if is_static_command_name(&target) {
                    self.add_reference(owner, target, word.byte_range())?;
                }
            }
        }

        let mut cursor = syntax.walk();
        for child in syntax.children(&mut cursor) {
            self.visit(child, owner)?;
        }
        Ok(())
    }

    fn add_reference(
        &mut self,
        owner: NodeId,
        target: String,
        byte_range: std::ops::Range<usize>,
    ) -> Result<(), ExtractorError> {
        let occurrence = self
            .reference_counts
            .entry((owner, target.clone()))
            .or_default();
        let ordinal = *occurrence;
        *occurrence = occurrence
            .checked_add(1)
            .ok_or_else(|| ExtractorError::InvalidInput("command count overflow".to_owned()))?;
        let source_location = location(self.source, byte_range)?;
        self.references.push(Reference {
            resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
            id: NodeId::derive(&[
                &owner.0.0,
                b"bash-command-reference",
                target.as_bytes(),
                &ordinal.to_le_bytes(),
            ]),
            source: owner,
            target,
            relation: RelationKind::Calls,
            source_location,
            provenance: self.provenance,
        });
        Ok(())
    }
}

fn is_static_command_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./:+-".contains(&byte))
}

fn text(source: &SourceFile, syntax: SyntaxNode<'_>) -> Result<String, ExtractorError> {
    syntax
        .utf8_text(source.content.as_bytes())
        .map(str::to_owned)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))
}

fn provenance(
    source: &SourceFile,
    identity: ExtractorIdentity,
) -> Result<Provenance, ExtractorError> {
    let source_span = whole_file_span(source)?;
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
            span: source_span,
        }),
    })
}

fn node(
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
        source: Some(location(source, byte_range)?),
        provenance,
        extension_payload: None,
    })
}

fn edge(source: NodeId, target: NodeId, relation: RelationKind, provenance: ProvenanceId) -> Edge {
    Edge {
        id: syntaxmesh_core::EdgeId::derive(&[
            &source.0.0,
            &target.0.0,
            format!("{relation:?}").as_bytes(),
        ]),
        source,
        target,
        relation,
        provenance,
        extension_payload: None,
    }
}

fn whole_file_span(source: &SourceFile) -> Result<SourceSpan, ExtractorError> {
    let end = u64::try_from(source.content.len())
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    SourceSpan::new(0, end).map_err(|error| ExtractorError::InvalidInput(error.to_string()))
}

fn location(
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

#[cfg(test)]
mod tests {
    use syntaxmesh_core::{FileId, FileVersion};

    use super::*;

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
    fn extracts_functions_and_static_command_calls_with_evidence() -> Result<(), ExtractorError> {
        let extraction = BashExtractor.extract(&source(
            "scripts/build.sh",
            "#!/usr/bin/env bash\nbuild() { cargo build; echo \"hi\"; }\nbuild\n$DYNAMIC\n",
        ))?;
        let functions = extraction
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Function)
            .collect::<Vec<_>>();
        if functions.len() != 1 || functions.first().is_none_or(|node| node.name != "build") {
            return Err(ExtractorError::InvalidInput(
                "Bash function declaration was not extracted".to_owned(),
            ));
        }
        let targets = extraction
            .references
            .iter()
            .map(|reference| reference.target.as_str())
            .collect::<Vec<_>>();
        if targets != ["cargo", "echo", "build"]
            || extraction.edges.len() != 1
            || extraction
                .edges
                .first()
                .is_none_or(|edge| edge.relation != RelationKind::Contains)
        {
            return Err(ExtractorError::InvalidInput(format!(
                "unexpected Bash facts: targets={targets:?}, edges={:?}",
                extraction.edges
            )));
        }
        Ok(())
    }

    #[test]
    fn rejects_non_bash_files_and_syntax_errors() {
        assert!(matches!(
            BashExtractor.extract(&source("script.py", "echo hi")),
            Err(ExtractorError::UnsupportedFile(_))
        ));
        assert!(matches!(
            BashExtractor.extract(&source("script.sh", "if then")),
            Err(ExtractorError::SyntaxError(_))
        ));
    }
}
