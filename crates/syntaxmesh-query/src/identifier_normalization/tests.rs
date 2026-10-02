use super::identifier_terms;

#[test]
fn streaming_terms_match_previous_character_vector_algorithm() {
    let alphabet = ['a', 'A', '1', '_', 'É', 'Σ', '\u{301}', '中'];
    for first in alphabet {
        for second in alphabet {
            for third in alphabet {
                let input = [first, second, third].into_iter().collect::<String>();
                assert_eq!(identifier_terms(&input), previous_terms(&input));
            }
        }
    }
    let large = "HTTPClient::executeWithRetry_retry(ÉclairHTTP2Client); ".repeat(10_000);
    assert_eq!(identifier_terms(&large), previous_terms(&large));
}

fn previous_terms(identifier: &str) -> std::collections::BTreeSet<String> {
    let characters = identifier.chars().collect::<Vec<_>>();
    let mut terms = std::collections::BTreeSet::new();
    let mut word = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        if !character.is_alphanumeric() {
            if !word.is_empty() {
                terms.insert(std::mem::take(&mut word));
            }
            continue;
        }
        let previous = index
            .checked_sub(1)
            .and_then(|previous| characters.get(previous));
        let next = characters.get(index.saturating_add(1));
        let lower_to_upper = previous.is_some_and(|previous| {
            (previous.is_lowercase() || previous.is_numeric()) && character.is_uppercase()
        });
        let acronym_to_word = previous.is_some_and(|previous| previous.is_uppercase())
            && character.is_uppercase()
            && next.is_some_and(|next| next.is_lowercase());
        if !word.is_empty() && (lower_to_upper || acronym_to_word) {
            terms.insert(std::mem::take(&mut word));
        }
        word.extend(character.to_lowercase());
    }
    if !word.is_empty() {
        terms.insert(word);
    }
    terms
}

#[test]
fn normalization_preserves_existing_identifier_vocabulary() {
    for (input, expected) in [
        (
            "HTTPClient::generateBlockDoc",
            vec!["block", "client", "doc", "generate", "http"],
        ),
        ("execute_with_retry", vec!["execute", "retry", "with"]),
        ("ÉclairHTTP2Client", vec!["client", "http2", "éclair"]),
        ("retry-retry", vec!["retry"]),
        ("...", vec![]),
    ] {
        assert_eq!(
            identifier_terms(input).into_iter().collect::<Vec<_>>(),
            expected
        );
    }
}
