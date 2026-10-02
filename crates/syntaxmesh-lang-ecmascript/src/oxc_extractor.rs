use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression, FunctionType,
    ImportDeclarationSpecifier, ImportOrExportKind, MethodDefinition, ModuleExportName,
    PropertyKey,
};
use oxc_ast::ast_kind::AstKind;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use syntaxmesh_core::{
    EvidenceClass, ExportKind, ExportRecord, ExportRecordId, ImportKind, ImportRecord,
    ImportRecordId, Node, NodeId, NodeKind, Provenance, ProvenanceId, RelationKind, SourceLocation,
    SourceSpan,
};
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, Reference, SourceFile,
};

const SEMANTIC_VERSION: &str = "11";

#[derive(Debug, Clone, Copy, Default)]
pub struct TypeScriptExtractor;

#[derive(Debug, Clone, Copy, Default)]
pub struct JavaScriptExtractor;

impl LanguageExtractor for TypeScriptExtractor {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        identity("syntaxmesh.lang.typescript")
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        extract(
            source,
            "typescript",
            &self.producer_identity(),
            &["ts", "tsx"],
        )
    }
}

impl LanguageExtractor for JavaScriptExtractor {
    fn language(&self) -> &'static str {
        "javascript"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        identity("syntaxmesh.lang.javascript")
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        extract(
            source,
            "javascript",
            &self.producer_identity(),
            &["js", "jsx", "mjs", "cjs"],
        )
    }
}

fn identity(namespace: &str) -> ExtractorIdentity {
    ExtractorIdentity::new(
        namespace,
        format!(
            "{};oxc-parser=0.152.0;extractor={SEMANTIC_VERSION}",
            env!("CARGO_PKG_VERSION")
        ),
    )
}

fn extract(
    source: &SourceFile,
    namespace: &str,
    identity: &ExtractorIdentity,
    extensions: &[&str],
) -> Result<Extraction, ExtractorError> {
    let path = Path::new(&source.file.normalized_path);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !extensions.iter().any(|candidate| *candidate == extension) {
        return Err(ExtractorError::UnsupportedFile(
            source.file.normalized_path.clone(),
        ));
    }
    let source_type = source_type(&extension)
        .ok_or_else(|| ExtractorError::UnsupportedFile(source.file.normalized_path.clone()))?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source.content, source_type).parse();
    if parsed.fatal_error || !parsed.diagnostics.is_empty() {
        let message = parsed
            .diagnostics
            .first()
            .map(ToString::to_string)
            .unwrap_or_else(|| "ECMAScript parser failed".to_owned());
        return Err(ExtractorError::SyntaxError(message));
    }

    let content_len = u64::try_from(source.content.len())
        .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
    let provenance = Provenance {
        id: ProvenanceId::derive(&[
            identity.namespace.as_bytes(),
            &source.file.file_id.0.0,
            source.file.content_hash.as_slice(),
        ]),
        producer_namespace: identity.namespace.clone(),
        producer_version: identity.version.clone(),
        evidence_class: EvidenceClass::SourceFact,
        source: Some(SourceLocation {
            file_id: source.file.file_id,
            content_hash: source.file.content_hash,
            span: SourceSpan::new(0, content_len)
                .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?,
        }),
    };
    let module_id = NodeId::derive(&[
        &source.file.file_id.0.0,
        identity.namespace.as_bytes(),
        b"source-module-v1",
    ]);
    let module = Node {
        id: module_id,
        kind: NodeKind::Module,
        name: source.file.normalized_path.clone(),
        owner_file: Some(source.file.file_id),
        source: provenance.source.clone(),
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut builder = Builder {
        source,
        provenance: provenance.id,
        namespace,
        module_id,
        scope: Vec::new(),
        current_class: None,
        this_class: None,
        owners: Vec::new(),
        ast_stack: Vec::new(),
        frames: Vec::new(),
        declaration_counts: BTreeMap::new(),
        reference_counts: BTreeMap::new(),
        source_fact_counts: BTreeMap::new(),
        namespace_imports: BTreeMap::new(),
        shadowed_namespace_imports: BTreeSet::new(),
        pending_namespace_members: Vec::new(),
        failure: None,
        nodes: vec![module],
        references: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
    };
    builder.visit_program(&parsed.program);
    if let Some(error) = builder.failure {
        return Err(error);
    }
    builder.finish_namespace_member_imports()?;
    Ok(Extraction {
        provenance,
        nodes: builder.nodes,
        edges: Vec::new(),
        references: builder.references,
        imports: builder.imports,
        exports: builder.exports,
    })
}

