//! Turso current-projection, reference, checkpoint, and retained-history validation.

use syntaxmesh_core::{
    Edge, GenerationHistoryEntry, GenerationId, GenerationManifest, GraphDelta, GraphSnapshot, Node,
};
use syntaxmesh_store::StoreError;

use super::{
    CHECKPOINT_INTERVAL, DeltaLookupIndex, TursoStoreError, decode_blob, node_exists_after_delta,
    provenance_exists, read_graph_at, read_many, read_persistent_snapshot, read_single,
    snapshot_root_for_schema, terminal_name,
};

pub(super) async fn turso_generation_history_findings(
    connection: &turso::Connection,
) -> Result<Vec<String>, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT sequence, generation, payload FROM syntaxmesh_generation_history ORDER BY sequence",
            (),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read generation history for integrity: {error}"))
        })?;
    let mut findings = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read generation history integrity row: {error}"))
    })? {
        let sequence = match row.get_value(0).map_err(|error| {
            TursoStoreError::Backend(format!("read history sequence for integrity: {error}"))
        })? {
            turso::Value::Integer(sequence) => sequence,
            value @ (turso::Value::Null
            | turso::Value::Real(_)
            | turso::Value::Text(_)
            | turso::Value::Blob(_)) => {
                findings.push(format!(
                    "generation history row: sequence is not an integer ({value:?})"
                ));
                continue;
            }
        };
        let location = format!("generation history sequence {sequence}");
        let generation = match row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read history generation for integrity: {error}"))
        })? {
            turso::Value::Blob(bytes) => bytes,
            value @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)) => {
                findings.push(format!("{location}: generation is not a blob ({value:?})"));
                continue;
            }
        };
        let payload = match row.get_value(2).map_err(|error| {
            TursoStoreError::Backend(format!("read history payload for integrity: {error}"))
        })? {
            turso::Value::Blob(bytes) => bytes,
            value @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)) => {
                findings.push(format!("{location}: payload is not a blob ({value:?})"));
                continue;
            }
        };
        match bincode::deserialize::<GenerationHistoryEntry>(&payload) {
            Ok(entry) if generation == entry.manifest.generation.0.0 => {}
            Ok(_) => findings.push(format!(
                "{location}: SQL generation differs from payload manifest"
            )),
            Err(error) => findings.push(format!("{location}: invalid history payload: {error}")),
        }
    }
    Ok(findings)
}

pub(super) async fn turso_integrity_findings(
    connection: &turso::Connection,
) -> Result<Vec<String>, TursoStoreError> {
    let mut rows = connection
        .query("PRAGMA integrity_check", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("run Turso integrity check: {error}")))?;
    let mut findings = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read Turso integrity result: {error}"))
    })? {
        match row.get_value(0).map_err(|error| {
            TursoStoreError::Backend(format!("decode Turso integrity result: {error}"))
        })? {
            turso::Value::Text(finding) => findings.push(finding),
            other @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Blob(_)) => {
                return Err(TursoStoreError::Backend(format!(
                    "Turso integrity result was not text: {other:?}"
                )));
            }
        }
    }
    if findings == ["ok"] {
        findings.clear();
    }
    findings.extend(turso_generation_history_findings(connection).await?);
    findings.extend(turso_checkpoint_findings(connection).await?);
    if findings.is_empty() {
        findings.push("ok".to_owned());
    }
    Ok(findings)
}

