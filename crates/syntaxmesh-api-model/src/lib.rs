//! Versioned public SyntaxMesh interchange records.

mod neighbor_cursor;
mod records;

pub use neighbor_cursor::{NeighborCursorError, decode_neighbor_cursor, encode_neighbor_cursor};
pub use records::*;