fn source_type(extension: &str) -> Option<SourceType> {
    match extension {
        "ts" => Some(SourceType::ts()),
        "tsx" => Some(SourceType::tsx()),
        "js" | "mjs" | "cjs" => Some(SourceType::mjs()),
        "jsx" => Some(SourceType::jsx()),
        _ => None,
    }
}

fn module_export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(identifier) => identifier.name.as_str().to_owned(),
        ModuleExportName::IdentifierReference(identifier) => identifier.name.as_str().to_owned(),
        ModuleExportName::StringLiteral(literal) => literal.value.as_str().to_owned(),
    }
}

fn binding_names(pattern: &BindingPattern<'_>, names: &mut Vec<(String, Span)>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => {
            names.push((identifier.name.as_str().to_owned(), identifier.span));
        }
        BindingPattern::ObjectPattern(pattern) => {
            for property in &pattern.properties {
                binding_names(&property.value, names);
            }
            if let Some(rest) = &pattern.rest {
                binding_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(pattern) => {
            for element in pattern.elements.iter().flatten() {
                binding_names(element, names);
            }
            if let Some(rest) = &pattern.rest {
                binding_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(pattern) => binding_names(&pattern.left, names),
    }
}

fn declared_export_names(declaration: &Declaration<'_>) -> Vec<(String, Span, bool)> {
    let mut names = Vec::new();
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                binding_names(&declarator.id, &mut names);
            }
            names
                .into_iter()
                .map(|(name, span)| (name, span, false))
                .collect()
        }
        Declaration::FunctionDeclaration(declaration) => declaration
            .id
            .as_ref()
            .map(|id| vec![(id.name.as_str().to_owned(), id.span, false)])
            .unwrap_or_default(),
        Declaration::ClassDeclaration(declaration) => declaration
            .id
            .as_ref()
            .map(|id| vec![(id.name.as_str().to_owned(), id.span, false)])
            .unwrap_or_default(),
        Declaration::TSTypeAliasDeclaration(declaration) => vec![(
            declaration.id.name.as_str().to_owned(),
            declaration.id.span,
            true,
        )],
        Declaration::TSInterfaceDeclaration(declaration) => vec![(
            declaration.id.name.as_str().to_owned(),
            declaration.id.span,
            true,
        )],
        Declaration::TSEnumDeclaration(declaration) => vec![(
            declaration.id.name.as_str().to_owned(),
            declaration.id.span,
            false,
        )],
        Declaration::TSNamespaceDeclaration(declaration) => vec![(
            declaration.id.name.as_str().to_owned(),
            declaration.id.span,
            false,
        )],
        Declaration::TSImportEqualsDeclaration(_)
        | Declaration::TSExternalModuleDeclaration(_)
        | Declaration::TSGlobalDeclaration(_) => Vec::new(),
    }
}

#[derive(Clone)]
enum AstTag {
    Other,
    VariableDeclarator(Option<(String, Span)>),
    MethodDefinition(Option<String>),
}

type SourceFactKey = (String, String, Option<String>, Option<String>, bool);
type PendingNamespaceMember = (String, String, Span);

