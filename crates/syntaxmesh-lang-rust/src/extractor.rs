//! Rust syntax extraction implementation.

use std::collections::BTreeMap;

use proc_macro2::Span;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syntaxmesh_core::{
    FileVersion, ImportKind, ImportRecord, ImportRecordId, Node, NodeId, NodeKind, Provenance,
    ProvenanceId, RelationKind, SourceLocation, SourceSpan,
};
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, Reference, SourceFile,
};

const RUST_PRODUCER_NAMESPACE: &str = "syntaxmesh.lang.rust";

#[cfg(test)]
#[path = "extractor/bindings/tests.rs"]
mod binding_tests;
mod bindings;
mod call_paths;
mod declarations;
mod definition_ranges;
mod implementations;
mod local_items;
mod test_roles;
mod trait_methods;

#[cfg(test)]
mod path_tests;

#[cfg(test)]
mod trait_tests;

#[cfg(test)]
mod implementation_tests;

#[cfg(test)]
#[path = "extractor/module_ownership/tests.rs"]
mod module_ownership_tests;

#[derive(Debug, Clone, Copy, Default)]
pub struct RustExtractor;

struct RustSource<'source> {
    file: &'source FileVersion,
    content: &'source str,
    parser_prefix: usize,
}

impl RustExtractor {
    fn node(
        file: &FileVersion,
        provenance: ProvenanceId,
        kind: NodeKind,
        name: &str,
        identity_name: &str,
        source: Option<SourceSpan>,
    ) -> Node {
        let node_id = declaration_node_id(file, &kind, identity_name);
        Node {
            id: node_id,
            kind,
            name: name.to_owned(),
            owner_file: Some(file.file_id),
            source: source.map(|span| SourceLocation {
                file_id: file.file_id,
                content_hash: file.content_hash,
                span,
            }),
            provenance,
            extension_payload: None,
        }
    }

