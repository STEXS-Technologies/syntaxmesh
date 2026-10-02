//! Runtime-neutral indexing contracts and orchestration.

mod indexer;

pub use indexer::{IndexError, Indexer, SourceSyntaxPolicy};