#[derive(Default)]
struct Frame {
    scope: bool,
    class_context: Option<Option<String>>,
    this_context: Option<Option<String>>,
    owner: bool,
}

struct Builder<'source> {
    source: &'source SourceFile,
    provenance: ProvenanceId,
    namespace: &'source str,
    module_id: NodeId,
    scope: Vec<String>,
    current_class: Option<String>,
    this_class: Option<String>,
    owners: Vec<NodeId>,
    ast_stack: Vec<AstTag>,
    frames: Vec<Frame>,
    declaration_counts: BTreeMap<(String, String), usize>,
    reference_counts: BTreeMap<(NodeId, String), usize>,
    source_fact_counts: BTreeMap<SourceFactKey, usize>,
    namespace_imports: BTreeMap<String, String>,
    shadowed_namespace_imports: BTreeSet<String>,
    pending_namespace_members: Vec<PendingNamespaceMember>,
    failure: Option<ExtractorError>,
    nodes: Vec<Node>,
    references: Vec<Reference>,
    imports: Vec<ImportRecord>,
    exports: Vec<ExportRecord>,
}

impl Builder<'_> {
    fn shadow_namespace_import(&mut self, name: &str) {
        if self.namespace_imports.contains_key(name) {
            self.shadowed_namespace_imports.insert(name.to_owned());
        }
    }

    fn finish_namespace_member_imports(&mut self) -> Result<(), ExtractorError> {
        let pending = std::mem::take(&mut self.pending_namespace_members);
        for (alias, member, span) in pending {
            let Some(specifier) = self.namespace_imports.get(&alias).cloned() else {
                continue;
            };
            if self.shadowed_namespace_imports.contains(&alias) {
                continue;
            }
            self.record_import(
                &specifier,
                ImportKind::NamespaceMember,
                Some(member.clone()),
                Some(format!("{alias}.{member}")),
                false,
                span,
            )?;
        }
        Ok(())
    }

    fn declare(
        &mut self,
        name: &str,
        kind: NodeKind,
        source_span: Span,
    ) -> Result<NodeId, ExtractorError> {
        self.shadow_namespace_import(name);
        let qualified = if self.scope.is_empty() {
            name.to_owned()
        } else {
            format!("{}::{name}", self.scope.join("::"))
        };
        let kind_name = format!("{kind:?}");
        let count = self
            .declaration_counts
            .entry((kind_name.clone(), qualified.clone()))
            .or_default();
        let identity_name = if *count == 0 {
            qualified.clone()
        } else {
            format!("{qualified}#{}", *count)
        };
        *count = count.saturating_add(1);
        let file_bytes = self.source.file.file_id.0.0;
        let id = NodeId::derive(&[
            &file_bytes,
            self.namespace.as_bytes(),
            kind_name.as_bytes(),
            identity_name.as_bytes(),
        ]);
        self.nodes.push(Node {
            id,
            kind,
            name: qualified,
            owner_file: Some(self.source.file.file_id),
            source: Some(SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span: convert_span(source_span)?,
            }),
            provenance: self.provenance,
            extension_payload: None,
        });
        Ok(id)
    }

    fn record_call(
        &mut self,
        source_node: NodeId,
        target: String,
        target_span: Span,
    ) -> Result<(), ExtractorError> {
        let count = self
            .reference_counts
            .entry((source_node, target.clone()))
            .or_default();
        let occurrence = count.to_le_bytes();
        *count = count.saturating_add(1);
        let source_bytes = source_node.0.0;
        let id = NodeId::derive(&[
            &source_bytes,
            b"call-reference-v1",
            target.as_bytes(),
            &occurrence,
        ]);
        self.references.push(Reference {
            resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
            id,
            source: source_node,
            target,
            relation: RelationKind::Calls,
            source_location: SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span: convert_span(target_span)?,
            },
            provenance: self.provenance,
        });
        Ok(())
    }

    fn record_import(
        &mut self,
        target: &str,
        kind: ImportKind,
        imported_name: Option<String>,
        local_name: Option<String>,
        type_only: bool,
        source_span: Span,
    ) -> Result<(), ExtractorError> {
        let key = (
            format!("import:{kind:?}"),
            target.to_owned(),
            imported_name.clone(),
            local_name.clone(),
            type_only,
        );
        let count = self.source_fact_counts.entry(key).or_default();
        let occurrence = count.to_le_bytes();
        *count = count.saturating_add(1);
        let id = ImportRecordId::derive(&[
            &self.module_id.0.0,
            format!("{kind:?}").as_bytes(),
            target.as_bytes(),
            imported_name.as_deref().unwrap_or_default().as_bytes(),
            local_name.as_deref().unwrap_or_default().as_bytes(),
            &[u8::from(type_only)],
            &occurrence,
        ]);
        self.imports.push(ImportRecord {
            id,
            module: self.module_id,
            specifier: target.to_owned(),
            kind,
            imported_name,
            local_name,
            type_only,
            source: SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span: convert_span(source_span)?,
            },
            provenance: self.provenance,
        });
        Ok(())
    }

    fn record_export(
        &mut self,
        source_specifier: Option<&str>,
        kind: ExportKind,
        exported_name: Option<String>,
        local_name: Option<String>,
        type_only: bool,
        source_span: Span,
    ) -> Result<(), ExtractorError> {
        let kind_name = format!("{kind:?}");
        let specifier = source_specifier.unwrap_or_default();
        let key = (
            format!("export:{kind_name}"),
            specifier.to_owned(),
            exported_name.clone(),
            local_name.clone(),
            type_only,
        );
        let count = self.source_fact_counts.entry(key).or_default();
        let occurrence = count.to_le_bytes();
        *count = count.saturating_add(1);
        let id = ExportRecordId::derive(&[
            &self.module_id.0.0,
            kind_name.as_bytes(),
            specifier.as_bytes(),
            exported_name.as_deref().unwrap_or_default().as_bytes(),
            local_name.as_deref().unwrap_or_default().as_bytes(),
            &[u8::from(type_only)],
            &occurrence,
        ]);
        self.exports.push(ExportRecord {
            id,
            module: self.module_id,
            source_specifier: source_specifier.map(str::to_owned),
            kind,
            exported_name,
            local_name,
            type_only,
            source: SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span: convert_span(source_span)?,
            },
            provenance: self.provenance,
        });
        Ok(())
    }

    fn source_slice(&self, span: Span) -> Result<String, ExtractorError> {
        let start = usize::try_from(span.start)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        let end = usize::try_from(span.end)
            .map_err(|error| ExtractorError::InvalidInput(error.to_string()))?;
        self.source
            .content
            .get(start..end)
            .map(str::to_owned)
            .ok_or_else(|| ExtractorError::InvalidInput("AST span is outside source".to_owned()))
    }

    fn call_target(&self, callee: &Expression<'_>) -> Result<(String, Span), ExtractorError> {
        if let Expression::StaticMemberExpression(member) = callee {
            let target = if matches!(member.object, Expression::ThisExpression(_))
                && let Some(class) = &self.this_class
            {
                format!("{class}::{}", member.property.name.as_str())
            } else {
                self.source_slice(member.span)?
            };
            Ok((target, member.property.span))
        } else {
            Ok((self.source_slice(callee.span())?, callee.span()))
        }
    }
}

