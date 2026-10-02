use std::error::Error;

pub(super) fn run() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(probe())
}

async fn probe() -> Result<(), Box<dyn Error>> {
    let database = turso::Builder::new_local(":memory:")
        .experimental_index_method(true)
        .build()
        .await?;
    let connection = database.connect()?;
    connection
        .execute(
            "CREATE TABLE search_probe (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
            (),
        )
        .await?;
    connection
        .execute(
            "CREATE INDEX search_probe_name_fts ON search_probe USING fts (name)",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO search_probe (id, name) VALUES (1, 'Scheduler overview')",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO search_probe (id, name) VALUES (2, 'History queries')",
            (),
        )
        .await?;

    let full_token_matches = matching_ids(&connection, "scheduler").await?;
    let partial_token_matches = matching_ids(&connection, "sched").await?;
    if full_token_matches != [1] || !partial_token_matches.is_empty() {
        return Err(format!(
            "unexpected Turso FTS results: full={full_token_matches:?}, partial={partial_token_matches:?}"
        )
        .into());
    }
    connection.execute(
        "INSERT INTO search_probe (id, name) VALUES (3, 'generateBlockDoc'), (4, 'execute_with_retry'), (5, 'generate block doc')", (),
    ).await?;
    let camel_matches = matching_ids(&connection, "block").await?;
    let snake_matches = matching_ids(&connection, "retry").await?;
    if camel_matches != [5] || snake_matches != [4] {
        return Err(format!(
            "identifier tokenization changed: block={camel_matches:?}; retry={snake_matches:?}"
        )
        .into());
    }
    println!("Identifier tokenization probe: block={camel_matches:?}; retry={snake_matches:?}");
    temporal_candidates(&connection).await?;
    println!(
        "Locked Turso FTS probe passed: token query matched {:?}; partial-token query matched {:?}; FTS does not preserve arbitrary substring semantics",
        full_token_matches, partial_token_matches
    );
    Ok(())
}

async fn temporal_candidates(connection: &turso::Connection) -> Result<(), Box<dyn Error>> {
    connection.execute(
        "CREATE TABLE temporal_probe (id INTEGER PRIMARY KEY, scope TEXT NOT NULL, valid_from INTEGER NOT NULL, valid_to INTEGER, name TEXT NOT NULL)", (),
    ).await?;
    connection
        .execute(
            "CREATE INDEX temporal_probe_fts ON temporal_probe USING fts (name)",
            (),
        )
        .await?;
    connection.execute(
        "INSERT INTO temporal_probe VALUES (1,'foreign',1,NULL,'generate block doc'),(2,'local',1,NULL,'generate block doc')", (),
    ).await?;
    connection.execute("BEGIN", ()).await?;
    connection
        .execute("UPDATE temporal_probe SET valid_to=2 WHERE id=2", ())
        .await?;
    connection
        .execute(
            "INSERT INTO temporal_probe VALUES (3,'local',2,NULL,'retry workflow')",
            (),
        )
        .await?;
    connection.execute("COMMIT", ()).await?;
    let historical = temporal_ids(connection, "block", 1).await?;
    let removed = temporal_ids(connection, "block", 2).await?;
    let introduced = temporal_ids(connection, "retry", 2).await?;
    let absent_before = temporal_ids(connection, "retry", 1).await?;
    if historical != [2] || !removed.is_empty() || introduced != [3] || !absent_before.is_empty() {
        return Err(format!("temporal FTS isolation failed: old={historical:?}, removed={removed:?}, new={introduced:?}, absent={absent_before:?}").into());
    }
    println!(
        "Temporal FTS probe passed: scope and validity filtered before LIMIT; retired labels remain searchable only in retained history"
    );
    let mut plan = connection.prepare(
        "EXPLAIN QUERY PLAN SELECT id FROM temporal_probe WHERE scope='local' AND valid_from<=1 AND (valid_to IS NULL OR valid_to>1) AND fts_match(name,'block') ORDER BY id LIMIT 1",
    ).await?;
    let mut plan_rows = plan.query(()).await?;
    while let Some(row) = plan_rows.next().await? {
        let detail: String = row.get(3)?;
        println!("Temporal FTS plan: {detail}");
    }
    for (generation, term, expected) in [(1, "block", 1), (2, "block", 0), (2, "retry", 1)] {
        let mut count = connection.prepare(
            "SELECT count(*) FROM temporal_probe WHERE scope='local' AND valid_from<=?2 AND (valid_to IS NULL OR valid_to>?2) AND fts_match(name,?1)",
        ).await?;
        let mut counts = count.query(turso::params![term, generation]).await?;
        let frequency: i64 = counts
            .next()
            .await?
            .ok_or("missing scoped frequency")?
            .get(0)?;
        if frequency != expected {
            return Err("scoped term frequency included foreign or retired rows".into());
        }
        println!("Scoped frequency: generation={generation} term={term} count={frequency}");
    }
    let score_before = historical_score(connection).await?;
    connection
        .execute(
            "INSERT INTO temporal_probe VALUES (4,'foreign',1,NULL,'unrelated vocabulary')",
            (),
        )
        .await?;
    let score_after = historical_score(connection).await?;
    println!(
        "Historical native score with unrelated foreign row: before={score_before}, after={score_after}"
    );
    Ok(())
}

async fn historical_score(connection: &turso::Connection) -> Result<f64, Box<dyn Error>> {
    let mut statement = connection.prepare(
        "SELECT fts_score(name,'block') FROM temporal_probe WHERE scope='local' AND valid_from<=1 AND (valid_to IS NULL OR valid_to>1) AND fts_match(name,'block')",
    ).await?;
    let mut rows = statement.query(()).await?;
    Ok(rows
        .next()
        .await?
        .ok_or("missing historical score")?
        .get(0)?)
}

async fn temporal_ids(
    connection: &turso::Connection,
    query: &str,
    generation: i64,
) -> Result<Vec<i64>, Box<dyn Error>> {
    let mut statement = connection.prepare(
        "SELECT id FROM temporal_probe WHERE scope='local' AND valid_from<=?2 AND (valid_to IS NULL OR valid_to>?2) AND fts_match(name,?1) ORDER BY id LIMIT 1",
    ).await?;
    let mut rows = statement.query(turso::params![query, generation]).await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        ids.push(row.get(0)?);
    }
    Ok(ids)
}

async fn matching_ids(
    connection: &turso::Connection,
    query: &str,
) -> Result<Vec<i64>, Box<dyn Error>> {
    let mut statement = connection
        .prepare("SELECT id FROM search_probe WHERE fts_match(name, ?1) ORDER BY id")
        .await?;
    let mut rows = statement.query([query]).await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        ids.push(row.get(0)?);
    }
    Ok(ids)
}
