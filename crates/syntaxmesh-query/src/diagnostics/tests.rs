use syntaxmesh_core::{
    GenerationId, ModuleResolutionDiagnosticStatus, Node, NodeId, NodeKind, ProvenanceId,
};
use syntaxmesh_store::HistoricalNodePage;

fn node(name: &str, kind: NodeKind) -> Node {
    Node {
        id: NodeId::derive(&[name.as_bytes()]),
        kind,
        name: name.to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"diagnostic-page-test"]),
        extension_payload: None,
    }
}

#[test]
fn empty_match_page_still_advances_over_scanned_nodes() -> Result<(), Box<dyn std::error::Error>> {
    let generation = GenerationId::derive(&[b"diagnostic-page-generation"]);
    let scanned = node("ordinary", NodeKind::Function);
    let page = super::diagnostic_page(
        generation,
        HistoricalNodePage {
            items: vec![scanned.clone()],
            has_more: true,
        },
    )?;
    if page.generation != generation
        || !page.items.is_empty()
        || page.scanned_nodes != 1
        || page.next_after != Some(scanned.id)
    {
        return Err("empty diagnostic matches lost scan progress".into());
    }
    Ok(())
}

#[test]
fn continuation_is_last_scanned_node_not_last_match() -> Result<(), Box<dyn std::error::Error>> {
    let generation = GenerationId::derive(&[b"diagnostic-page-generation"]);
    let diagnostic = node(
        "diagnostic",
        NodeKind::ModuleResolutionDiagnostic {
            occurrence: NodeId::derive(&[b"import"]),
            status: ModuleResolutionDiagnosticStatus::Unresolved,
            candidate_paths: vec![],
        },
    );
    let ordinary = node("ordinary", NodeKind::Function);
    let page = super::diagnostic_page(
        generation,
        HistoricalNodePage {
            items: vec![diagnostic.clone(), ordinary.clone()],
            has_more: true,
        },
    )?;
    if page.items != vec![diagnostic]
        || page.scanned_nodes != 2
        || page.next_after != Some(ordinary.id)
    {
        return Err("diagnostic filtering changed scan continuation".into());
    }
    Ok(())
}

#[test]
fn terminal_empty_page_has_no_continuation() -> Result<(), Box<dyn std::error::Error>> {
    let page = super::diagnostic_page(
        GenerationId::derive(&[b"generation"]),
        HistoricalNodePage {
            items: vec![],
            has_more: false,
        },
    )?;
    if !page.items.is_empty() || page.scanned_nodes != 0 || page.next_after.is_some() {
        return Err("terminal diagnostic page is inconsistent".into());
    }
    Ok(())
}

#[test]
fn impossible_backend_continuation_is_rejected() {
    assert!(
        super::diagnostic_page(
            GenerationId::derive(&[b"generation"]),
            HistoricalNodePage {
                items: vec![],
                has_more: true,
            }
        )
        .is_err()
    );
}
