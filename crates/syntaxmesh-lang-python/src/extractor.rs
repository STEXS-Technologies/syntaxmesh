//! Tree-sitter-backed Python declarations and call-reference extraction.

use std::collections::BTreeMap;
use std::path::Path;

use syntaxmesh_core::{
    EvidenceClass, ImportKind, ImportRecord, ImportRecordId, Node, NodeId, NodeKind, Provenance,
    ProvenanceId, RelationKind, SourceLocation, SourceSpan,
};
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, Reference, SourceFile,
};
use tree_sitter::{Node as SyntaxNode, Parser};

const PRODUCER_NAMESPACE: &str = "syntaxmesh.lang.python";
const EXTRACTOR_SEMANTIC_VERSION: &str = "4";
const GRAMMAR_VERSION: &str = "0.25.0";

#[derive(Debug, Clone, Copy, Default)]
pub struct PythonExtractor;

impl LanguageExtractor for PythonExtractor {
    fn language(&self) -> &'static str {
        "python"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new(
            PRODUCER_NAMESPACE,
            format!(
                "{};tree-sitter-python={GRAMMAR_VERSION};extractor={EXTRACTOR_SEMANTIC_VERSION}",
                env!("CARGO_PKG_VERSION")
            ),
        )
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        if !Path::new(&source.file.normalized_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
        {
            return Err(ExtractorError::UnsupportedFile(
                source.file.normalized_path.clone(),
            ));
        }

        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let tree = parser
            .parse(source.content.as_bytes(), None)
            .ok_or_else(|| ExtractorError::InvalidInput("Python parse was cancelled".to_owned()))?;
        if tree.root_node().has_error() {
            return Err(ExtractorError::SyntaxError(
                "Python source contains syntax errors".to_owned(),
            ));
        }

        let identity = self.producer_identity();
        let content_len = u64::try_from(source.content.len())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let provenance = Provenance {
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
                span: SourceSpan::new(0, content_len)
                    .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?,
            }),
        };

        let mut extraction = ExtractionBuilder::new(source, provenance.id)?;
        extraction.visit(tree.root_node(), None)?;

        Ok(Extraction {
            provenance,
            nodes: extraction.nodes,
            edges: Vec::new(),
            references: extraction.references,
            imports: extraction.imports,
            exports: Vec::new(),
        })
    }
}

struct ExtractionBuilder<'source> {
    source: &'source SourceFile,
    provenance: ProvenanceId,
    scope: Vec<String>,
    class_scope: Vec<String>,
    duplicate_counts: BTreeMap<(String, String), usize>,
    reference_counts: BTreeMap<(NodeId, String), usize>,
    import_counts: BTreeMap<(ImportKind, String, Option<String>, Option<String>), usize>,
    nodes: Vec<Node>,
    references: Vec<Reference>,
    imports: Vec<ImportRecord>,
}

impl<'source> ExtractionBuilder<'source> {
    fn new(source: &'source SourceFile, provenance: ProvenanceId) -> Result<Self, ExtractorError> {
        let content_len = u64::try_from(source.content.len())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let span = SourceSpan::new(0, content_len)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let module_id = module_id(source);
        let module = Node {
            id: module_id,
            kind: NodeKind::Module,
            name: source.file.normalized_path.clone(),
            owner_file: Some(source.file.file_id),
            source: Some(SourceLocation {
                file_id: source.file.file_id,
                content_hash: source.file.content_hash,
                span,
            }),
            provenance,
            extension_payload: None,
        };
        Ok(Self {
            source,
            provenance,
            scope: Vec::new(),
            class_scope: Vec::new(),
            duplicate_counts: BTreeMap::new(),
            reference_counts: BTreeMap::new(),
            import_counts: BTreeMap::new(),
            nodes: vec![module],
            references: Vec::new(),
            imports: Vec::new(),
        })
    }

