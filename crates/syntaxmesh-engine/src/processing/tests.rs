use super::*;
use syntaxmesh_core::FileVersion;
use syntaxmesh_language_sdk::ExtractorIdentity;

#[test]
fn malformed_processing_evidence_never_becomes_clean_coverage() {
    let generation = GenerationId::derive(&[b"coverage"]);
    let file = FileVersion {
        file_id: FileId::derive(&[b"input"]),
        normalized_path: "input.rs".to_owned(),
        content_hash: [1; 32],
        size_bytes: 10,
    };
    let (node, provenance) = SourceProcessing::SyntaxFailed(SyntaxDiagnostic::new("invalid"))
        .facts(&file, &ExtractorIdentity::new("syntaxmesh.lang.rust", "1"));
    let original = GraphSnapshot {
        files: vec![file],
        nodes: vec![node],
        provenance: vec![provenance],
        edges: Vec::new(),
    };
    assert!(summarize(generation, &original).is_ok());
    let mut malformed = original.clone();
    malformed.nodes.clear();
    assert!(
        matches!(summarize(generation, &malformed), Ok(report) if report.unclassified.len() == 1 && report.completed.is_empty())
    );
    malformed = original.clone();
    for altered_node in &mut malformed.nodes {
        altered_node.owner_file = None;
    }
    assert!(summarize(generation, &malformed).is_err());
    malformed = original.clone();
    malformed.provenance.clear();
    assert!(summarize(generation, &malformed).is_err());
    malformed = original.clone();
    for altered_node in &mut malformed.nodes {
        altered_node.extension_payload = None;
    }
    assert!(summarize(generation, &malformed).is_err());
    malformed = original.clone();
    for altered_file in &mut malformed.files {
        altered_file.content_hash = [2; 32];
    }
    assert!(summarize(generation, &malformed).is_err());
    malformed = original.clone();
    malformed.nodes.extend(original.nodes);
    assert!(summarize(generation, &malformed).is_err());
}
