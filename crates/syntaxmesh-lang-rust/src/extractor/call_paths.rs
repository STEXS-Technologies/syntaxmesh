use quote::ToTokens;

pub(super) fn method_target(call: &syn::ExprMethodCall, impl_type: Option<&str>) -> String {
    let method = call.method.to_string();
    if is_self_receiver(&call.receiver)
        && let Some(impl_type) = impl_type
    {
        return format!("{impl_type}::{method}");
    }
    format!("{}.{method}", call.receiver.to_token_stream())
}

fn is_self_receiver(receiver: &syn::Expr) -> bool {
    if let syn::Expr::Path(path) = receiver {
        path.qself.is_none() && path.path.is_ident("self")
    } else if let syn::Expr::Reference(reference) = receiver {
        is_self_receiver(&reference.expr)
    } else if let syn::Expr::Paren(parenthesized) = receiver {
        is_self_receiver(&parenthesized.expr)
    } else if let syn::Expr::Group(group) = receiver {
        is_self_receiver(&group.expr)
    } else {
        false
    }
}

/// Preserve distinctions that cannot safely be interpreted as bare calls.
pub(super) fn call_target(path: &syn::ExprPath) -> String {
    if path.qself.is_some() {
        return path.to_token_stream().to_string();
    }
    path_target(&path.path)
}

pub(super) fn path_target(path: &syn::Path) -> String {
    let segmented = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    if path.leading_colon.is_some() {
        format!("::{segmented}")
    } else {
        segmented
    }
}