    fn collect_items(
        items: &[syn::Item],
        path: &[String],
        source: &RustSource<'_>,
        provenance: ProvenanceId,
        nodes: &mut declarations::Declarations,
    ) -> Result<(), ExtractorError> {
        for item in items {
            if let syn::Item::Fn(item) = item {
                Self::push_item_node(
                    NodeKind::Function,
                    &item.sig.ident,
                    path,
                    source,
                    provenance,
                    nodes,
                )?;
                let scope = local_items::scope(path, &item.sig.ident);
                for nested in local_items::items(&item.block) {
                    Self::collect_items(
                        std::slice::from_ref(nested),
                        &scope,
                        source,
                        provenance,
                        nodes,
                    )?;
                }
                continue;
            }
            if let syn::Item::Struct(item) = item {
                Self::push_item_node(
                    NodeKind::Struct,
                    &item.ident,
                    path,
                    source,
                    provenance,
                    nodes,
                )?;
                continue;
            }
            if let syn::Item::Enum(item) = item {
                Self::push_item_node(NodeKind::Enum, &item.ident, path, source, provenance, nodes)?;
                continue;
            }
            if let syn::Item::Trait(item) = item {
                Self::push_item_node(
                    NodeKind::Trait,
                    &item.ident,
                    path,
                    source,
                    provenance,
                    nodes,
                )?;
                trait_methods::collect_nodes(item, path, source, provenance, nodes)?;
                continue;
            }
            if let syn::Item::Mod(item) = item {
                let name = item.ident.to_string();
                Self::push_item_node(
                    NodeKind::Module,
                    &item.ident,
                    path,
                    source,
                    provenance,
                    nodes,
                )?;
                if let Some((_, nested)) = &item.content {
                    let mut nested_path = path.to_vec();
                    nested_path.push(name);
                    Self::collect_items(nested, &nested_path, source, provenance, nodes)?;
                }
                continue;
            }
            if let syn::Item::Impl(item) = item {
                let mut impl_path = path.to_vec();
                impl_path.push(item.self_ty.to_token_stream().to_string());
                for impl_item in &item.items {
                    if let syn::ImplItem::Fn(function) = impl_item {
                        Self::push_item_node(
                            NodeKind::Function,
                            &function.sig.ident,
                            &impl_path,
                            source,
                            provenance,
                            nodes,
                        )?;
                        let scope = local_items::scope(&impl_path, &function.sig.ident);
                        for nested in local_items::items(&function.block) {
                            Self::collect_items(
                                std::slice::from_ref(nested),
                                &scope,
                                source,
                                provenance,
                                nodes,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn push_item_node(
        kind: NodeKind,
        name: &syn::Ident,
        path: &[String],
        source: &RustSource<'_>,
        provenance: ProvenanceId,
        nodes: &mut declarations::Declarations,
    ) -> Result<(), ExtractorError> {
        let token_name = name.to_string();
        let qualified = if path.is_empty() {
            token_name
        } else {
            format!("{}::{name}", path.join("::"))
        };
        let span = source_span(source, name.span())?;
        let identity_name = nodes.identity(&kind, &qualified)?;
        nodes.nodes.push(Self::node(
            source.file,
            provenance,
            kind,
            &qualified,
            &identity_name,
            Some(span),
        ));
        Ok(())
    }

    fn collect_references(
        items: &[syn::Item],
        nodes: &BTreeMap<(u64, u64), NodeId>,
        path: &[String],
        source: &RustSource<'_>,
        provenance: ProvenanceId,
        occurrences: &mut BTreeMap<(NodeId, String), usize>,
        references: &mut Vec<Reference>,
    ) -> Result<(), ExtractorError> {
        for item in items {
            if let syn::Item::Trait(item) = item {
                trait_methods::collect_references(
                    item,
                    path,
                    nodes,
                    source,
                    provenance,
                    occurrences,
                    references,
                )?;
                continue;
            }
            if let syn::Item::Fn(function) = item
                && let Some(source_node) = declaration_at(nodes, &function.sig.ident, source)?
            {
                let mut visitor = CallVisitor::default();
                visitor.parameters(&function.sig);
                visitor.visit_block(&function.block);
                for call in visitor.calls {
                    references.push(make_reference(
                        source_node,
                        call.target,
                        call.span,
                        call.resolution,
                        source,
                        provenance,
                        occurrences,
                    )?);
                }
                let scope = local_items::scope(path, &function.sig.ident);
                for nested in local_items::items(&function.block) {
                    Self::collect_references(
                        std::slice::from_ref(nested),
                        nodes,
                        &scope,
                        source,
                        provenance,
                        occurrences,
                        references,
                    )?;
                }
            }
            if let syn::Item::Mod(module) = item {
                if let Some((_, nested)) = &module.content {
                    let mut nested_path = path.to_vec();
                    nested_path.push(module.ident.to_string());
                    Self::collect_references(
                        nested,
                        nodes,
                        &nested_path,
                        source,
                        provenance,
                        occurrences,
                        references,
                    )?;
                }
                continue;
            }
            if let syn::Item::Impl(item) = item {
                let mut impl_path = path.to_vec();
                impl_path.push(item.self_ty.to_token_stream().to_string());
                for impl_item in &item.items {
                    if let syn::ImplItem::Fn(function) = impl_item
                        && let Some(source_node) =
                            declaration_at(nodes, &function.sig.ident, source)?
                    {
                        let mut visitor = CallVisitor {
                            impl_type: Some(impl_path.join("::")),
                            ..CallVisitor::default()
                        };
                        visitor.parameters(&function.sig);
                        visitor.visit_block(&function.block);
                        for call in visitor.calls {
                            references.push(make_reference(
                                source_node,
                                call.target,
                                call.span,
                                call.resolution,
                                source,
                                provenance,
                                occurrences,
                            )?);
                        }
                        let scope = local_items::scope(&impl_path, &function.sig.ident);
                        for nested in local_items::items(&function.block) {
                            Self::collect_references(
                                std::slice::from_ref(nested),
                                nodes,
                                &scope,
                                source,
                                provenance,
                                occurrences,
                                references,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn declaration_at(
    nodes: &BTreeMap<(u64, u64), NodeId>,
    name: &syn::Ident,
    source: &RustSource<'_>,
) -> Result<Option<NodeId>, ExtractorError> {
    let span = source_span(source, name.span())?;
    Ok(nodes.get(&(span.start_byte, span.end_byte)).copied())
}

fn declaration_node_id(file: &FileVersion, kind: &NodeKind, identity_name: &str) -> NodeId {
    let kind_name = format!("{kind:?}");
    NodeId::derive(&[
        &file.file_id.0.0,
        kind_name.as_bytes(),
        identity_name.as_bytes(),
    ])
}

fn file_module_node(source: &SourceFile, provenance: ProvenanceId) -> Result<Node, ExtractorError> {
    let end = u64::try_from(source.content.len())
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let module_id = NodeId::derive(&[
        &source.file.file_id.0.0,
        RUST_PRODUCER_NAMESPACE.as_bytes(),
        b"source-module-v1",
    ]);
    Ok(Node {
        id: module_id,
        kind: NodeKind::Module,
        name: source.file.normalized_path.clone(),
        owner_file: Some(source.file.file_id),
        source: Some(SourceLocation {
            file_id: source.file.file_id,
            content_hash: source.file.content_hash,
            span: SourceSpan::new(0, end)
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?,
        }),
        provenance,
        extension_payload: None,
    })
}

#[derive(Clone, Default)]
struct UsePath {
    absolute: bool,
    segments: Vec<String>,
}

impl UsePath {
    fn text(&self) -> String {
        let path = self.segments.join("::");
        if self.absolute {
            format!("::{path}")
        } else {
            path
        }
    }
}

struct RustImportVisitor<'source> {
    source: &'source RustSource<'source>,
    provenance: ProvenanceId,
    modules: Vec<NodeId>,
    module_nodes: &'source BTreeMap<(u64, u64), NodeId>,
    imports: Vec<ImportRecord>,
    occurrences: BTreeMap<(ImportKind, String, Option<String>, Option<String>), u64>,
    failure: Option<ExtractorError>,
}

impl<'source> RustImportVisitor<'source> {
    fn new(
        source: &'source RustSource<'source>,
        provenance: ProvenanceId,
        module: NodeId,
        module_nodes: &'source BTreeMap<(u64, u64), NodeId>,
    ) -> Self {
        Self {
            source,
            provenance,
            modules: vec![module],
            module_nodes,
            imports: Vec::new(),
            occurrences: BTreeMap::new(),
            failure: None,
        }
    }

    fn collect_use_tree(
        &mut self,
        tree: &syn::UseTree,
        prefix: &UsePath,
    ) -> Result<(), ExtractorError> {
        match tree {
            syn::UseTree::Path(path) => {
                let mut full_path = prefix.clone();
                full_path.segments.push(path.ident.to_string());
                self.collect_use_tree(&path.tree, &full_path)
            }
            syn::UseTree::Name(name) => {
                let imported_name = name.ident.to_string();
                let (specifier, imported_name, local_name) =
                    if imported_name == "self" && !prefix.segments.is_empty() {
                        let bound = prefix.segments.last().cloned().unwrap_or_default();
                        (prefix.text(), bound.clone(), bound)
                    } else {
                        let mut full_path = prefix.clone();
                        full_path.segments.push(imported_name.clone());
                        (full_path.text(), imported_name.clone(), imported_name)
                    };
                self.push_import(
                    ImportKind::RustUse,
                    specifier,
                    Some(imported_name),
                    Some(local_name),
                    name.span(),
                )
            }
            syn::UseTree::Rename(rename) => {
                let imported = rename.ident.to_string();
                let (specifier, imported_name) =
                    if imported == "self" && !prefix.segments.is_empty() {
                        (prefix.text(), prefix.segments.last().cloned())
                    } else {
                        let mut full_path = prefix.clone();
                        full_path.segments.push(imported.clone());
                        (full_path.text(), Some(imported))
                    };
                self.push_import(
                    ImportKind::RustUse,
                    specifier,
                    imported_name,
                    Some(rename.rename.to_string()),
                    rename.span(),
                )
            }
            syn::UseTree::Glob(glob) => {
                self.push_import(ImportKind::RustGlob, prefix.text(), None, None, glob.span())
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.collect_use_tree(item, prefix)?;
                }
                Ok(())
            }
        }
    }

    fn push_import(
        &mut self,
        kind: ImportKind,
        specifier: String,
        imported_name: Option<String>,
        local_name: Option<String>,
        span: Span,
    ) -> Result<(), ExtractorError> {
        if specifier.is_empty() {
            return Err(ExtractorError::InvalidInput(
                "Rust use tree produced an empty import path".to_owned(),
            ));
        }
        let key = (
            kind,
            specifier.clone(),
            imported_name.clone(),
            local_name.clone(),
        );
        let ordinal = self.occurrences.entry(key).or_default();
        let ordinal_bytes = ordinal.to_le_bytes();
        *ordinal = ordinal.saturating_add(1);
        let module = self.modules.last().copied().ok_or_else(|| {
            ExtractorError::InvalidInput("Rust module scope is missing".to_owned())
        })?;
        let kind_name = format!("{kind:?}");
        let id = ImportRecordId::derive(&[
            &module.0.0,
            kind_name.as_bytes(),
            specifier.as_bytes(),
            imported_name.as_deref().unwrap_or_default().as_bytes(),
            local_name.as_deref().unwrap_or_default().as_bytes(),
            &ordinal_bytes,
        ]);
        let source_span = source_span(self.source, span)?;
        self.imports.push(ImportRecord {
            id,
            module,
            specifier,
            kind,
            imported_name,
            local_name,
            type_only: false,
            source: SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span: source_span,
            },
            provenance: self.provenance,
        });
        Ok(())
    }
}

impl<'ast> syn::visit::Visit<'ast> for RustImportVisitor<'_> {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if self.failure.is_none() {
            self.failure = self
                .collect_use_tree(
                    &item.tree,
                    &UsePath {
                        absolute: item.leading_colon.is_some(),
                        segments: Vec::new(),
                    },
                )
                .err();
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.content.is_some() {
            let module = declaration_at(self.module_nodes, &item.ident, self.source);
            let module = match module {
                Ok(Some(module)) => module,
                Ok(None) => {
                    self.failure = Some(ExtractorError::InvalidInput(
                        "inline module has no source-backed declaration".to_owned(),
                    ));
                    return;
                }
                Err(error) => {
                    self.failure = Some(error);
                    return;
                }
            };
            self.modules.push(module);
            syn::visit::visit_item_mod(self, item);
            self.modules.pop();
        } else {
            syn::visit::visit_item_mod(self, item);
        }
    }
}

#[derive(Default)]
struct CallVisitor {
    calls: Vec<CallOccurrence>,
    impl_type: Option<String>,
    bindings: bindings::Bindings,
}

struct CallOccurrence {
    target: String,
    span: Span,
    resolution: syntaxmesh_language_sdk::ReferenceResolution,
}

fn make_reference(
    source_node: NodeId,
    target: String,
    span: Span,
    resolution: syntaxmesh_language_sdk::ReferenceResolution,
    source: &RustSource<'_>,
    provenance: ProvenanceId,
    occurrences: &mut BTreeMap<(NodeId, String), usize>,
) -> Result<Reference, ExtractorError> {
    let key = (source_node, target.clone());
    let ordinal = occurrences.entry(key).or_default();
    let ordinal_bytes = u64::try_from(*ordinal)
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?
        .to_le_bytes();
    *ordinal = ordinal.saturating_add(1);
    let source_span = source_span(source, span)?;
    let source_location = SourceLocation {
        file_id: source.file.file_id,
        content_hash: source.file.content_hash,
        span: source_span,
    };
    Ok(Reference {
        resolution,
        id: NodeId::derive(&[&source_node.0.0, target.as_bytes(), &ordinal_bytes]),
        source: source_node,
        target,
        relation: RelationKind::Calls,
        source_location,
        provenance,
    })
}

fn source_span(source: &RustSource<'_>, span: Span) -> Result<SourceSpan, ExtractorError> {
    let range = span.byte_range();
    let start = range
        .start
        .checked_add(source.parser_prefix)
        .ok_or_else(|| ExtractorError::InvalidInput("source offset overflow".to_owned()))?;
    let end = range
        .end
        .checked_add(source.parser_prefix)
        .ok_or_else(|| ExtractorError::InvalidInput("source offset overflow".to_owned()))?;
    if source.content.get(start..end).is_none() {
        return Err(ExtractorError::InvalidInput(
            "parser span is outside UTF-8 source".to_owned(),
        ));
    }
    let start =
        u64::try_from(start).map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let end =
        u64::try_from(end).map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    SourceSpan::new(start, end).map_err(|error| ExtractorError::InvalidInput(error.to_string()))
}

impl<'ast> syn::visit::Visit<'ast> for CallVisitor {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.bindings.push();
        self.bindings.block_items(block);
        syn::visit::visit_block(self, block);
        self.bindings.pop();
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(initializer) = &local.init {
            self.visit_expr(&initializer.expr);
            if let Some((_, diverge)) = &initializer.diverge {
                self.visit_expr(diverge);
            }
        }
        self.bindings.pattern(&local.pat);
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        self.bindings.push();
        for pattern in &closure.inputs {
            self.bindings.pattern(pattern);
        }
        self.visit_expr(&closure.body);
        self.bindings.pop();
    }

    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        self.visit_expr(&expression.expr);
        self.bindings.push();
        self.bindings.pattern(&expression.pat);
        self.visit_block(&expression.body);
        self.bindings.pop();
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        self.bindings.push();
        self.bindings.pattern(&arm.pat);
        syn::visit::visit_arm(self, arm);
        self.bindings.pop();
    }

    fn visit_expr_let(&mut self, expression: &'ast syn::ExprLet) {
        self.visit_expr(&expression.expr);
        self.bindings.pattern(&expression.pat);
    }

    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        self.bindings.push();
        self.visit_expr(&expression.cond);
        self.visit_block(&expression.then_branch);
        self.bindings.pop();
        if let Some((_, branch)) = &expression.else_branch {
            self.visit_expr(branch);
        }
    }

    fn visit_expr_while(&mut self, expression: &'ast syn::ExprWhile) {
        self.bindings.push();
        self.visit_expr(&expression.cond);
        self.visit_block(&expression.body);
        self.bindings.pop();
    }

    fn visit_item(&mut self, _item: &'ast syn::Item) {
        // Local declarations have their own owners and reference collection.
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            self.calls.push(CallOccurrence {
                resolution: if path.qself.is_none()
                    && path.path.leading_colon.is_none()
                    && path.path.segments.len() == 1
                    && path
                        .path
                        .segments
                        .first()
                        .is_some_and(|segment| self.bindings.contains(&segment.ident.to_string()))
                {
                    syntaxmesh_language_sdk::ReferenceResolution::Unresolved
                } else {
                    syntaxmesh_language_sdk::ReferenceResolution::Name
                },
                target: call_paths::call_target(path),
                span: path.span(),
            });
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        self.calls.push(CallOccurrence {
            resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
            target: call_paths::method_target(call, self.impl_type.as_deref()),
            span: call.span(),
        });
        syn::visit::visit_expr_method_call(self, call);
    }
}

impl LanguageExtractor for RustExtractor {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new(RUST_PRODUCER_NAMESPACE, "0.18.0")
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        if !source.file.normalized_path.ends_with(".rs") {
            return Err(ExtractorError::UnsupportedFile(
                source.file.normalized_path.clone(),
            ));
        }
        let syntax = syn::parse_file(&source.content)
            .map_err(|error| ExtractorError::SyntaxError(error.to_string()))?;
        let identity = self.producer_identity();
        let provenance = Provenance {
            id: ProvenanceId::derive(&[
                identity.namespace.as_bytes(),
                &source.file.file_id.0.0,
                source.file.content_hash.as_slice(),
            ]),
            producer_namespace: identity.namespace,
            producer_version: identity.version,
            evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
            source: Some(SourceLocation {
                file_id: source.file.file_id,
                content_hash: source.file.content_hash,
                span: SourceSpan::new(
                    0,
                    u64::try_from(source.content.len())
                        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?,
                )
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?,
            }),
        };
        let file_module = file_module_node(source, provenance.id)?;
        let mut declarations = declarations::Declarations::new(file_module.clone());
        let input = RustSource {
            file: &source.file,
            content: &source.content,
            parser_prefix: (if source.content.starts_with('\u{feff}') {
                '\u{feff}'.len_utf8()
            } else {
                0_usize
            })
            .checked_add(syntax.shebang.as_ref().map_or(0, String::len))
            .ok_or_else(|| ExtractorError::InvalidInput("parser prefix overflow".to_owned()))?,
        };
        Self::collect_items(&syntax.items, &[], &input, provenance.id, &mut declarations)?;
        let mut nodes = declarations.into_nodes();
        let function_nodes = nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Function)
            .filter_map(|node| {
                node.source
                    .as_ref()
                    .map(|location| ((location.span.start_byte, location.span.end_byte), node.id))
            })
            .collect::<BTreeMap<_, _>>();
        let mut references = Vec::new();
        Self::collect_references(
            &syntax.items,
            &function_nodes,
            &[],
            &input,
            provenance.id,
            &mut BTreeMap::new(),
            &mut references,
        )?;
        let declaration_nodes = nodes
            .iter()
            .filter_map(|node| {
                node.source
                    .as_ref()
                    .map(|location| ((location.span.start_byte, location.span.end_byte), node.id))
            })
            .collect::<BTreeMap<_, _>>();
        let (implementation_nodes, mut implementation_edges, implementation_references) =
            implementations::extract(
                &syntax,
                &input,
                provenance.id,
                file_module.id,
                &declaration_nodes,
            )?;
        nodes.extend(implementation_nodes);
        implementation_edges.extend(local_items::extract(
            &syntax,
            &input,
            provenance.id,
            &function_nodes,
        )?);
        references.extend(implementation_references);
        let module_nodes = nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Module)
            .filter_map(|node| {
                node.source
                    .as_ref()
                    .map(|location| ((location.span.start_byte, location.span.end_byte), node.id))
            })
            .collect::<BTreeMap<_, _>>();
        let mut import_visitor =
            RustImportVisitor::new(&input, provenance.id, file_module.id, &module_nodes);
        import_visitor.visit_file(&syntax);
        if let Some(error) = import_visitor.failure.take() {
            return Err(error);
        }
        definition_ranges::apply(&syntax, &input, &function_nodes, &mut nodes)?;
        test_roles::apply(&syntax, &input, &function_nodes, &mut nodes)?;
        Ok(Extraction {
            provenance,
            nodes,
            edges: implementation_edges,
            references,
            imports: import_visitor.imports,
            exports: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests;
