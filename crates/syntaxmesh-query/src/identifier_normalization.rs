use std::collections::BTreeSet;

/// Revision to bind into derived indexes using these normalization rules.
pub const IDENTIFIER_NORMALIZATION_REVISION: &str = "syntaxmesh-identifier-terms-v1";

/// Sorted unique lowercase identifier words, split at punctuation, camelCase
/// boundaries and acronym-to-word transitions. No stemming or stop-word removal.
#[must_use]
pub fn identifier_terms(identifier: &str) -> BTreeSet<String> {
    let mut characters = identifier.chars().peekable();
    let mut previous_character: Option<char> = None;
    let mut terms = BTreeSet::new();
    let mut word = String::new();
    while let Some(character) = characters.next() {
        let previous = previous_character.replace(character);
        if !character.is_alphanumeric() {
            if !word.is_empty() {
                terms.insert(std::mem::take(&mut word));
            }
            continue;
        }
        let next = characters.peek();
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

#[cfg(test)]
mod tests;
