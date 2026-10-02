//! Merge two sorted incidence streams with one retained page per direction.

use std::collections::VecDeque;

use syntaxmesh_api_model::HistoricalNeighborCursor;
use syntaxmesh_core::{Edge, EdgeDirection, NodeId};
use syntaxmesh_store::{GraphStore, MAX_HISTORICAL_EDGE_PAGE_SIZE};

use crate::{Query, QueryError};

struct EdgeCursor {
    direction: EdgeDirection,
    after: Option<HistoricalNeighborCursor>,
    buffered: VecDeque<Edge>,
    finished: bool,
}

impl EdgeCursor {
    const fn new(direction: EdgeDirection) -> Self {
        Self {
            direction,
            after: None,
            buffered: VecDeque::new(),
            finished: false,
        }
    }

    fn fill<S: GraphStore + ?Sized>(
        &mut self,
        query: &Query<'_, S>,
        endpoint: NodeId,
    ) -> Result<(), QueryError> {
        if self.buffered.is_empty() && !self.finished {
            let (page, cursor) = query.historical_edge_page(
                endpoint,
                self.direction,
                MAX_HISTORICAL_EDGE_PAGE_SIZE,
                self.after,
            )?;
            self.buffered = page.items.into();
            self.after = cursor;
            self.finished = cursor.is_none();
        }
        Ok(())
    }
}

pub(super) fn visit<S: GraphStore + ?Sized>(
    query: &Query<'_, S>,
    endpoint: NodeId,
    mut visitor: impl FnMut(Edge) -> Result<(), QueryError>,
) -> Result<(), QueryError> {
    let mut outgoing = EdgeCursor::new(EdgeDirection::Outgoing);
    let mut incoming = EdgeCursor::new(EdgeDirection::Incoming);
    loop {
        outgoing.fill(query, endpoint)?;
        incoming.fill(query, endpoint)?;
        let selected = match (outgoing.buffered.front(), incoming.buffered.front()) {
            (None, None) => break,
            (Some(_), None) => &mut outgoing,
            (None, Some(_)) => &mut incoming,
            (Some(left), Some(right)) if left.id <= right.id => &mut outgoing,
            (Some(_), Some(_)) => &mut incoming,
        };
        if let Some(edge) = selected.buffered.pop_front() {
            visitor(edge)?;
        }
    }
    Ok(())
}
