use super::*;
use syntaxmesh_core::{Edge, EdgeId};

type ImplementationFacts = (Vec<Node>, Vec<Edge>, Vec<Reference>);

pub(super) fn extract(
    syntax: &syn::File,
    source: &RustSource<'_>,
    provenance: ProvenanceId,
    file_module: NodeId,
    declarations: &BTreeMap<(u64, u64), NodeId>,
) -> Result<ImplementationFacts, ExtractorError> {
    let mut visitor = ImplementationVisitor {
        source,
        provenance,
        owners: vec![Some(file_module)],
        declarations,
        path: Vec::new(),
        ordinals: BTreeMap::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        references: Vec::new(),
        failure: None,
    };
    visitor.visit_file(syntax);
    if let Some(error) = visitor.failure {
        return Err(error);
    }
    Ok((visitor.nodes, visitor.edges, visitor.references))
}

struct ImplementationVisitor<'context, 'source> {
    source: &'context RustSource<'source>,
    provenance: ProvenanceId,
    owners: Vec<Option<NodeId>>,
    declarations: &'context BTreeMap<(u64, u64), NodeId>,
    path: Vec<String>,
    ordinals: BTreeMap<String, u64>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    references: Vec<Reference>,
    failure: Option<ExtractorError>,
}

impl ImplementationVisitor<'_, '_> {
    fn implementation(&mut self, item: &syn::ItemImpl) -> Result<NodeId, ExtractorError> {
        let self_type = item.self_ty.to_token_stream().to_string();
        let negative = item.modifiers.polarity.is_some();
        let trait_name = item.trait_.as_ref().map(|(path, _)| {
            format!(
                "{}{}",
                if negative { "!" } else { "" },
                path.to_token_stream()
            )
        });
        let name = trait_name.as_ref().map_or_else(
            || format!("impl {self_type}"),
            |name| format!("impl {name} for {self_type}"),
        );
        let header = syntaxmesh_core::StableId::derive(
            "rust-implementation-header-v1",
            &[
                self.path.join("::").as_bytes(),
                name.as_bytes(),
                item.generics.to_token_stream().to_string().as_bytes(),
                item.generics
                    .where_clause
                    .to_token_stream()
                    .to_string()
                    .as_bytes(),
                &[u8::from(item.unsafety.is_some())],
                &[u8::from(item.modifiers.defaultness.is_some())],
            ],
        );
        let ordinal = self.ordinals.entry(header.to_hex()).or_default();
        let id = NodeId::derive(&[
            &self.source.file.file_id.0.0,
            b"rust-implementation-v1",
            &header.0,
            &ordinal.to_le_bytes(),
        ]);
        *ordinal = ordinal.saturating_add(1);
        self.nodes.push(Node {
            id,
            kind: NodeKind::External {
                namespace: RUST_PRODUCER_NAMESPACE.to_owned(),
                kind: if negative {
                    "negative-implementation"
                } else {
                    "implementation"
                }
                .to_owned(),
            },
            name,
            owner_file: Some(self.source.file.file_id),
            source: Some(SourceLocation {
                file_id: self.source.file.file_id,
                content_hash: self.source.file.content_hash,
                span: source_span(self.source, item.span())?,
            }),
            provenance: self.provenance,
            extension_payload: None,
        });
        if let Some(Some(owner)) = self.owners.last() {
            self.contains(*owner, id);
        }
        for member in &item.items {
            if let syn::ImplItem::Fn(method) = member
                && let Some(method_id) =
                    declaration_at(self.declarations, &method.sig.ident, self.source)?
            {
                self.contains(id, method_id);
            }
        }
        if !negative && let Some((trait_path, _)) = &item.trait_ {
            let span = source_span(self.source, trait_path.span())?;
            self.references.push(Reference {
                resolution: syntaxmesh_language_sdk::ReferenceResolution::Name,
                id: NodeId::derive(&[&id.0.0, b"rust-implemented-trait-v1"]),
                source: id,
                target: call_paths::path_target(trait_path),
                relation: RelationKind::Implements,
                source_location: SourceLocation {
                    file_id: self.source.file.file_id,
                    content_hash: self.source.file.content_hash,
                    span,
                },
                provenance: self.provenance,
            });
        }
        Ok(id)
    }

    fn enter_declaration(&mut self, name: &syn::Ident) {
        self.path.push(name.to_string());
        let owner = match declaration_at(self.declarations, name, self.source) {
            Ok(owner) => owner,
            Err(error) => {
                self.failure = Some(error);
                None
            }
        };
        self.owners.push(owner);
    }

    fn leave_declaration(&mut self) {
        self.path.pop();
        self.owners.pop();
    }

    fn contains(&mut self, source: NodeId, target: NodeId) {
        self.edges.push(Edge {
            id: EdgeId::derive(&[&source.0.0, &target.0.0, b"rust-implementation-contains-v1"]),
            source,
            target,
            relation: RelationKind::Contains,
            provenance: self.provenance,
            extension_payload: None,
        });
    }
}

impl<'ast> Visit<'ast> for ImplementationVisitor<'_, '_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_mod(self, item);
        self.leave_declaration();
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.enter_declaration(&item.sig.ident);
        syn::visit::visit_item_fn(self, item);
        self.leave_declaration();
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_const(self, item);
        self.leave_declaration();
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_static(self, item);
        self.leave_declaration();
    }

    fn visit_impl_item_const(&mut self, item: &'ast syn::ImplItemConst) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_impl_item_const(self, item);
        self.leave_declaration();
    }

    fn visit_trait_item_const(&mut self, item: &'ast syn::TraitItemConst) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_trait_item_const(self, item);
        self.leave_declaration();
    }

    fn visit_impl_item_type(&mut self, item: &'ast syn::ImplItemType) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_impl_item_type(self, item);
        self.leave_declaration();
    }

    fn visit_trait_item_type(&mut self, item: &'ast syn::TraitItemType) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_trait_item_type(self, item);
        self.leave_declaration();
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_struct(self, item);
        self.leave_declaration();
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_enum(self, item);
        self.leave_declaration();
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_type(self, item);
        self.leave_declaration();
    }

    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_union(self, item);
        self.leave_declaration();
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.enter_declaration(&item.sig.ident);
        syn::visit::visit_impl_item_fn(self, item);
        self.leave_declaration();
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.enter_declaration(&item.ident);
        syn::visit::visit_item_trait(self, item);
        self.leave_declaration();
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        self.enter_declaration(&item.sig.ident);
        syn::visit::visit_trait_item_fn(self, item);
        self.leave_declaration();
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let owner = if self.failure.is_none() {
            match self.implementation(item) {
                Ok(id) => Some(id),
                Err(error) => {
                    self.failure = Some(error);
                    None
                }
            }
        } else {
            None
        };
        self.owners.push(owner);
        self.path.push(item.self_ty.to_token_stream().to_string());
        syn::visit::visit_item_impl(self, item);
        self.leave_declaration();
    }
}
