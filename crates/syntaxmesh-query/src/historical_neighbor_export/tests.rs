use syntaxmesh_api_model::{TEMPORAL_EXPORT_SCHEMA_VERSION, TemporalQueryMode, TemporalRecord};
use syntaxmesh_core::{EdgeDirection, GenerationId, NodeId};

use crate::HistoricalNeighborPage;

#[test]
fn empty_page_retains_selection_in_one_terminal_footer() {
    let generation = GenerationId::derive(&[b"export-generation"]);
    let endpoint = NodeId::derive(&[b"export-endpoint"]);
    for direction in [EdgeDirection::Incoming, EdgeDirection::Outgoing] {
        let records = HistoricalNeighborPage {
            generation,
            endpoint,
            direction,
            items: Vec::new(),
            has_more: false,
            next_cursor: None,
        }
        .into_records();
        assert_eq!(
            records,
            vec![TemporalRecord::HistoricalNeighborFooter {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                endpoint,
                direction,
                returned: 0,
                has_more: false,
                next_cursor: None,
            }]
        );
    }
}