    fn visit(
        &mut self,
        syntax: SyntaxNode<'_>,
        owner: Option<NodeId>,
    ) -> Result<(), ExtractorError> {
        let mut child_owner = owner;
        let mut pushed_scope = false;
        let mut pushed_class_scope = false;
        match syntax.kind() {
            "class_definition" => {
                let declaration = self.add_declaration(syntax, NodeKind::Class)?;
                child_owner = Some(declaration.0);
                self.scope.push(declaration.1.clone());
                self.class_scope.push(declaration.1);
                pushed_scope = true;
                pushed_class_scope = true;
            }
            "function_definition" => {
                let declaration = self.add_declaration(syntax, NodeKind::Function)?;
                child_owner = Some(declaration.0);
                self.scope.push(declaration.1);
                pushed_scope = true;
            }
            "call" => {
                if let Some(source_node) = owner {
                    self.add_call_reference(syntax, source_node)?;
                }
            }
            "import_statement" => self.add_module_imports(syntax)?,
            "import_from_statement" => self.add_from_imports(syntax)?,
            _ => {}
        }

        let mut cursor = syntax.walk();
        for child in syntax.children(&mut cursor) {
            self.visit(child, child_owner)?;
        }
        if pushed_scope {
            self.scope.truncate(self.scope.len().saturating_sub(1));
        }
        if pushed_class_scope {
            self.class_scope
                .truncate(self.class_scope.len().saturating_sub(1));
        }
        Ok(())
    }