impl<'visitor> Visit<'visitor> for Builder<'_> {
    fn enter_node(&mut self, kind: AstKind<'visitor>) {
        if self.failure.is_some() {
            self.ast_stack.push(AstTag::Other);
            self.frames.push(Frame::default());
            return;
        }
        let mut tag = AstTag::Other;
        let mut frame = Frame::default();
        if let AstKind::ImportDeclaration(declaration) = kind {
            let type_only = declaration.import_kind == ImportOrExportKind::Type;
            let mut recorded = false;
            if let Some(specifiers) = &declaration.specifiers {
                for specifier in specifiers {
                    let result = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => self
                            .record_import(
                                declaration.source.value.as_str(),
                                ImportKind::Named,
                                Some(module_export_name(&specifier.imported)),
                                Some(specifier.local.name.as_str().to_owned()),
                                type_only || specifier.import_kind == ImportOrExportKind::Type,
                                specifier.span,
                            ),
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => self
                            .record_import(
                                declaration.source.value.as_str(),
                                ImportKind::Default,
                                Some("default".to_owned()),
                                Some(specifier.local.name.as_str().to_owned()),
                                type_only,
                                specifier.span,
                            ),
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            let local = specifier.local.name.as_str().to_owned();
                            let result = self.record_import(
                                declaration.source.value.as_str(),
                                ImportKind::Namespace,
                                Some("*".to_owned()),
                                Some(local.clone()),
                                type_only,
                                specifier.span,
                            );
                            if result.is_ok() {
                                self.namespace_imports
                                    .insert(local, declaration.source.value.as_str().to_owned());
                            }
                            result
                        }
                    };
                    if let Err(error) = result {
                        self.failure = Some(error);
                        break;
                    }
                    recorded = true;
                }
            }
            if !recorded
                && let Err(error) = self.record_import(
                    declaration.source.value.as_str(),
                    ImportKind::SideEffect,
                    None,
                    None,
                    type_only,
                    declaration.source.span,
                )
            {
                self.failure = Some(error);
            }
        }
        if let AstKind::ExportFromDeclaration(declaration) = kind {
            if declaration.specifiers.is_empty()
                && let Err(error) = self.record_import(
                    declaration.source.value.as_str(),
                    ImportKind::SideEffect,
                    None,
                    None,
                    declaration.export_kind == ImportOrExportKind::Type,
                    declaration.source.span,
                )
            {
                self.failure = Some(error);
            }
            for specifier in &declaration.specifiers {
                if let Err(error) = self.record_export(
                    Some(declaration.source.value.as_str()),
                    ExportKind::NamedReExport,
                    Some(module_export_name(&specifier.exported)),
                    Some(module_export_name(&specifier.local)),
                    declaration.export_kind == ImportOrExportKind::Type
                        || specifier.export_kind == ImportOrExportKind::Type,
                    specifier.span,
                ) {
                    self.failure = Some(error);
                    break;
                }
            }
        }
        if let AstKind::ExportAllDeclaration(declaration) = kind {
            let export_kind = if declaration.exported.is_some() {
                ExportKind::NamespaceReExport
            } else {
                ExportKind::StarReExport
            };
            let exported_name = declaration.exported.as_ref().map(module_export_name);
            if let Err(error) = self.record_export(
                Some(declaration.source.value.as_str()),
                export_kind,
                exported_name,
                None,
                declaration.export_kind == ImportOrExportKind::Type,
                declaration.source.span,
            ) {
                self.failure = Some(error);
            }
        }
        if let AstKind::ExportDeclaration(declaration) = kind {
            for (name, span, type_only) in declared_export_names(&declaration.declaration) {
                if let Err(error) = self.record_export(
                    None,
                    ExportKind::Local,
                    Some(name.clone()),
                    Some(name),
                    type_only,
                    span,
                ) {
                    self.failure = Some(error);
                    break;
                }
            }
        }
        if let AstKind::ExportNamedDeclaration(declaration) = kind {
            for specifier in &declaration.specifiers {
                if let Err(error) = self.record_export(
                    None,
                    ExportKind::Local,
                    Some(module_export_name(&specifier.exported)),
                    Some(module_export_name(&specifier.local)),
                    declaration.export_kind == ImportOrExportKind::Type
                        || specifier.export_kind == ImportOrExportKind::Type,
                    specifier.span,
                ) {
                    self.failure = Some(error);
                    break;
                }
            }
        }
        if let AstKind::ExportDefaultDeclaration(declaration) = kind {
            let local_name = if let ExportDefaultDeclarationKind::FunctionDeclaration(function) =
                &declaration.declaration
            {
                function.id.as_ref().map(|id| id.name.as_str().to_owned())
            } else if let ExportDefaultDeclarationKind::ClassDeclaration(class) =
                &declaration.declaration
            {
                class.id.as_ref().map(|id| id.name.as_str().to_owned())
            } else if let ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) =
                &declaration.declaration
            {
                Some(interface.id.name.as_str().to_owned())
            } else {
                None
            };
            if let Err(error) = self.record_export(
                None,
                ExportKind::Default,
                Some("default".to_owned()),
                local_name,
                false,
                declaration.span,
            ) {
                self.failure = Some(error);
            }
        }
        if let AstKind::ImportExpression(expression) = kind
            && let Expression::StringLiteral(source) = &expression.source
            && let Err(error) = self.record_import(
                source.value.as_str(),
                ImportKind::Dynamic,
                None,
                None,
                false,
                source.span,
            )
        {
            self.failure = Some(error);
        }
        if let AstKind::VariableDeclarator(declarator) = kind {
            let mut names = Vec::new();
            binding_names(&declarator.id, &mut names);
            for (name, _) in names {
                self.shadow_namespace_import(&name);
            }
            let name = if let BindingPattern::BindingIdentifier(identifier) = &declarator.id {
                Some((identifier.name.as_str().to_owned(), identifier.span))
            } else {
                None
            };
            tag = AstTag::VariableDeclarator(name);
        }
        if let AstKind::FormalParameters(parameters) = kind {
            for parameter in &parameters.items {
                let mut names = Vec::new();
                binding_names(&parameter.pattern, &mut names);
                for (name, _) in names {
                    self.shadow_namespace_import(&name);
                }
            }
            if let Some(rest) = &parameters.rest {
                let mut names = Vec::new();
                binding_names(&rest.rest.argument, &mut names);
                for (name, _) in names {
                    self.shadow_namespace_import(&name);
                }
            }
        }
        if let AstKind::CatchClause(clause) = kind
            && let Some(parameter) = &clause.param
        {
            let mut names = Vec::new();
            binding_names(&parameter.pattern, &mut names);
            for (name, _) in names {
                self.shadow_namespace_import(&name);
            }
        }
        if let AstKind::StaticMemberExpression(member) = kind
            && let Expression::Identifier(alias) = &member.object
        {
            self.pending_namespace_members.push((
                alias.name.as_str().to_owned(),
                member.property.name.as_str().to_owned(),
                member.property.span,
            ));
        }
        if let AstKind::ComputedMemberExpression(member) = kind
            && let Expression::Identifier(alias) = &member.object
            && let Expression::StringLiteral(key) = &member.expression
        {
            self.pending_namespace_members.push((
                alias.name.as_str().to_owned(),
                key.value.as_str().to_owned(),
                key.span,
            ));
        }
        if let AstKind::Class(class) = kind {
            frame.class_context = Some(self.current_class.take());
            frame.this_context = Some(self.this_class.take());
            if let Some(id) = &class.id {
                match self.declare(id.name.as_str(), NodeKind::Class, id.span) {
                    Ok(node) => {
                        self.scope.push(id.name.as_str().to_owned());
                        self.current_class = Some(self.scope.join("::"));
                        self.owners.push(node);
                        frame.scope = true;
                        frame.owner = true;
                    }
                    Err(error) => self.failure = Some(error),
                }
            }
        }
        if let AstKind::MethodDefinition(method) = kind {
            tag = AstTag::MethodDefinition(self.current_class.clone());
            if let Some((name, _)) = method_name(method) {
                match self.declare(name, NodeKind::Function, method.span) {
                    Ok(node) => {
                        self.scope.push(name.to_owned());
                        self.owners.push(node);
                        frame.scope = true;
                        frame.owner = true;
                    }
                    Err(error) => self.failure = Some(error),
                }
            }
        }
        if let AstKind::Function(function) = kind {
            frame.this_context = Some(self.this_class.take());
            let is_method_body =
                if let Some(AstTag::MethodDefinition(class)) = self.ast_stack.last() {
                    self.this_class = class.clone();
                    true
                } else {
                    false
                };
            if !is_method_body
                && matches!(
                    function.r#type,
                    FunctionType::FunctionDeclaration | FunctionType::TSDeclareFunction
                )
                && let Some(id) = &function.id
            {
                match self.declare(id.name.as_str(), NodeKind::Function, function.span) {
                    Ok(node) => {
                        self.scope.push(id.name.as_str().to_owned());
                        self.owners.push(node);
                        frame.scope = true;
                        frame.owner = true;
                    }
                    Err(error) => self.failure = Some(error),
                }
            }
        }
        if let AstKind::ArrowFunctionExpression(arrow) = kind
            && let Some(AstTag::VariableDeclarator(Some((name, binding)))) =
                self.ast_stack.last().cloned()
        {
            let definition = Span::new(binding.start, arrow.span.end);
            match self.declare(&name, NodeKind::Function, definition) {
                Ok(node) => {
                    self.scope.push(name);
                    self.owners.push(node);
                    frame.scope = true;
                    frame.owner = true;
                }
                Err(error) => self.failure = Some(error),
            }
        }
        if let AstKind::CallExpression(call) = kind {
            if let Expression::Identifier(identifier) = &call.callee
                && identifier.name.as_str() == "require"
                && let Some(Argument::StringLiteral(source)) = call.arguments.first()
                && let Err(error) = self.record_import(
                    source.value.as_str(),
                    ImportKind::CommonJs,
                    None,
                    None,
                    false,
                    source.span,
                )
            {
                self.failure = Some(error);
            }
            if let Some(owner) = self.owners.last().copied() {
                match self.call_target(&call.callee) {
                    Ok((target, span)) => {
                        if let Err(error) = self.record_call(owner, target, span) {
                            self.failure = Some(error);
                        }
                    }
                    Err(error) => self.failure = Some(error),
                }
            }
        }
        if let AstKind::NewExpression(new_expression) = kind
            && let Some(owner) = self.owners.last().copied()
        {
            match self.source_slice(new_expression.callee.span()) {
                Ok(target) => {
                    if let Err(error) =
                        self.record_call(owner, target, new_expression.callee.span())
                    {
                        self.failure = Some(error);
                    }
                }
                Err(error) => self.failure = Some(error),
            }
        }
        self.ast_stack.push(tag);
        self.frames.push(frame);
    }

    fn leave_node(&mut self, _kind: AstKind<'visitor>) {
        if let Some(frame) = self.frames.pop() {
            if frame.owner {
                self.owners.pop();
            }
            if let Some(previous) = frame.class_context {
                self.current_class = previous;
            }
            if let Some(previous) = frame.this_context {
                self.this_class = previous;
            }
            if frame.scope {
                self.scope.pop();
            }
        }
        self.ast_stack.pop();
    }
}

fn method_name<'method>(method: &'method MethodDefinition<'_>) -> Option<(&'method str, Span)> {
    if let PropertyKey::StaticIdentifier(identifier) = &method.key {
        Some((identifier.name.as_str(), identifier.span))
    } else {
        None
    }
}

fn convert_span(span: Span) -> Result<SourceSpan, ExtractorError> {
    let start = u64::from(span.start);
    let end = u64::from(span.end);
    SourceSpan::new(start, end).map_err(|error| ExtractorError::InvalidInput(error.to_string()))
}

#[cfg(test)]
mod tests;
