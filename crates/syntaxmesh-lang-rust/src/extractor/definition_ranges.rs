//! Enrich emitted evidence only after identifier-keyed owner extraction finishes.

use super::*;

#[cfg(test)]
mod tests;

pub(super) fn apply(
    syntax: &syn::File,
    source: &RustSource<'_>,
    functions: &BTreeMap<(u64, u64), NodeId>,
    nodes: &mut [Node],
) -> Result<(), ExtractorError> {
    let mut visitor = DefinitionRanges {
        source,
        functions,
        ranges: BTreeMap::new(),
        failure: None,
    };
    visitor.visit_file(syntax);
    if let Some(error) = visitor.failure {
        return Err(error);
    }
    for node in nodes {
        if let Some(span) = visitor.ranges.remove(&node.id)
            && let Some(location) = &mut node.source
        {
            location.span = span;
        }
    }
    Ok(())
}

struct DefinitionRanges<'borrow, 'source> {
    source: &'borrow RustSource<'source>,
    functions: &'borrow BTreeMap<(u64, u64), NodeId>,
    ranges: BTreeMap<NodeId, SourceSpan>,
    failure: Option<ExtractorError>,
}

impl DefinitionRanges<'_, '_> {
    fn record(&mut self, name: &syn::Ident, definition: Span) {
        if self.failure.is_some() {
            return;
        }
        match declaration_at(self.functions, name, self.source).and_then(|id| {
            id.map(|id| source_span(self.source, definition).map(|span| (id, span)))
                .transpose()
        }) {
            Ok(Some((id, span))) => {
                self.ranges.insert(id, span);
            }
            Ok(None) => {}
            Err(error) => self.failure = Some(error),
        }
    }
}

impl<'ast> Visit<'ast> for DefinitionRanges<'_, '_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.record(&item.sig.ident, item.span());
        syn::visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.record(&item.sig.ident, item.span());
        syn::visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        self.record(&item.sig.ident, item.span());
        syn::visit::visit_trait_item_fn(self, item);
    }
}
