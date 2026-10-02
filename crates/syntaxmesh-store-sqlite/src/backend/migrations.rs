//! Ordered SQLite schema migration registry.
//!
//! Schema transitions remain implemented beside the SQLite persistence logic
//! because several legacy upgrades decode canonical Rust graph payloads. This
//! module owns the single ordered registry, mirroring Shardline's dedicated
//! migration manifest and keeping it out of the store's operational methods.

use rusqlite::Connection;
use std::path::Path;
use syntaxmesh_store::StoreError;

use super::migration_steps::{
    migrate_v1_to_v2, migrate_v2_to_v3, migrate_v3_to_v4, migrate_v4_to_v5, migrate_v5_to_v6,
    migrate_v6_to_v7, migrate_v7_to_v8, migrate_v8_to_v9, migrate_v9_to_v10, migrate_v10_to_v11,
    migrate_v11_to_v12, migrate_v12_to_v13, migrate_v13_to_v14, migrate_v14_to_v15,
    migrate_v15_to_v16, migrate_v16_to_v17, migrate_v17_to_v18, migrate_v18_to_v19,
    migrate_v19_to_v20, migrate_v20_to_v21,
};

#[derive(Debug)]
pub(super) struct SchemaMigration {
    pub(super) from: i64,
    pub(super) to: i64,
    pub(super) name: &'static str,
    pub(super) apply: fn(&mut Connection, &Path) -> Result<(), StoreError>,
    pub(super) checksum_sql: Option<&'static str>,
    /// SQL also required when the latest direct-bootstrap schema is installed.
    /// The bootstrap schema already includes most historical SQL changes, so
    /// only missing deltas are registered here rather than hard-coded in the
    /// open/migration lifecycle.
    pub(super) fresh_bootstrap_sql: Option<&'static str>,
    /// Reversal is defined only for additive SQL migrations whose data can be
    /// safely discarded; Rust payload migrations remain intentionally one-way.
    pub(super) rollback_sql: Option<&'static str>,
}

// One authoritative, strictly ordered upgrade path. Each apply function owns
// one SQLite transaction and advances syntaxmesh_schema atomically.
pub(super) const SCHEMA_MIGRATIONS: &[SchemaMigration] = &[
    SchemaMigration {
        from: 1,
        to: 2,
        name: "initialize_temporal_history",
        apply: migrate_v1_to_v2,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 2,
        to: 3,
        name: "materialize_fact_versions",
        apply: migrate_v2_to_v3,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 3,
        to: 4,
        name: "checkpoint_historical_graphs",
        apply: migrate_v3_to_v4,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 4,
        to: 5,
        name: "persistent_temporal_roots",
        apply: migrate_v4_to_v5,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 5,
        to: 6,
        name: "ordered_observation_time",
        apply: migrate_v5_to_v6,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 6,
        to: 7,
        name: "generation_acceptance_time",
        apply: migrate_v6_to_v7,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 7,
        to: 8,
        name: "observation_cursor_index",
        apply: migrate_v7_to_v8,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 8,
        to: 9,
        name: "indexed_change_events",
        apply: migrate_v8_to_v9,
        checksum_sql: Some(include_str!(
            "../../migrations/0009_indexed_change_events.up.sql"
        )),
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 9,
        to: 10,
        name: "schema_migration_ledger",
        apply: migrate_v9_to_v10,
        checksum_sql: Some(include_str!(
            "../../migrations/0010_schema_migration_ledger.up.sql"
        )),
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 10,
        to: 11,
        name: "sql_migration_checksums",
        apply: migrate_v10_to_v11,
        checksum_sql: Some(include_str!(
            "../../migrations/0011_sql_migration_checksums.up.sql"
        )),
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 11,
        to: 12,
        name: "explicit_startup_compatibility",
        apply: migrate_v11_to_v12,
        checksum_sql: Some(include_str!(
            "../../migrations/0012_explicit_startup_compatibility.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0012_explicit_startup_compatibility.up.sql"
        )),
        rollback_sql: None,
    },
    SchemaMigration {
        from: 12,
        to: 13,
        name: "explicit_change_set_lineage",
        apply: migrate_v12_to_v13,
        checksum_sql: Some(include_str!(
            "../../migrations/0013_explicit_change_set_lineage.up.sql"
        )),
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 13,
        to: 14,
        name: "evidence_consequence_edges",
        apply: migrate_v13_to_v14,
        checksum_sql: Some(include_str!(
            "../../migrations/0014_evidence_consequence_edges.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0014_evidence_consequence_edges.up.sql"
        )),
        rollback_sql: Some(include_str!(
            "../../migrations/0014_evidence_consequence_edges.down.sql"
        )),
    },
    SchemaMigration {
        from: 14,
        to: 15,
        name: "generation_acceptance_prefix",
        apply: migrate_v14_to_v15,
        checksum_sql: Some(include_str!(
            "../../migrations/0015_generation_acceptance_prefix.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0015_generation_acceptance_prefix.up.sql"
        )),
        rollback_sql: Some(include_str!(
            "../../migrations/0015_generation_acceptance_prefix.down.sql"
        )),
    },
    SchemaMigration {
        from: 15,
        to: 16,
        name: "indexed_node_kinds",
        apply: migrate_v15_to_v16,
        checksum_sql: Some(include_str!(
            "../../migrations/0016_indexed_node_kinds.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0016_indexed_node_kinds.up.sql"
        )),
        rollback_sql: None,
    },
    SchemaMigration {
        from: 16,
        to: 17,
        name: "versioned_module_resolution_diagnostics",
        apply: migrate_v16_to_v17,
        checksum_sql: None,
        fresh_bootstrap_sql: None,
        rollback_sql: None,
    },
    SchemaMigration {
        from: 17,
        to: 18,
        name: "remove_temporal_endpoint_indexes",
        apply: migrate_v17_to_v18,
        checksum_sql: Some(include_str!(
            "../../migrations/0018_remove_temporal_endpoint_indexes.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0018_remove_temporal_endpoint_indexes.up.sql"
        )),
        rollback_sql: Some(include_str!(
            "../../migrations/0018_remove_temporal_endpoint_indexes.down.sql"
        )),
    },
    SchemaMigration {
        from: 18,
        to: 19,
        name: "temporal_incidence_roots",
        apply: migrate_v18_to_v19,
        checksum_sql: Some(include_str!(
            "../../migrations/0019_temporal_incidence_roots.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0019_temporal_incidence_roots.up.sql"
        )),
        rollback_sql: Some(include_str!(
            "../../migrations/0019_temporal_incidence_roots.down.sql"
        )),
    },
    SchemaMigration {
        from: 19,
        to: 20,
        name: "fact_history_end_lookup",
        apply: migrate_v19_to_v20,
        checksum_sql: Some(include_str!(
            "../../migrations/0020_fact_history_end_lookup.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0020_fact_history_end_lookup.up.sql"
        )),
        rollback_sql: Some(include_str!(
            "../../migrations/0020_fact_history_end_lookup.down.sql"
        )),
    },
    SchemaMigration {
        from: 20,
        to: 21,
        name: "fact_history_generation_lookup",
        apply: migrate_v20_to_v21,
        checksum_sql: Some(include_str!(
            "../../migrations/0021_fact_history_generation_lookup.up.sql"
        )),
        fresh_bootstrap_sql: Some(include_str!(
            "../../migrations/0021_fact_history_generation_lookup.up.sql"
        )),
        rollback_sql: Some(include_str!(
            "../../migrations/0021_fact_history_generation_lookup.down.sql"
        )),
    },
];
