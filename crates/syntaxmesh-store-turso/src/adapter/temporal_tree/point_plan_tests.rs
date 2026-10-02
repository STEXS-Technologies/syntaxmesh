use super::*;

#[test]
fn historical_point_plan_seeks_identity_and_sequence() -> Result<(), TursoStoreError> {
    let directory =
        tempfile::tempdir().map_err(|error| TursoStoreError::Backend(error.to_string()))?;
    let store = crate::adapter::tests::open_migrated(directory.path().join("point-plan.db"))?;
    let plan = store.runtime.block_on(async {
        let mut rows = store
            .connection
            .query(
                &format!("EXPLAIN QUERY PLAN {HISTORICAL_POINT_QUERY}"),
                (NODE_FACT, vec![1_u8; 32], 1_i64),
            )
            .await
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?;
        let mut details = Vec::<String>::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|error| TursoStoreError::Backend(error.to_string()))?
        {
            details.push(
                row.get(3)
                    .map_err(|error| TursoStoreError::Backend(error.to_string()))?,
            );
        }
        Ok::<_, TursoStoreError>(details)
    })?;
    eprintln!("historical_point_plan={plan:?}");
    if plan
        .iter()
        .filter(|detail| {
            detail.contains("SEARCH")
                && (detail.contains("syntaxmesh_fact_versions_identity_time_idx")
                    || detail.contains("sqlite_autoindex_syntaxmesh_fact_versions_1"))
        })
        .count()
        < 2
    {
        return Err(TursoStoreError::Snapshot(format!(
            "historical point lookup does not seek identity/time twice: {plan:?}"
        )));
    }
    Ok(())
}
