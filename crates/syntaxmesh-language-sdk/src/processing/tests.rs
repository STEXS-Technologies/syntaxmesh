use super::SourceProcessing;
use crate::{ExtractorIdentity, SyntaxDiagnostic};
use syntaxmesh_core::{FileId, FileVersion};

#[test]
fn processing_identity_tracks_file_not_status_hash_or_producer() {
    let file = FileVersion {
        file_id: FileId::derive(&[b"file"]),
        normalized_path: "input.rs".to_owned(),
        content_hash: [1; 32],
        size_bytes: 20,
    };
    let producer = ExtractorIdentity::new("syntaxmesh.lang.rust", "1");
    let (completed, success_provenance) = SourceProcessing::Completed.facts(&file, &producer);
    let (failed, failure_provenance) =
        SourceProcessing::SyntaxFailed(SyntaxDiagnostic::new("invalid syntax"))
            .facts(&file, &producer);
    assert_eq!(completed.id, failed.id);
    assert_ne!(success_provenance.id, failure_provenance.id);
    assert_ne!(failure_provenance.producer_namespace, producer.namespace);
    assert_eq!(failed.provenance, failure_provenance.id);
    assert_eq!(failed.source, failure_provenance.source);
    assert_eq!(
        SourceProcessing::from_facts(&file, &completed, &success_provenance),
        Ok(Some(SourceProcessing::Completed))
    );
    assert_eq!(
        SourceProcessing::from_facts(&file, &failed, &failure_provenance),
        Ok(Some(SourceProcessing::SyntaxFailed(SyntaxDiagnostic::new(
            "invalid syntax"
        ))))
    );
    let mut corrupted = failed.clone();
    corrupted.owner_file = None;
    assert!(SourceProcessing::from_facts(&file, &corrupted, &failure_provenance).is_err());
    corrupted = failed.clone();
    corrupted.extension_payload = None;
    assert!(SourceProcessing::from_facts(&file, &corrupted, &failure_provenance).is_err());
    assert!(SourceProcessing::from_facts(&file, &failed, &success_provenance).is_err());
    assert!(completed.extension_payload.is_none());
    assert_eq!(
        SyntaxDiagnostic::from_payload(failed.extension_payload.as_ref()),
        Ok(Some(SyntaxDiagnostic::new("invalid syntax")))
    );
    let mut changed = file.clone();
    changed.content_hash = [2; 32];
    let upgraded = ExtractorIdentity::new("syntaxmesh.lang.rust", "2");
    for (source, identity) in [(&changed, &producer), (&file, &upgraded)] {
        let (node, provenance) = SourceProcessing::Completed.facts(source, identity);
        assert_eq!(node.id, completed.id);
        assert_ne!(provenance.id, success_provenance.id);
    }
    changed.file_id = FileId::derive(&[b"different"]);
    assert_ne!(
        SourceProcessing::Completed.facts(&changed, &producer).0.id,
        completed.id
    );
}
