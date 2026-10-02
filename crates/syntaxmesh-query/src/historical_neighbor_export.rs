use syntaxmesh_api_model::{TEMPORAL_EXPORT_SCHEMA_VERSION, TemporalQueryMode, TemporalRecord};

use crate::HistoricalNeighborPage;

#[cfg(test)]
mod tests;

impl HistoricalNeighborPage {
    /// Convert an already validated page into the existing temporal export
    /// item/footer records, preserving ordering and its typed continuation.
    /// This does not validate caller-constructed pages or execute a query.
    #[must_use]
    pub fn into_records(self) -> Vec<TemporalRecord> {
        let mut records = self
            .items
            .into_iter()
            .map(|item| TemporalRecord::HistoricalNeighbor {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation: self.generation,
                endpoint: self.endpoint,
                direction: self.direction,
                edge: item.edge,
                neighbor: item.neighbor,
            })
            .collect::<Vec<_>>();
        records.push(TemporalRecord::HistoricalNeighborFooter {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            generation: self.generation,
            endpoint: self.endpoint,
            direction: self.direction,
            returned: u64::try_from(records.len()).unwrap_or(u64::MAX),
            has_more: self.has_more,
            next_cursor: self.next_cursor,
        });
        records
    }
}
