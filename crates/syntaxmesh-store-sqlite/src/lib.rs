//! SQLite reference and conformance backend for SyntaxMesh.

mod backend;

pub use backend::{SqliteGraphStore, SqliteMigrationStatus, SqliteMigrationStatusEntry};