async fn turso_checkpoint_findings(
    connection: &turso::Connection,
) -> Result<Vec<String>, TursoStoreError> {
    let mut findings = Vec::new();
    let mut rows = connection
        .query(
            "SELECT checkpoint.sequence, checkpoint.generation, checkpoint.manifest, \
                    checkpoint.payload, history.payload \
             FROM syntaxmesh_graph_checkpoints AS checkpoint \
             LEFT JOIN syntaxmesh_generation_history AS history \
               ON history.sequence = checkpoint.sequence \
             ORDER BY checkpoint.sequence",
            (),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read graph checkpoints for integrity: {error}"))
        })?;
    let mut checkpoints = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read graph checkpoint for integrity: {error}"))
    })? {
        checkpoints.push((
            row.get::<i64>(0).map_err(|error| {
                TursoStoreError::Backend(format!("decode graph checkpoint sequence: {error}"))
            })?,
            row.get_value(1).map_err(|error| {
                TursoStoreError::Backend(format!("read graph checkpoint generation: {error}"))
            })?,
            row.get_value(2).map_err(|error| {
                TursoStoreError::Backend(format!("read graph checkpoint manifest: {error}"))
            })?,
            row.get_value(3).map_err(|error| {
                TursoStoreError::Backend(format!("read graph checkpoint payload: {error}"))
            })?,
            row.get_value(4).map_err(|error| {
                TursoStoreError::Backend(format!("read graph checkpoint history: {error}"))
            })?,
        ));
    }
    drop(rows);

    for (sequence, generation_value, manifest_value, snapshot_value, history_value) in checkpoints {
        let location = format!("checkpoint sequence {sequence}");
        let generation_bytes = match generation_value {
            turso::Value::Blob(bytes) => bytes,
            turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_) => {
                findings.push(format!("{location}: generation is not a blob"));
                continue;
            }
        };
        let history_bytes = match history_value {
            turso::Value::Blob(bytes) => bytes,
            turso::Value::Null => {
                findings.push(format!(
                    "{location}: no corresponding generation-history row"
                ));
                continue;
            }
            turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_) => {
                findings.push(format!("{location}: history payload is not a blob"));
                continue;
            }
        };
        let entry = match bincode::deserialize::<GenerationHistoryEntry>(&history_bytes) {
            Ok(entry) => entry,
            Err(error) => {
                findings.push(format!("{location}: invalid history entry: {error}"));
                continue;
            }
        };
        if generation_bytes.as_slice() != entry.manifest.generation.0.0.as_slice() {
            findings.push(format!("{location}: generation disagrees with history"));
            continue;
        }
        let checkpoint_manifest =
            decode_blob::<GenerationManifest>(manifest_value, "checkpoint manifest");
        if checkpoint_manifest.as_ref().ok() != Some(&entry.manifest) {
            findings.push(format!(
                "{location}: manifest disagrees with history or is invalid"
            ));
            continue;
        }
        let snapshot = match decode_blob::<GraphSnapshot>(snapshot_value, "checkpoint graph") {
            Ok(snapshot) => snapshot,
            Err(error) => {
                findings.push(format!("{location}: invalid graph payload: {error}"));
                continue;
            }
        };
        match read_graph_at(connection, entry.manifest.generation).await {
            Ok(authoritative) if authoritative == snapshot => {}
            Ok(_) => findings.push(format!(
                "{location}: checkpoint graph differs from the persistent historical root"
            )),
            Err(error) => findings.push(format!(
                "{location}: persistent historical graph cannot validate checkpoint: {error}"
            )),
        }
    }

    let missing = read_many::<i64>(
        connection,
        &format!(
            "SELECT history.sequence FROM syntaxmesh_generation_history AS history \
             LEFT JOIN syntaxmesh_graph_checkpoints AS checkpoint \
               ON checkpoint.sequence = history.sequence \
             WHERE (history.sequence = 1 OR history.sequence % {CHECKPOINT_INTERVAL} = 0) \
               AND checkpoint.sequence IS NULL ORDER BY history.sequence"
        ),
    )
    .await?;
    findings.extend(
        missing.into_iter().map(|sequence| {
            format!("checkpoint sequence {sequence}: required checkpoint is missing")
        }),
    );
    let unexpected = read_many::<i64>(
        connection,
        &format!(
            "SELECT sequence FROM syntaxmesh_graph_checkpoints \
             WHERE sequence < 1 OR (sequence != 1 AND sequence % {CHECKPOINT_INTERVAL} != 0) \
             ORDER BY sequence"
        ),
    )
    .await?;
    findings.extend(unexpected.into_iter().map(|sequence| {
        format!("checkpoint sequence {sequence}: checkpoint is outside the configured cadence")
    }));
    Ok(findings)
}

