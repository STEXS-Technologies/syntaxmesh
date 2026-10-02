//! SQLite database and retained-checkpoint integrity validation.

use rusqlite::Connection;
use syntaxmesh_core::{GenerationHistoryEntry, GenerationId, GenerationManifest, GraphSnapshot};
use syntaxmesh_store::{
    BackendIntegrityCheck, BackendIntegrityKind, BackendIntegrityReport, StoreError,
};

use super::{CHECKPOINT_INTERVAL, SqliteGraphStore, decode, read_graph_at, sql_error, stable_id};

impl BackendIntegrityCheck for SqliteGraphStore {
    fn backend_integrity_check(&self) -> Result<BackendIntegrityReport, StoreError> {
        let findings = match sqlite_integrity_findings(&self.connection) {
            Ok(findings) => findings,
            Err(error) => vec![error.to_string()],
        };
        Ok(BackendIntegrityReport::from_database_findings(
            BackendIntegrityKind::SqliteDatabase,
            findings,
        ))
    }
}

fn sqlite_integrity_findings(connection: &Connection) -> Result<Vec<String>, StoreError> {
    let mut statement = connection
        .prepare("PRAGMA integrity_check")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    let mut findings = rows
        .map(|row| row.map_err(sql_error))
        .collect::<Result<Vec<_>, _>>()?;
    if findings == ["ok"] {
        findings.clear();
    }
    findings.extend(sqlite_checkpoint_findings(connection)?);
    if findings.is_empty() {
        findings.push("ok".to_owned());
    }
    Ok(findings)
}

fn sqlite_checkpoint_findings(connection: &Connection) -> Result<Vec<String>, StoreError> {
    let mut findings = Vec::new();
    let mut statement = connection
        .prepare(
            "SELECT checkpoint.sequence, checkpoint.generation, checkpoint.manifest, \
                    checkpoint.payload, history.payload \
             FROM syntaxmesh_graph_checkpoints AS checkpoint \
             LEFT JOIN syntaxmesh_generation_history AS history \
               ON history.sequence = checkpoint.sequence \
             ORDER BY checkpoint.sequence",
        )
        .map_err(sql_error)?;
    let checkpoints = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    drop(statement);

    for (sequence, generation_bytes, manifest_bytes, snapshot_bytes, history_bytes) in checkpoints {
        let location = format!("checkpoint sequence {sequence}");
        let Some(history_bytes) = history_bytes else {
            findings.push(format!(
                "{location}: no corresponding generation-history row"
            ));
            continue;
        };
        let entry = match decode::<GenerationHistoryEntry>(&history_bytes) {
            Ok(entry) => entry,
            Err(error) => {
                findings.push(format!("{location}: invalid history entry: {error}"));
                continue;
            }
        };
        let stored_generation = stable_id(&generation_bytes).map(GenerationId).ok();
        let checkpoint_manifest = decode::<GenerationManifest>(&manifest_bytes);
        let snapshot = decode::<GraphSnapshot>(&snapshot_bytes);
        if stored_generation != Some(entry.manifest.generation)
            || checkpoint_manifest.as_ref().ok() != Some(&entry.manifest)
        {
            findings.push(format!(
                "{location}: generation or manifest disagrees with history"
            ));
            continue;
        }
        let Ok(snapshot) = snapshot else {
            findings.push(format!(
                "{location}: checkpoint graph payload cannot be decoded"
            ));
            continue;
        };
        match read_graph_at(connection, entry.manifest.generation) {
            Ok(authoritative) if authoritative == snapshot => {}
            Ok(_) => findings.push(format!(
                "{location}: checkpoint graph differs from the persistent historical root"
            )),
            Err(error) => findings.push(format!(
                "{location}: persistent historical graph cannot validate checkpoint: {error}"
            )),
        }
    }

    let mut missing = connection
        .prepare(
            "SELECT history.sequence FROM syntaxmesh_generation_history AS history \
             LEFT JOIN syntaxmesh_graph_checkpoints AS checkpoint \
               ON checkpoint.sequence = history.sequence \
             WHERE (history.sequence = 1 OR history.sequence % ?1 = 0) \
               AND checkpoint.sequence IS NULL ORDER BY history.sequence",
        )
        .map_err(sql_error)?;
    let missing_sequences = missing
        .query_map([CHECKPOINT_INTERVAL], |row| row.get::<_, i64>(0))
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    findings.extend(
        missing_sequences.into_iter().map(|sequence| {
            format!("checkpoint sequence {sequence}: required checkpoint is missing")
        }),
    );
    let mut unexpected = connection
        .prepare(
            "SELECT sequence FROM syntaxmesh_graph_checkpoints \
             WHERE sequence < 1 OR (sequence != 1 AND sequence % ?1 != 0) \
             ORDER BY sequence",
        )
        .map_err(sql_error)?;
    let unexpected_sequences = unexpected
        .query_map([CHECKPOINT_INTERVAL], |row| row.get::<_, i64>(0))
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    findings.extend(unexpected_sequences.into_iter().map(|sequence| {
        format!("checkpoint sequence {sequence}: checkpoint is outside the configured cadence")
    }));
    Ok(findings)
}
