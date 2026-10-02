//! Additive syntactic test intent without changing callable identities.

use super::*;
use std::collections::BTreeSet;
use syntaxmesh_language_sdk::SourceRole;

pub(super) fn apply(
    syntax: &syn::File,
    source: &RustSource<'_>,
    functions: &BTreeMap<(u64, u64), NodeId>,
    nodes: &mut [Node],
) -> Result<(), ExtractorError> {
    let mut visitor = TestRoles {
        source,
        functions,
        ids: BTreeSet::new(),
        failure: None,
    };
    visitor.visit_file(syntax);
    if let Some(error) = visitor.failure {
        return Err(error);
    }
    for node in nodes {
        if !visitor.ids.remove(&node.id) {
            continue;
        }
        node.extension_payload = SourceRole::TestIntent.payload();
    }
    if !visitor.ids.is_empty() {
        return Err(ExtractorError::InvalidInput(
            "missing test callable declaration".to_owned(),
        ));
    }
    Ok(())
}

struct TestRoles<'context, 'source> {
    source: &'context RustSource<'source>,
    functions: &'context BTreeMap<(u64, u64), NodeId>,
    ids: BTreeSet<NodeId>,
    failure: Option<ExtractorError>,
}

impl<'ast> Visit<'ast> for TestRoles<'_, '_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if self.failure.is_none() && item.attrs.iter().any(test_intent) {
            match declaration_at(self.functions, &item.sig.ident, self.source) {
                Ok(Some(id)) => {
                    self.ids.insert(id);
                }
                Ok(None) => {
                    self.failure = Some(ExtractorError::InvalidInput(
                        "missing test declaration".to_owned(),
                    ))
                }
                Err(error) => self.failure = Some(error),
            }
        }
        syn::visit::visit_item_fn(self, item);
    }
}

fn test_intent(attribute: &syn::Attribute) -> bool {
    if !matches!(attribute.style, syn::AttrStyle::Outer) {
        return false;
    }
    if matches!(&attribute.meta, syn::Meta::Path(path) if path.is_ident("test")) {
        return true;
    }
    let path = match &attribute.meta {
        syn::Meta::Path(path) => path,
        syn::Meta::List(list) => {
            if !matches!(list.delimiter, syn::MacroDelimiter::Paren(_)) {
                return false;
            }
            &list.path
        }
        syn::Meta::NameValue(_) => return false,
    };
    if path.leading_colon.is_some() || path.segments.len() != 2 {
        return false;
    }
    let mut segments = path.segments.iter();
    let (Some(framework), Some(role)) = (segments.next(), segments.next()) else {
        return false;
    };
    (framework.ident == "tokio" || framework.ident == "async_std")
        && role.ident == "test"
        && matches!(framework.arguments, syn::PathArguments::None)
        && matches!(role.arguments, syn::PathArguments::None)
}

#[cfg(test)]
mod tests;
