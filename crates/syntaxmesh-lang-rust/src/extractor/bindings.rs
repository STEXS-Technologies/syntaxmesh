use std::collections::BTreeSet;

use syn::visit::Visit;

use super::CallVisitor;

#[derive(Default)]
pub(super) struct Bindings {
    scopes: Vec<BTreeSet<String>>,
}

impl Bindings {
    pub(super) fn push(&mut self) {
        self.scopes.push(BTreeSet::new());
    }

    pub(super) fn pop(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    pub(super) fn block_items(&mut self, block: &syn::Block) {
        if let Some(scope) = self.scopes.last_mut() {
            for statement in &block.stmts {
                let name = match statement {
                    syn::Stmt::Item(syn::Item::Const(item)) => Some(&item.ident),
                    syn::Stmt::Item(syn::Item::Static(item)) => Some(&item.ident),
                    syn::Stmt::Local(_)
                    | syn::Stmt::Item(_)
                    | syn::Stmt::Expr(..)
                    | syn::Stmt::Macro(_) => None,
                };
                if let Some(name) = name {
                    scope.insert(name.to_string());
                }
            }
        }
    }

    pub(super) fn pattern(&mut self, pattern: &syn::Pat) {
        let mut collector = PatternNames::default();
        collector.visit_pat(pattern);
        if let Some(scope) = self.scopes.last_mut() {
            scope.extend(collector.names);
        }
    }
}

#[derive(Default)]
struct PatternNames {
    names: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for PatternNames {
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.names.insert(pattern.ident.to_string());
        syn::visit::visit_pat_ident(self, pattern);
    }

    fn visit_expr(&mut self, _expression: &'ast syn::Expr) {}
}

impl CallVisitor {
    pub(super) fn parameters(&mut self, signature: &syn::Signature) {
        self.bindings.push();
        for argument in &signature.inputs {
            if let syn::FnArg::Typed(argument) = argument {
                self.bindings.pattern(&argument.pat);
            }
        }
    }
}
