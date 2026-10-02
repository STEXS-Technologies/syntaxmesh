use super::*;

pub(in crate::adapter) async fn visit_historical_nodes(
    connection: &turso::Connection,
    schema_cache: &super::super::history_schema_cache::HistorySchemaCache,
    generation: GenerationId,
    max_nodes: usize,
    visitor: &mut dyn FnMut(Node) -> Result<(), StoreError>,
) -> Result<usize, TursoStoreError> {
    if max_nodes == 0 {
        return Err(TursoStoreError::Store(StoreError::InvalidPageLimit));
    }
    let transaction = turso::transaction::Transaction::new_unchecked(
        connection,
        turso::transaction::TransactionBehavior::Deferred,
    )
    .await
    .map_err(|error| TursoStoreError::Backend(format!("begin historical node scan: {error}")))?;
    let result = scan(&transaction, schema_cache, generation, max_nodes, visitor).await;
    transaction.rollback().await.map_err(|error| {
        TursoStoreError::Backend(format!("rollback historical node scan: {error}"))
    })?;
    result
}

async fn scan(
    connection: &turso::Connection,
    schema_cache: &super::super::history_schema_cache::HistorySchemaCache,
    generation: GenerationId,
    max_nodes: usize,
    visitor: &mut dyn FnMut(Node) -> Result<(), StoreError>,
) -> Result<usize, TursoStoreError> {
    #[cfg(feature = "benchmark-instrumentation")]
    let mut profile = NodeReadProfile::new();
    #[cfg(feature = "benchmark-instrumentation")]
    let schema_started = profile.timer();
    let schema =
        super::super::history_schema_cache::read_schema(connection, schema_cache, generation)
            .await?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.record(ReadStage::Schema, schema_started);
    #[cfg(feature = "benchmark-instrumentation")]
    let root_started = profile.timer();
    let root = read_persistent_root(connection, generation).await?;
    #[cfg(feature = "benchmark-instrumentation")]
    profile.record(ReadStage::Root, root_started);
    let before_nodes = PersistentFactKey {
        fact_kind: u8::try_from(NODE_FACT - 1)
            .map_err(|error| TursoStoreError::Snapshot(error.to_string()))?,
        fact_id: StableId([u8::MAX; 32]),
    };
    let mut walker = syntaxmesh_store::PersistentFactTreeRangeWalker::new(root, Some(before_nodes));
    let mut cache = node_page_cache::NodePageCache::default();
    let mut statement = connection
        .prepare("SELECT payload FROM syntaxmesh_temporal_tree_pages WHERE page_id = ?1")
        .await
        .map_err(|error| TursoStoreError::Backend(format!("prepare node scan page: {error}")))?;
    let mut visited = 0_usize;
    let mut after = None;
    loop {
        match walker.next(cache.pages()) {
            Ok(Some(page)) => {
                if i64::from(page.key.fact_kind) != NODE_FACT {
                    break;
                }
                if visited == max_nodes {
                    return Err(TursoStoreError::Store(StoreError::InvalidPageLimit));
                }
                #[cfg(feature = "benchmark-instrumentation")]
                let decode_started = profile.timer();
                let node: Node = decode_tree_fact(page.value_payload(), schema)?;
                #[cfg(feature = "benchmark-instrumentation")]
                profile.record(ReadStage::DecodeNode, decode_started);
                if node.id.0 != page.key.fact_id
                    || after.is_some_and(|previous| node.id <= previous)
                {
                    return Err(TursoStoreError::Snapshot(
                        "node scan identity/order differs from tree".to_owned(),
                    ));
                }
                after = Some(node.id);
                visitor(node).map_err(TursoStoreError::Store)?;
                visited = visited.saturating_add(1);
                cache.yielded();
            }
            Ok(None) => break,
            Err(PersistentFactTreeError::MissingPage(id)) => {
                #[cfg(feature = "benchmark-instrumentation")]
                let sql_started = profile.timer();
                #[cfg(feature = "benchmark-instrumentation")]
                let mut pages = read_profiled_pages(&mut statement, id, &mut profile).await?;
                #[cfg(not(feature = "benchmark-instrumentation"))]
                let mut pages =
                    read_many_prepared::<PersistentFactNode>(&mut statement, [id.0.to_vec()])
                        .await?;
                let page = pages.pop().ok_or_else(|| {
                    TursoStoreError::Snapshot("node scan references missing page".to_owned())
                })?;
                #[cfg(feature = "benchmark-instrumentation")]
                profile.record(ReadStage::SqlPage, sql_started);
                #[cfg(feature = "benchmark-instrumentation")]
                let validation_started = profile.timer();
                cache
                    .insert_loaded(id, page)
                    .map_err(|error| persistent_tree_error(&error))?;
                #[cfg(feature = "benchmark-instrumentation")]
                profile.record(ReadStage::ValidatePage, validation_started);
            }
            Err(error) => return Err(persistent_tree_error(&error)),
        }
    }
    #[cfg(feature = "benchmark-instrumentation")]
    profile.finish(visited);
    Ok(visited)
}

#[cfg(feature = "benchmark-instrumentation")]
async fn read_profiled_pages(
    statement: &mut turso::Statement,
    id: StableId,
    profile: &mut NodeReadProfile,
) -> Result<Vec<PersistentFactNode>, TursoStoreError> {
    let query_started = profile.timer();
    let mut rows = statement
        .query([id.0.to_vec()])
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query Turso table: {error}")))?;
    profile.record(ReadStage::PageTransfer, query_started);
    let mut items = Vec::new();
    loop {
        let transfer_started = profile.timer();
        let row = rows
            .next()
            .await
            .map_err(|error| TursoStoreError::Backend(format!("read Turso row: {error}")))?;
        let Some(row) = row else {
            profile.record(ReadStage::PageTransfer, transfer_started);
            break;
        };
        let value = row
            .get_value(0)
            .map_err(|error| TursoStoreError::Backend(format!("read Turso payload: {error}")))?;
        profile.record(ReadStage::PageTransfer, transfer_started);
        match value {
            turso::Value::Blob(payload) => {
                let decode_started = profile.timer();
                items.push(bincode::deserialize(&payload).map_err(|error| {
                    TursoStoreError::Snapshot(format!("decode Turso row: {error}"))
                })?);
                profile.record(ReadStage::PageDecode, decode_started);
            }
            turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "Turso row payload is not a blob".to_owned(),
                ));
            }
        }
    }
    Ok(items)
}
