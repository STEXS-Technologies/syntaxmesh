use crate::QueryError;
use syntaxmesh_core::{FactPayload, FactRef, GenerationId};
use syntaxmesh_store::{FactHistoryCursor, FactHistoryPage};

pub(super) fn validate_page(
    page: &FactHistoryPage,
    generation: GenerationId,
    as_of_sequence: u64,
    limit: usize,
    after: Option<FactHistoryCursor>,
    mut previous: Option<(FactRef, u64)>,
) -> Result<Option<(FactRef, u64)>, QueryError> {
    if limit == 0 || page.items.len() > limit {
        return Err(QueryError::InvalidLimit);
    }
    for entry in &page.items {
        let key = (entry.fact, entry.valid_from_sequence);
        let payload_fact = match &entry.version.payload {
            FactPayload::File(file) => FactRef::File(file.file_id),
            FactPayload::Provenance(provenance) => FactRef::Provenance(provenance.id),
            FactPayload::Node(node) => FactRef::Node(node.id),
            FactPayload::Edge(edge) => FactRef::Edge(edge.id),
        };
        if payload_fact != entry.fact
            || previous.is_some_and(|last| last >= key)
            || entry.valid_from_sequence == 0
            || entry.valid_from_sequence > as_of_sequence
            || entry.valid_until_sequence.is_some() != entry.version.valid_until.is_some()
            || entry
                .valid_until_sequence
                .is_some_and(|until| until <= entry.valid_from_sequence || until > as_of_sequence)
        {
            return Err(QueryError::Context(
                "invalid inference history entry".to_owned(),
            ));
        }
        previous = Some(key);
    }
    if page.next_cursor.is_some_and(|next| {
        next.as_of_generation != generation
            || Some(next) == after
            || page.items.last().is_none_or(|entry| {
                entry.fact != next.after_fact || entry.version.valid_from != next.after_valid_from
            })
    }) {
        return Err(QueryError::Context(
            "invalid inference history continuation".to_owned(),
        ));
    }
    Ok(previous)
}
