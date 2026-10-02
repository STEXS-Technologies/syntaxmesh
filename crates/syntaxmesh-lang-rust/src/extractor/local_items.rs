use syn::visit::Visit;

mod containment;

pub(super) use containment::extract;

#[cfg(test)]
mod tests;

pub(super) fn items(block: &syn::Block) -> Vec<&syn::Item> {
    let mut visitor = LocalItems { items: Vec::new() };
    visitor.visit_block(block);
    visitor.items
}

struct LocalItems<'syntax> {
    items: Vec<&'syntax syn::Item>,
}

impl<'syntax> Visit<'syntax> for LocalItems<'syntax> {
    fn visit_item(&mut self, item: &'syntax syn::Item) {
        // The existing collectors recurse through each item's own body.
        self.items.push(item);
    }
}

pub(super) fn scope(path: &[String], name: &syn::Ident) -> Vec<String> {
    let mut scope = path.to_vec();
    scope.push(name.to_string());
    scope
}