pub(super) async fn validate_index_columns(
    connection: &turso::Connection,
) -> Result<(), TursoStoreError> {
    let has_node_kind = connection
        .query("SELECT node_kind FROM syntaxmesh_nodes LIMIT 0", ())
        .await
        .is_ok();
    let node_sql = if has_node_kind {
        "SELECT id, name, owner_file, terminal_name, provenance, payload, node_kind FROM syntaxmesh_nodes ORDER BY id"
    } else {
        "SELECT id, name, owner_file, terminal_name, provenance, payload FROM syntaxmesh_nodes ORDER BY id"
    };
    let mut rows = connection
        .query(node_sql, ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("validate node indexes: {error}")))?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read node index row: {error}")))?
    {
        let id = row
            .get_value(0)
            .map_err(|error| TursoStoreError::Backend(format!("read indexed node ID: {error}")))?;
        let name = row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed node name: {error}"))
        })?;
        let owner_file = row.get_value(2).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed owner file: {error}"))
        })?;
        let terminal = row.get_value(3).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed terminal name: {error}"))
        })?;
        let provenance = row.get_value(4).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed node provenance: {error}"))
        })?;
        let payload = row.get_value(5).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed node payload: {error}"))
        })?;
        let node_kind = if has_node_kind {
            Some(row.get_value(6).map_err(|error| {
                TursoStoreError::Backend(format!("read indexed node kind: {error}"))
            })?)
        } else {
            None
        };
        let node: Node = decode_blob(payload, "node payload")?;
        let expected_owner = node.owner_file.map(|file| file.0.0.to_vec());
        let actual_owner = match owner_file {
            turso::Value::Null => None,
            turso::Value::Blob(value) => Some(value),
            turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "owner-file index is not a blob or null".to_owned(),
                ));
            }
        };
        if id != turso::Value::Blob(node.id.0.0.to_vec())
            || name != turso::Value::Text(node.name.clone())
            || actual_owner != expected_owner
            || terminal != turso::Value::Text(terminal_name(&node.name))
            || provenance != turso::Value::Blob(node.provenance.0.0.to_vec())
            || node_kind.is_some_and(|actual| {
                actual
                    != turso::Value::Integer(syntaxmesh_store::node_kind_storage_code(&node.kind))
            })
        {
            return Err(TursoStoreError::Snapshot(
                "node index columns disagree with canonical payload".to_owned(),
            ));
        }
    }
    let mut edge_rows = connection
        .query(
            "SELECT id, source, target, provenance, payload FROM syntaxmesh_edges ORDER BY id",
            (),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("validate edge indexes: {error}")))?;
    while let Some(row) = edge_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read edge index row: {error}")))?
    {
        let id = row
            .get_value(0)
            .map_err(|error| TursoStoreError::Backend(format!("read indexed edge ID: {error}")))?;
        let source = row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed edge source: {error}"))
        })?;
        let target = row.get_value(2).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed edge target: {error}"))
        })?;
        let provenance = row.get_value(3).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed edge provenance: {error}"))
        })?;
        let payload = row.get_value(4).map_err(|error| {
            TursoStoreError::Backend(format!("read indexed edge payload: {error}"))
        })?;
        let edge: Edge = decode_blob(payload, "edge payload")?;
        if id != turso::Value::Blob(edge.id.0.0.to_vec())
            || source != turso::Value::Blob(edge.source.0.0.to_vec())
            || target != turso::Value::Blob(edge.target.0.0.to_vec())
            || provenance != turso::Value::Blob(edge.provenance.0.0.to_vec())
        {
            return Err(TursoStoreError::Snapshot(
                "edge index columns disagree with canonical payload".to_owned(),
            ));
        }
    }
    Ok(())
}

pub(super) async fn validate_node_kind_column(
    connection: &turso::Connection,
) -> Result<(), TursoStoreError> {
    connection
        .query("SELECT node_kind FROM syntaxmesh_nodes LIMIT 0", ())
        .await
        .map_err(|error| {
            TursoStoreError::Snapshot(format!(
                "current Turso schema is missing the indexed node-kind projection: {error}"
            ))
        })?;
    let mut index_rows = connection
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'syntaxmesh_nodes_kind_idx' LIMIT 1",
            (),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("inspect node-kind index: {error}"))
        })?;
    if index_rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read node-kind index: {error}")))?
        .is_none()
    {
        return Err(TursoStoreError::Snapshot(
            "current Turso schema is missing syntaxmesh_nodes_kind_idx; run TursoGraphStore::migrate".to_owned(),
        ));
    }
    Ok(())
}

pub(super) async fn validate_canonical_state(
    connection: &turso::Connection,
) -> Result<(), TursoStoreError> {
    let manifest = read_single::<GenerationManifest>(
        connection,
        "SELECT payload FROM syntaxmesh_manifest WHERE id = 1",
    )
    .await?;
    if let Some(manifest) = manifest {
        validate_references_sql(connection).await?;
        let root = if manifest.schema_version == 1 {
            stream_graph_root(connection, manifest.generation).await?
        } else if manifest.schema_version == 2 {
            let projection = GraphSnapshot {
                files: read_many(
                    connection,
                    "SELECT payload FROM syntaxmesh_files ORDER BY id",
                )
                .await?,
                provenance: read_many(
                    connection,
                    "SELECT payload FROM syntaxmesh_provenance ORDER BY id",
                )
                .await?,
                nodes: read_many(
                    connection,
                    "SELECT payload FROM syntaxmesh_nodes ORDER BY id",
                )
                .await?,
                edges: read_many(
                    connection,
                    "SELECT payload FROM syntaxmesh_edges ORDER BY id",
                )
                .await?,
            };
            let tree = read_persistent_snapshot(connection, manifest.generation).await?;
            if projection != tree {
                return Err(TursoStoreError::Snapshot(
                    "canonical projection disagrees with persistent fact tree".to_owned(),
                ));
            }
            snapshot_root_for_schema(manifest.generation, &projection, manifest.schema_version)?
        } else {
            return Err(TursoStoreError::Snapshot(format!(
                "unsupported generation root schema version {}",
                manifest.schema_version
            )));
        };
        if root != manifest.graph_root {
            return Err(TursoStoreError::Snapshot(
                "canonical graph root does not match persisted manifest".to_owned(),
            ));
        }
    } else if has_graph_facts(connection).await? {
        return Err(TursoStoreError::Snapshot(
            "graph facts exist without a generation manifest".to_owned(),
        ));
    }
    Ok(())
}

