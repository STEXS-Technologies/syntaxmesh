use super::{MAX_SYNTAX_DIAGNOSTIC_BYTES, SyntaxDiagnostic};

#[test]
fn payload_round_trips_with_explicit_loss_and_exact_bytes() {
    for message in [String::new(), "invalid 🦀".to_owned(), "界".repeat(2000)] {
        let diagnostic = SyntaxDiagnostic::new(&message);
        let payload = diagnostic.payload();
        assert_eq!(payload.schema_version, 1);
        assert_eq!(
            payload.bytes.first(),
            Some(&u8::from(diagnostic.truncated()))
        );
        assert_eq!(
            SyntaxDiagnostic::from_payload(Some(&payload)),
            Ok(Some(diagnostic))
        );
    }
}

#[test]
fn malformed_recognized_payloads_fail_closed() {
    let valid = SyntaxDiagnostic::new("invalid").payload();
    for bytes in [
        vec![],
        vec![2],
        vec![0, 255],
        vec![0; MAX_SYNTAX_DIAGNOSTIC_BYTES + 2],
    ] {
        let mut malformed = valid.clone();
        malformed.bytes = bytes;
        assert!(SyntaxDiagnostic::from_payload(Some(&malformed)).is_err());
    }
    let mut unsupported = valid.clone();
    unsupported.schema_version = 2;
    assert!(SyntaxDiagnostic::from_payload(Some(&unsupported)).is_err());
    let mut unrelated = valid;
    unrelated.namespace = "another.producer".to_owned();
    unrelated.bytes = vec![255];
    assert_eq!(SyntaxDiagnostic::from_payload(Some(&unrelated)), Ok(None));
    assert_eq!(SyntaxDiagnostic::from_payload(None), Ok(None));
}

#[test]
fn preserves_empty_short_and_exact_limit_messages() {
    for message in [
        String::new(),
        "invalid syntax 🦀".to_owned(),
        "a".repeat(MAX_SYNTAX_DIAGNOSTIC_BYTES),
    ] {
        let diagnostic = SyntaxDiagnostic::new(&message);
        assert_eq!(diagnostic.message(), message);
        assert!(!diagnostic.truncated());
    }
}

#[test]
fn truncates_ascii_without_an_implicit_ellipsis() {
    let message = "a".repeat(MAX_SYNTAX_DIAGNOSTIC_BYTES + 1);
    let diagnostic = SyntaxDiagnostic::new(&message);
    assert_eq!(diagnostic.message().len(), MAX_SYNTAX_DIAGNOSTIC_BYTES);
    assert!(diagnostic.truncated());
    assert!(message.starts_with(diagnostic.message()));
}

#[test]
fn retains_only_complete_unicode_characters_at_every_boundary() {
    for character in ['é', '界', '🦀'] {
        for offset in 0..character.len_utf8() {
            let prefix = "a".repeat(MAX_SYNTAX_DIAGNOSTIC_BYTES - offset);
            let message = format!("{prefix}{character}tail");
            let diagnostic = SyntaxDiagnostic::new(&message);
            assert!(diagnostic.message().len() <= MAX_SYNTAX_DIAGNOSTIC_BYTES);
            assert!(message.starts_with(diagnostic.message()));
            assert!(diagnostic.truncated());
            assert_eq!(diagnostic.message(), prefix);
        }
    }
}
