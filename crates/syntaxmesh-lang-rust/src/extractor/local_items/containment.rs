use super::super::*;
use syntaxmesh_core::{Edge, EdgeId};

pub(in crate::extractor) fn extract(
    syntax: &syn::File,
    source: &RustSource<'_>,
    provenance: ProvenanceId,
    functions: &BTreeMap<(u64, u64), NodeId>,
) -> Result<Vec<Edge>, ExtractorError> {
    let mut visitor = Containment {
        source,
        provenance,
        functions,
        edges: Vec::new(),
        failure: None,
    };
    visitor.visit_file(syntax);
    if let Some(error) = visitor.failure {
        return Err(error);
    }
    Ok(visitor.edges)
}

struct Containment<'context, 'source> {
    source: &'context RustSource<'source>,
    provenance: ProvenanceId,
    functions: &'context BTreeMap<(u64, u64), NodeId>,
    edges: Vec<Edge>,
    failure: Option<ExtractorError>,
}

impl Containment<'_, '_> {
    fn body(&mut self, name: &syn::Ident, block: &syn::Block) -> Result<(), ExtractorError> {
        let Some(owner) = declaration_at(self.functions, name, self.source)? else {
            return Ok(());
        };
        for item in super::items(block) {
            if let syn::Item::Fn(function) = item
                && let Some(target) =
                    declaration_at(self.functions, &function.sig.ident, self.source)?
            {
                self.edges.push(Edge {
                    id: EdgeId::derive(&[
                        &owner.0.0,
                        &target.0.0,
                        b"rust-local-function-contains-v1",
                    ]),
                    source: owner,
                    target,
                    relation: RelationKind::Contains,
                    provenance: self.provenance,
                    extension_payload: None,
                });
            }
        }
        Ok(())
    }
}

impl<'ast> Visit<'ast> for Containment<'_, '_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if self.failure.is_none() {
            self.failure = self.body(&item.sig.ident, &item.block).err();
        }
        syn::visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if self.failure.is_none() {
            self.failure = self.body(&item.sig.ident, &item.block).err();
        }
        syn::visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if self.failure.is_none()
            && let Some(block) = &item.default
        {
            self.failure = self.body(&item.sig.ident, block).err();
        }
        syn::visit::visit_trait_item_fn(self, item);
    }
}
