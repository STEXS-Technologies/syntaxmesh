use super::*;

pub(super) fn collect_nodes(
    item: &syn::ItemTrait,
    path: &[String],
    source: &RustSource<'_>,
    provenance: ProvenanceId,
    nodes: &mut declarations::Declarations,
) -> Result<(), ExtractorError> {
    let mut trait_path = path.to_vec();
    trait_path.push(item.ident.to_string());
    for member in &item.items {
        if let syn::TraitItem::Fn(method) = member {
            RustExtractor::push_item_node(
                NodeKind::Function,
                &method.sig.ident,
                &trait_path,
                source,
                provenance,
                nodes,
            )?;
            if let Some(body) = &method.default {
                let scope = local_items::scope(&trait_path, &method.sig.ident);
                for nested in local_items::items(body) {
                    RustExtractor::collect_items(
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
    Ok(())
}

pub(super) fn collect_references(
    item: &syn::ItemTrait,
    path: &[String],
    nodes: &BTreeMap<(u64, u64), NodeId>,
    source: &RustSource<'_>,
    provenance: ProvenanceId,
    occurrences: &mut BTreeMap<(NodeId, String), usize>,
    references: &mut Vec<Reference>,
) -> Result<(), ExtractorError> {
    let trait_path = local_items::scope(path, &item.ident);
    for member in &item.items {
        if let syn::TraitItem::Fn(method) = member
            && let Some(body) = &method.default
            && let Some(source_node) = declaration_at(nodes, &method.sig.ident, source)?
        {
            // A trait default has no concrete implementing type. Keep `self`
            // calls receiver-qualified instead of pretending trait dispatch.
            let mut visitor = CallVisitor::default();
            visitor.parameters(&method.sig);
            visitor.visit_block(body);
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
            let scope = local_items::scope(&trait_path, &method.sig.ident);
            for nested in local_items::items(body) {
                RustExtractor::collect_references(
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
    Ok(())
}