    fn add_declaration(
        &mut self,
        syntax: SyntaxNode<'_>,
        kind: NodeKind,
    ) -> Result<(NodeId, String), ExtractorError> {
        let name_node = syntax
            .child_by_field_name("name")
            .ok_or_else(|| ExtractorError::InvalidInput("declaration has no name".to_owned()))?;
        let name = syntax_text(name_node, self.source)?;
        let qualified_name = if self.scope.is_empty() {
            name.clone()
        } else {
            format!("{}::{name}", self.scope.join("::"))
        };
        let kind_name = format!("{kind:?}");
        let duplicate_count = self
            .duplicate_counts
            .entry((kind_name.clone(), qualified_name.clone()))
            .or_default();
        let identity_name = if *duplicate_count == 0 {
            qualified_name.clone()
        } else {
            format!("{qualified_name}#{}", *duplicate_count)
        };
        *duplicate_count = (*duplicate_count).saturating_add(1);

        let evidence = if kind == NodeKind::Function {
            syntax
        } else {
            name_node
        };
        let start = u64::try_from(evidence.start_byte())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = u64::try_from(evidence.end_byte())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let span = SourceSpan::new(start, end)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let file_bytes = self.source.file.file_id.0.0;
        let id = NodeId::derive(&[&file_bytes, kind_name.as_bytes(), identity_name.as_bytes()]);
        self.nodes.push(Node {
            id,
            kind,
            name: qualified_name,
            owner_file: Some(self.source.file.file_id),
            source: Some(SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span,
            }),
            provenance: self.provenance,
            extension_payload: None,
        });
        Ok((id, name))
    }

    fn add_call_reference(
        &mut self,
        call: SyntaxNode<'_>,
        source_node: NodeId,
    ) -> Result<(), ExtractorError> {
        let Some(function) = call.child_by_field_name("function") else {
            return Ok(());
        };
        let target = if function.kind() == "attribute" {
            let attribute = function
                .child_by_field_name("attribute")
                .unwrap_or(function);
            let object = function.child_by_field_name("object");
            let object_text = object
                .map(|node| syntax_text(node, self.source))
                .transpose()?;
            if matches!(object_text.as_deref(), Some("self" | "cls"))
                && !self.class_scope.is_empty()
            {
                format!(
                    "{}::{}",
                    self.class_scope.join("::"),
                    syntax_text(attribute, self.source)?
                )
            } else {
                syntax_text(function, self.source)?
            }
        } else {
            syntax_text(function, self.source)?
        };
        let count = self
            .reference_counts
            .entry((source_node, target.clone()))
            .or_default();
        let occurrence = (*count).to_le_bytes();
        *count = (*count).saturating_add(1);
        let source_bytes = source_node.0.0;
        let id = NodeId::derive(&[
            &source_bytes,
            b"call-reference-v1",
            target.as_bytes(),
            &occurrence,
        ]);
        let target_span_node = if function.kind() == "attribute" {
            function
                .child_by_field_name("attribute")
                .unwrap_or(function)
        } else {
            function
        };
        let start = u64::try_from(target_span_node.start_byte())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = u64::try_from(target_span_node.end_byte())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let span = SourceSpan::new(start, end)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        self.references.push(Reference {
            resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
            id,
            source: source_node,
            target,
            relation: RelationKind::Calls,
            source_location: SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span,
            },
            provenance: self.provenance,
        });
        Ok(())
    }

    fn add_module_imports(&mut self, syntax: SyntaxNode<'_>) -> Result<(), ExtractorError> {
        for index in 0..syntax.child_count() {
            if syntax.field_name_for_child(index) != Some("name") {
                continue;
            }
            let Some(binding) = syntax.child(index) else {
                continue;
            };
            let (name_node, alias_node) = if binding.kind() == "aliased_import" {
                (
                    binding.child_by_field_name("name").ok_or_else(|| {
                        ExtractorError::InvalidInput("aliased import has no module name".to_owned())
                    })?,
                    binding.child_by_field_name("alias"),
                )
            } else {
                (binding, None)
            };
            let specifier = syntax_text(name_node, self.source)?;
            let local_name = if let Some(alias) = alias_node {
                syntax_text(alias, self.source)?
            } else {
                specifier.split('.').next().unwrap_or_default().to_owned()
            };
            self.record_import(
                &specifier,
                ImportKind::PythonModule,
                None,
                Some(local_name),
                binding,
            )?;
        }
        Ok(())
    }

    fn add_from_imports(&mut self, syntax: SyntaxNode<'_>) -> Result<(), ExtractorError> {
        let module = syntax.child_by_field_name("module_name").ok_or_else(|| {
            ExtractorError::InvalidInput("from-import has no module specifier".to_owned())
        })?;
        let specifier = syntax_text(module, self.source)?;
        for index in 0..syntax.child_count() {
            if syntax.field_name_for_child(index) != Some("name") {
                continue;
            }
            let Some(binding) = syntax.child(index) else {
                continue;
            };
            let (name_node, alias_node) = if binding.kind() == "aliased_import" {
                (
                    binding.child_by_field_name("name").ok_or_else(|| {
                        ExtractorError::InvalidInput("aliased from-import has no name".to_owned())
                    })?,
                    binding.child_by_field_name("alias"),
                )
            } else {
                (binding, None)
            };
            let imported_name = syntax_text(name_node, self.source)?;
            let local_name = alias_node
                .map(|alias| syntax_text(alias, self.source))
                .transpose()?
                .unwrap_or_else(|| imported_name.clone());
            self.record_import(
                &specifier,
                ImportKind::PythonFrom,
                Some(imported_name),
                Some(local_name),
                binding,
            )?;
        }
        let mut cursor = syntax.walk();
        let wildcard = syntax
            .named_children(&mut cursor)
            .find(|child| child.kind() == "wildcard_import");
        if let Some(node) = wildcard {
            self.record_import(&specifier, ImportKind::PythonStar, None, None, node)?;
        }
        Ok(())
    }

    fn record_import(
        &mut self,
        specifier: &str,
        kind: ImportKind,
        imported_name: Option<String>,
        local_name: Option<String>,
        syntax: SyntaxNode<'_>,
    ) -> Result<(), ExtractorError> {
        let key = (
            kind,
            specifier.to_owned(),
            imported_name.clone(),
            local_name.clone(),
        );
        let count = self.import_counts.entry(key).or_default();
        let occurrence = count.to_le_bytes();
        *count = count.saturating_add(1);
        let module_id = module_id(self.source);
        let id = ImportRecordId::derive(&[
            &module_id.0.0,
            format!("{kind:?}").as_bytes(),
            specifier.as_bytes(),
            imported_name.as_deref().unwrap_or_default().as_bytes(),
            local_name.as_deref().unwrap_or_default().as_bytes(),
            &occurrence,
        ]);
        let start = u64::try_from(syntax.start_byte())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = u64::try_from(syntax.end_byte())
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let span = SourceSpan::new(start, end)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        self.imports.push(ImportRecord {
            id,
            module: module_id,
            specifier: specifier.to_owned(),
            kind,
            imported_name,
            local_name,
            type_only: false,
            source: SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span,
            },
            provenance: self.provenance,
        });
        Ok(())
    }
}

fn module_id(source: &SourceFile) -> NodeId {
    NodeId::derive(&[
        &source.file.file_id.0.0,
        PRODUCER_NAMESPACE.as_bytes(),
        b"source-module-v1",
    ])
}

fn syntax_text(node: SyntaxNode<'_>, source: &SourceFile) -> Result<String, ExtractorError> {
    node.utf8_text(source.content.as_bytes())
        .map(str::to_owned)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))
}

#[cfg(test)]
mod tests;
