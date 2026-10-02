use syntaxmesh_core::NodeKind;

/// Reserved namespace for language-independent, source-backed anchor targets.
pub const SOURCE_ANCHOR_NAMESPACE: &str = "syntaxmesh.source";

/// A qualified source anchor uses its complete name, never a terminal alias.
#[must_use]
pub fn is_source_anchor(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::External { namespace, kind }
        if namespace == SOURCE_ANCHOR_NAMESPACE && kind == "anchor")
}

/// Definition kinds eligible for conservative source-reference resolution.
#[must_use]
pub fn is_reference_target_kind(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Function
            | NodeKind::Test
            | NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Trait
            | NodeKind::File
            | NodeKind::Document
    ) || is_source_anchor(kind)
}