async fn has_graph_facts(connection: &turso::Connection) -> Result<bool, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM syntaxmesh_files UNION ALL SELECT 1 FROM syntaxmesh_provenance UNION ALL SELECT 1 FROM syntaxmesh_nodes UNION ALL SELECT 1 FROM syntaxmesh_edges LIMIT 1",
            (),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("check unbound graph rows: {error}")))?;
    rows.next()
        .await
        .map(|row| row.is_some())
        .map_err(|error| TursoStoreError::Backend(format!("read unbound graph row: {error}")))
}

async fn validate_references_sql(connection: &turso::Connection) -> Result<(), TursoStoreError> {
    let checks = [
        (
            "SELECT n.id FROM syntaxmesh_nodes n LEFT JOIN syntaxmesh_provenance p ON p.id = n.provenance WHERE p.id IS NULL LIMIT 1",
            "node references missing provenance",
        ),
        (
            "SELECT e.id FROM syntaxmesh_edges e LEFT JOIN syntaxmesh_provenance p ON p.id = e.provenance LEFT JOIN syntaxmesh_nodes s ON s.id = e.source LEFT JOIN syntaxmesh_nodes t ON t.id = e.target WHERE p.id IS NULL OR s.id IS NULL OR t.id IS NULL LIMIT 1",
            "edge references missing provenance or endpoint",
        ),
    ];
    for (sql, message) in checks {
        let mut rows = connection.query(sql, ()).await.map_err(|error| {
            TursoStoreError::Backend(format!("validate graph references: {error}"))
        })?;
        if rows
            .next()
            .await
            .map_err(|error| {
                TursoStoreError::Backend(format!("read graph reference check: {error}"))
            })?
            .is_some()
        {
            return Err(TursoStoreError::Store(StoreError::Integrity(
                message.to_owned(),
            )));
        }
    }
    Ok(())
}

/// Validate only references introduced by the pending delta.
///
/// The current projection is validated on open and every accepted transition
/// preserves the invariant: provenance is append/upsert-only, and removing a
/// node deletes every incident edge. Checking changed facts therefore avoids
/// two whole-table joins on every publication without weakening the explicit
/// full audit performed during open and integrity checks.
pub(super) async fn validate_delta_references_sql(
    connection: &turso::Connection,
    delta: &GraphDelta,
) -> Result<(), TursoStoreError> {
    let lookup = DeltaLookupIndex::new(delta);
    for node in &delta.upsert_nodes {
        if !provenance_exists(connection, &lookup, node.provenance).await? {
            return Err(TursoStoreError::Store(StoreError::Integrity(format!(
                "node {:?} references missing provenance {:?}",
                node.id, node.provenance
            ))));
        }
    }
    for edge in &delta.upsert_edges {
        if !provenance_exists(connection, &lookup, edge.provenance).await?
            || !node_exists_after_delta(connection, &lookup, edge.source).await?
            || !node_exists_after_delta(connection, &lookup, edge.target).await?
        {
            return Err(TursoStoreError::Store(StoreError::Integrity(format!(
                "edge {:?} references missing provenance or endpoint",
                edge.id
            ))));
        }
    }
    Ok(())
}

async fn stream_graph_root(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<[u8; 32], TursoStoreError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&generation.0.0);
    let mut nodes = connection
        .query("SELECT payload FROM syntaxmesh_nodes ORDER BY id", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("stream graph nodes: {error}")))?;
    while let Some(row) = nodes
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read graph node: {error}")))?
    {
        let node: Node = decode_blob(
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(format!("read node payload: {error}")))?,
            "node payload",
        )?;
        hasher.update(&serde_json::to_vec(&node).map_err(|error| {
            TursoStoreError::Snapshot(format!("serialize node for graph root: {error}"))
        })?);
    }
    let mut edges = connection
        .query("SELECT payload FROM syntaxmesh_edges ORDER BY id", ())
        .await
        .map_err(|error| TursoStoreError::Backend(format!("stream graph edges: {error}")))?;
    while let Some(row) = edges
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read graph edge: {error}")))?
    {
        let edge: Edge = decode_blob(
            row.get_value(0)
                .map_err(|error| TursoStoreError::Backend(format!("read edge payload: {error}")))?,
            "edge payload",
        )?;
        hasher.update(&serde_json::to_vec(&edge).map_err(|error| {
            TursoStoreError::Snapshot(format!("serialize edge for graph root: {error}"))
        })?);
    }
    Ok(*hasher.finalize().as_bytes())
}
