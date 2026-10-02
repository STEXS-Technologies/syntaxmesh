//! Ordered Turso schema migration metadata.
//!
//! Typed backfills remain implemented beside the adapter because they decode
//! SyntaxMesh facts. This registry is the single migration order/status source.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SchemaMigration {
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) name: &'static str,
    pub(crate) checksum_sql: Option<&'static str>,
    pub(crate) kind: MigrationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MigrationKind {
    VersionOnly,
    V2ToV3,
    V3ToV4,
    V4ToV5,
    V6ToV7,
    V7ToV8,
    V8ToV9,
    V9ToV10,
    V10ToV11,
    V11ToV12,
    V12ToV13,
    V13ToV14,
    V14ToV15,
    V15ToV16,
    V16ToV17,
    V17ToV18,
    V18ToV19,
    V19ToV20,
    V20ToV21,
    V21ToV22,
    V22ToV23,
    V23ToV24,
}

pub(crate) const SCHEMA_MIGRATIONS: &[SchemaMigration] = &[
    SchemaMigration {
        from: 1,
        to: 2,
        name: "initial_schema_adoption",
        checksum_sql: None,
        kind: MigrationKind::VersionOnly,
    },
    SchemaMigration {
        from: 2,
        to: 3,
        name: "indexed_graph_columns",
        checksum_sql: None,
        kind: MigrationKind::V2ToV3,
    },
    SchemaMigration {
        from: 3,
        to: 4,
        name: "terminal_name_index",
        checksum_sql: None,
        kind: MigrationKind::V3ToV4,
    },
    SchemaMigration {
        from: 4,
        to: 5,
        name: "provenance_indexes",
        checksum_sql: None,
        kind: MigrationKind::V4ToV5,
    },
    SchemaMigration {
        from: 5,
        to: 6,
        name: "generation_history",
        checksum_sql: None,
        kind: MigrationKind::VersionOnly,
    },
    SchemaMigration {
        from: 6,
        to: 7,
        name: "temporal_fact_versions",
        checksum_sql: None,
        kind: MigrationKind::V6ToV7,
    },
    SchemaMigration {
        from: 7,
        to: 8,
        name: "graph_checkpoints",
        checksum_sql: None,
        kind: MigrationKind::V7ToV8,
    },
    SchemaMigration {
        from: 8,
        to: 9,
        name: "persistent_generation_roots",
        checksum_sql: None,
        kind: MigrationKind::V8ToV9,
    },
    SchemaMigration {
        from: 9,
        to: 10,
        name: "ordered_observation_time",
        checksum_sql: None,
        kind: MigrationKind::V9ToV10,
    },
    SchemaMigration {
        from: 10,
        to: 11,
        name: "generation_acceptance_time",
        checksum_sql: None,
        kind: MigrationKind::V10ToV11,
    },
    SchemaMigration {
        from: 11,
        to: 12,
        name: "observation_cursor_index",
        checksum_sql: None,
        kind: MigrationKind::V11ToV12,
    },
    SchemaMigration {
        from: 12,
        to: 13,
        name: "indexed_change_events",
        checksum_sql: None,
        kind: MigrationKind::V12ToV13,
    },
    SchemaMigration {
        from: 13,
        to: 14,
        name: "explicit_change_set_lineage",
        checksum_sql: None,
        kind: MigrationKind::V13ToV14,
    },
    SchemaMigration {
        from: 14,
        to: 15,
        name: "evidence_consequence_edges",
        checksum_sql: Some(include_str!(
            "../migrations/0015_evidence_consequence_edges.up.sql"
        )),
        kind: MigrationKind::V14ToV15,
    },
    SchemaMigration {
        from: 15,
        to: 16,
        name: "generation_acceptance_prefix",
        checksum_sql: Some(include_str!(
            "../migrations/0016_generation_acceptance_prefix.up.sql"
        )),
        kind: MigrationKind::V15ToV16,
    },
    SchemaMigration {
        from: 16,
        to: 17,
        name: "schema_migration_ledger",
        checksum_sql: Some(include_str!(
            "../migrations/0017_schema_migration_ledger.up.sql"
        )),
        kind: MigrationKind::V16ToV17,
    },
    SchemaMigration {
        from: 17,
        to: 18,
        name: "temporal_endpoint_indexes",
        checksum_sql: Some(include_str!(
            "../migrations/0018_temporal_endpoint_indexes.up.sql"
        )),
        kind: MigrationKind::V17ToV18,
    },
    SchemaMigration {
        from: 18,
        to: 19,
        name: "indexed_node_kinds",
        checksum_sql: Some(include_str!("../migrations/0019_indexed_node_kinds.up.sql")),
        kind: MigrationKind::V18ToV19,
    },
    SchemaMigration {
        from: 19,
        to: 20,
        name: "versioned_module_resolution_diagnostics",
        checksum_sql: None,
        kind: MigrationKind::V19ToV20,
    },
    SchemaMigration {
        from: 20,
        to: 21,
        name: "remove_temporal_endpoint_indexes",
        checksum_sql: Some(include_str!(
            "../migrations/0021_remove_temporal_endpoint_indexes.up.sql"
        )),
        kind: MigrationKind::V20ToV21,
    },
    SchemaMigration {
        from: 21,
        to: 22,
        name: "temporal_incidence_roots",
        checksum_sql: Some(include_str!(
            "../migrations/0022_temporal_incidence_roots.up.sql"
        )),
        kind: MigrationKind::V21ToV22,
    },
    SchemaMigration {
        from: 22,
        to: 23,
        name: "fact_history_end_lookup",
        checksum_sql: Some(include_str!(
            "../migrations/0023_fact_history_end_lookup.up.sql"
        )),
        kind: MigrationKind::V22ToV23,
    },
    SchemaMigration {
        from: 23,
        to: 24,
        name: "fact_history_generation_lookup",
        checksum_sql: Some(include_str!(
            "../migrations/0024_fact_history_generation_lookup.up.sql"
        )),
        kind: MigrationKind::V23ToV24,
    },
];
