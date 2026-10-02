//! Runtime-neutral GraphStore port implementation over Turso.

use super::*;
use syntaxmesh_core::FileVersion;

impl GraphStore for TursoGraphStore {
    fn current_generation(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationManifest>, StoreError> {
        Ok(self
            .database_manifest()?
            .filter(|manifest| manifest.repository == repository && manifest.worktree == worktree))
    }

    fn manifest(&self, generation: GenerationId) -> Result<GenerationManifest, StoreError> {
        if let Some(current) = self.database_manifest()?
            && current.generation == generation
        {
            return Ok(current);
        }
        self.runtime
            .block_on(read_many_with_params::<GenerationHistoryEntry>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
                [generation.0.0.to_vec()],
            ))
            .map_err(Self::map_backend)?
            .into_iter()
            .next()
            .map(|entry| entry.manifest)
            .ok_or(StoreError::StaleBase {
                expected: Some(generation),
                actual: self
                    .database_manifest()?
                    .map(|manifest| manifest.generation),
            })
    }

    fn generation_history(&self) -> Result<Vec<GenerationHistoryEntry>, StoreError> {
        self.runtime
            .block_on(read_many::<GenerationHistoryEntry>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_generation_history ORDER BY sequence",
            ))
            .map_err(Self::map_backend)
    }

    fn current_generation_entry(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationHistoryEntry>, StoreError> {
        let Some(current) = self.current_generation(repository, worktree)? else {
            return Ok(None);
        };
        self.runtime
            .block_on(read_many_with_params::<GenerationHistoryEntry>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
                [current.generation.0.0.to_vec()],
            ))
            .map_err(Self::map_backend)
            .map(|entries| entries.into_iter().next())
    }

    fn generation_lineage_history(&self) -> Result<Vec<GenerationLineageEntry>, StoreError> {
        self.runtime
            .block_on(read_many::<GenerationLineageEntry>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_generation_lineage ORDER BY sequence",
            ))
            .map_err(Self::map_backend)
    }

    fn generation_consequence_history(
        &self,
    ) -> Result<Vec<GenerationConsequenceEntry>, StoreError> {
        self.runtime
            .block_on(read_many::<GenerationConsequenceEntry>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_generation_consequences ORDER BY sequence",
            ))
            .map_err(Self::map_backend)
    }

    fn generation_sequence(&self, generation: GenerationId) -> Result<u64, StoreError> {
        self.runtime
            .block_on(read_generation_sequence(&self.connection, generation))
            .map_err(Self::map_backend)
    }

    fn change_events_for_fact(
        &self,
        fact: FactRef,
        after: Option<ChangeEventCursor>,
        limit: usize,
    ) -> Result<ChangeEventPage, StoreError> {
        self.runtime
            .block_on(read_change_events_for_fact(
                &self.connection,
                fact,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn change_event(&self, generation: GenerationId) -> Result<Option<ChangeEvent>, StoreError> {
        self.runtime
            .block_on(read_change_event(&self.connection, generation))
            .map_err(Self::map_backend)
    }

    fn events_for_change_set(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
        after: Option<EventsForChangeSetCursor>,
        limit: usize,
    ) -> Result<EventsForChangeSetPage, StoreError> {
        self.runtime
            .block_on(read_events_for_change_set(
                &self.connection,
                change_set,
                as_of,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn consequence_edges_for_endpoint(
        &self,
        endpoint: LineageEndpoint,
        as_of: GenerationId,
        after: Option<ConsequenceEdgeCursor>,
        limit: usize,
    ) -> Result<ConsequenceEdgePage, StoreError> {
        self.runtime
            .block_on(read_consequence_edges_for_endpoint(
                &self.connection,
                endpoint,
                as_of,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn consequence_edges_for_endpoint_range(
        &self,
        endpoint: LineageEndpoint,
        from_generation: GenerationId,
        until_generation: GenerationId,
        after: Option<ConsequenceRangeCursor>,
        limit: usize,
    ) -> Result<ConsequenceRangePage, StoreError> {
        self.runtime
            .block_on(read_consequence_edges_for_endpoint_range(
                &self.connection,
                endpoint,
                from_generation,
                until_generation,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn change_set_at(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
    ) -> Result<Option<ChangeSetVersion>, StoreError> {
        self.runtime
            .block_on(read_change_set_at(&self.connection, change_set, as_of))
            .map_err(Self::map_backend)
    }

    fn acceptance_time(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError> {
        self.runtime
            .block_on(read_acceptance_time(&self.connection, generation))
            .map_err(Self::map_backend)
    }

    fn accepted_through(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError> {
        self.runtime
            .block_on(read_accepted_through(&self.connection, generation))
            .map_err(Self::map_backend)
    }

    fn accepted_generations_between(
        &self,
        from_inclusive: AcceptanceTime,
        until_exclusive: AcceptanceTime,
        after: Option<AcceptedGenerationCursor>,
        limit: usize,
    ) -> Result<AcceptedGenerationPage, StoreError> {
        if from_inclusive >= until_exclusive {
            return Err(StoreError::InvalidTemporalRange);
        }
        if limit == 0 {
            return Ok(AcceptedGenerationPage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        self.runtime
            .block_on(read_accepted_generations_between(
                &self.connection,
                from_inclusive,
                until_exclusive,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn node_history(&self, id: NodeId) -> Result<Vec<NodeHistoryVersion>, StoreError> {
        self.runtime
            .block_on(read_node_history(&self.connection, id))
            .map_err(Self::map_backend)
    }

    fn fact_history(&self, fact: FactRef) -> Result<Vec<FactHistoryVersion>, StoreError> {
        self.runtime
            .block_on(read_fact_history(&self.connection, fact))
            .map_err(Self::map_backend)
    }

    fn fact_versions_changed_at(
        &self,
        fact: FactRef,
        generation: GenerationId,
    ) -> Result<Vec<FactHistoryEntry>, StoreError> {
        self.runtime
            .block_on(read_fact_versions_changed_at(
                &self.connection,
                fact,
                generation,
            ))
            .map_err(Self::map_backend)
    }

    fn fact_version_changes_at_page(
        &self,
        generation: GenerationId,
        after: Option<FactVersionChangeCursor>,
        limit: usize,
    ) -> Result<FactVersionChangePage, StoreError> {
        self.runtime
            .block_on(read_fact_version_changes_at_page(
                &self.connection,
                generation,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn fact_history_page(
        &self,
        as_of_generation: GenerationId,
        after: Option<FactHistoryCursor>,
        limit: usize,
    ) -> Result<FactHistoryPage, StoreError> {
        self.runtime
            .block_on(read_fact_history_page(
                &self.connection,
                as_of_generation,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn observed_facts_between(
        &self,
        from_inclusive: ObservationTime,
        until_exclusive: ObservationTime,
        after: Option<ObservedFactCursor>,
        limit: usize,
    ) -> Result<ObservedFactPage, StoreError> {
        self.runtime
            .block_on(read_observed_facts_between(
                &self.connection,
                from_inclusive,
                until_exclusive,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn changes_between(
        &self,
        from: GenerationId,
        to: GenerationId,
    ) -> Result<Vec<GenerationChange>, StoreError> {
        self.runtime
            .block_on(read_changes_between(&self.connection, from, to))
            .map_err(Self::map_backend)
    }

    fn changes_between_page(
        &self,
        from: GenerationId,
        to: GenerationId,
        after: Option<GenerationChangeCursor>,
        limit: usize,
    ) -> Result<GenerationChangePage, StoreError> {
        self.runtime
            .block_on(read_changes_between_page(
                &self.connection,
                from,
                to,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn historical_snapshot(&self, generation: GenerationId) -> Result<GraphSnapshot, StoreError> {
        self.runtime
            .block_on(read_graph_at(&self.connection, generation))
            .map_err(Self::map_backend)
    }

    fn historical_node(
        &self,
        generation: GenerationId,
        id: NodeId,
    ) -> Result<Option<Node>, StoreError> {
        self.runtime
            .block_on(read_temporal_node(&self.connection, generation, id))
            .map_err(Self::map_backend)
    }

    fn historical_nodes_by_ids(
        &self,
        generation: GenerationId,
        ids: &[NodeId],
    ) -> Result<Vec<Node>, StoreError> {
        if ids.len() > 256 {
            return Err(StoreError::InvalidPageLimit);
        }
        self.runtime
            .block_on(super::temporal_tree::read_temporal_nodes_by_ids(
                &self.connection,
                generation,
                ids,
            ))
            .map_err(Self::map_backend)
    }

    fn historical_file(
        &self,
        generation: GenerationId,
        id: FileId,
    ) -> Result<Option<FileVersion>, StoreError> {
        self.runtime
            .block_on(super::temporal_tree::read_temporal_evidence(
                &self.connection,
                generation,
                FILE_FACT,
                id.0,
            ))
            .map_err(Self::map_backend)
    }

    fn historical_provenance(
        &self,
        generation: GenerationId,
        id: ProvenanceId,
    ) -> Result<Option<Provenance>, StoreError> {
        self.runtime
            .block_on(super::temporal_tree::read_temporal_evidence(
                &self.connection,
                generation,
                PROVENANCE_FACT,
                id.0,
            ))
            .map_err(Self::map_backend)
    }

    fn historical_search_nodes(
        &self,
        generation: GenerationId,
        text: &str,
        limit: usize,
    ) -> Result<Vec<Node>, StoreError> {
        self.runtime
            .block_on(super::temporal_tree::historical_search_nodes(
                &self.connection,
                &self.history_schema_cache,
                generation,
                text,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn historical_files_page(
        &self,
        generation: GenerationId,
        after: Option<syntaxmesh_core::FileId>,
        limit: usize,
    ) -> Result<syntaxmesh_store::HistoricalFilePage, StoreError> {
        self.runtime
            .block_on(super::file_pages::historical_files_page(
                &self.connection,
                generation,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn historical_nodes_page(
        &self,
        generation: GenerationId,
        after: Option<NodeId>,
        limit: usize,
    ) -> Result<syntaxmesh_store::HistoricalNodePage, StoreError> {
        self.runtime
            .block_on(super::temporal_tree::historical_nodes_page(
                &self.connection,
                &self.history_schema_cache,
                generation,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn visit_historical_nodes(
        &self,
        generation: GenerationId,
        max_nodes: usize,
        visitor: &mut dyn FnMut(Node) -> Result<(), StoreError>,
    ) -> Result<usize, StoreError> {
        self.runtime
            .block_on(super::temporal_tree::visit_historical_nodes(
                &self.connection,
                &self.history_schema_cache,
                generation,
                max_nodes,
                visitor,
            ))
            .map_err(Self::map_backend)
    }

    fn historical_incident_edges(
        &self,
        generation: GenerationId,
        endpoint: NodeId,
        direction: EdgeDirection,
        after: Option<EdgeId>,
        limit: usize,
    ) -> Result<HistoricalEdgePage, StoreError> {
        self.runtime
            .block_on(historical_incident_edge_page(
                &self.connection,
                generation,
                endpoint,
                direction,
                after,
                limit,
            ))
            .map_err(Self::map_backend)
    }

    fn apply_delta(&mut self, delta: GraphDelta) -> Result<GenerationManifest, StoreError> {
        self.apply_delta_with_acceptance_time(delta, None)
    }

    fn apply_delta_with_acceptance_time(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        self.apply_database_delta(delta, accepted_at)
            .map_err(Self::map_backend)
    }

    fn apply_delta_with_lineage(
        &mut self,
        delta: GraphDeltaWithLineage,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        self.apply_database_delta_with_lineage(delta.graph, accepted_at, delta.lineage)
            .map_err(Self::map_backend)
    }

    fn apply_delta_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        request
            .validate()
            .map_err(|error| StoreError::InvalidDelta(error.to_string()))?;
        self.apply_database_delta_with_consequences(
            request.publication.graph,
            accepted_at,
            request.publication.lineage,
            request.consequences,
        )
        .map_err(Self::map_backend)
    }

    fn set_generation_status(
        &mut self,
        generation: GenerationId,
        status: GenerationStatus,
    ) -> Result<GenerationManifest, StoreError> {
        let expected = self.last_manifest.clone();
        let manifest = self
            .runtime
            .block_on(set_database_status(
                &mut self.connection,
                generation,
                status,
                expected,
            ))
            .map_err(Self::map_backend)?;
        self.last_manifest = Some(manifest.clone());
        Ok(manifest)
    }

    fn node(&self, generation: GenerationId, id: NodeId) -> Result<Option<Node>, StoreError> {
        self.require_database_generation(generation)?;
        let node = self
            .runtime
            .block_on(async {
                Ok(read_many_with_params::<Node>(
                    &self.connection,
                    "SELECT payload FROM syntaxmesh_nodes WHERE id = ?1",
                    [id.0.0.to_vec()],
                )
                .await?
                .into_iter()
                .next())
            })
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(node)
    }

    fn nodes(&self, generation: GenerationId) -> Result<Vec<Node>, StoreError> {
        self.require_database_generation(generation)?;
        let nodes = self
            .runtime
            .block_on(read_many::<Node>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_nodes ORDER BY id",
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(nodes)
    }

    fn module_resolution_nodes(&self, generation: GenerationId) -> Result<Vec<Node>, StoreError> {
        self.require_database_generation(generation)?;
        let nodes = self
            .runtime
            .block_on(read_many_with_params::<Node>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_nodes WHERE node_kind IN (?1, ?2, ?3, ?4) ORDER BY id",
                (
                    syntaxmesh_store::NODE_KIND_CODE_MODULE,
                    syntaxmesh_store::NODE_KIND_CODE_IMPORT,
                    syntaxmesh_store::NODE_KIND_CODE_EXPORT,
                    syntaxmesh_store::NODE_KIND_CODE_MODULE_RESOLUTION_DIAGNOSTIC,
                ),
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(nodes)
    }

    fn search_nodes(
        &self,
        generation: GenerationId,
        text: &str,
        limit: usize,
    ) -> Result<Vec<Node>, StoreError> {
        self.require_database_generation(generation)?;
        let limit = i64::try_from(limit)
            .map_err(|error| StoreError::Backend(format!("search limit is too large: {error}")))?;
        let nodes = self.runtime
            .block_on(read_many_with_params::<Node>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_nodes WHERE instr(lower(name), lower(?1)) > 0 ORDER BY id LIMIT ?2",
                (text, limit),
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(nodes)
    }

    fn symbol_candidates(
        &self,
        generation: GenerationId,
        exact_names: &[String],
        terminal_names: &[String],
    ) -> Result<Vec<Node>, StoreError> {
        self.require_database_generation(generation)?;
        let nodes = self
            .runtime
            .block_on(async {
                let mut candidates = BTreeMap::new();
                for name in exact_names {
                    for node in read_many_with_params::<Node>(
                        &self.connection,
                        "SELECT payload FROM syntaxmesh_nodes WHERE name = ?1 ORDER BY id",
                        [name.as_str()],
                    )
                    .await?
                    {
                        candidates.insert(node.id, node);
                    }
                }
                for name in terminal_names {
                    for node in read_many_with_params::<Node>(
                        &self.connection,
                        "SELECT payload FROM syntaxmesh_nodes WHERE terminal_name = ?1 ORDER BY id",
                        [name.as_str()],
                    )
                    .await?
                    {
                        candidates.insert(node.id, node);
                    }
                }
                Ok::<_, TursoStoreError>(candidates.into_values().collect())
            })
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(nodes)
    }

    fn files(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::FileVersion>, StoreError> {
        self.require_database_generation(generation)?;
        let files = self
            .runtime
            .block_on(read_many::<syntaxmesh_core::FileVersion>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_files ORDER BY id",
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(files)
    }

    fn edges(&self, generation: GenerationId) -> Result<Vec<Edge>, StoreError> {
        self.require_database_generation(generation)?;
        let edges = self
            .runtime
            .block_on(read_many::<Edge>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_edges ORDER BY id",
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(edges)
    }

    fn provenance(&self, generation: GenerationId) -> Result<Vec<Provenance>, StoreError> {
        self.require_database_generation(generation)?;
        let provenance = self
            .runtime
            .block_on(read_many::<Provenance>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_provenance ORDER BY id",
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(provenance)
    }

    fn provenance_for_ids(
        &self,
        generation: GenerationId,
        ids: &[ProvenanceId],
    ) -> Result<Vec<Provenance>, StoreError> {
        self.manifest(generation)?;
        let mut unique_ids = ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        let mut records = Vec::new();
        for batch in unique_ids.chunks(400) {
            let placeholders = (1..=batch.len())
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT payload FROM syntaxmesh_provenance WHERE id IN ({placeholders}) ORDER BY id"
            );
            let values = batch
                .iter()
                .map(|id| turso::Value::Blob(id.0.0.to_vec()))
                .collect::<Vec<_>>();
            records.extend(
                self.runtime
                    .block_on(read_many_with_params::<Provenance>(
                        &self.connection,
                        &sql,
                        turso::params_from_iter(values),
                    ))
                    .map_err(Self::map_backend)?,
            );
        }
        records.sort_unstable_by_key(|provenance| provenance.id);
        Ok(records)
    }

    fn outgoing(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        self.require_database_generation(generation)?;
        let edges = self
            .runtime
            .block_on(read_many_with_params::<Edge>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_edges WHERE source = ?1 ORDER BY id",
                [id.0.0.to_vec()],
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(edges)
    }

    fn incoming(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        self.require_database_generation(generation)?;
        let edges = self
            .runtime
            .block_on(read_many_with_params::<Edge>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_edges WHERE target = ?1 ORDER BY id",
                [id.0.0.to_vec()],
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(edges)
    }

    fn nodes_for_file(
        &self,
        generation: GenerationId,
        file: syntaxmesh_core::FileId,
    ) -> Result<Vec<NodeId>, StoreError> {
        self.require_database_generation(generation)?;
        let nodes = self
            .runtime
            .block_on(read_many_with_params::<Node>(
                &self.connection,
                "SELECT payload FROM syntaxmesh_nodes WHERE owner_file = ?1 ORDER BY id",
                [file.0.0.to_vec()],
            ))
            .map_err(Self::map_backend)?;
        self.require_database_generation(generation)?;
        Ok(nodes.into_iter().map(|node| node.id).collect())
    }
}

pub(super) async fn history_sequence_for_generation(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<i64, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("locate historical generation: {error}"))
        })?;
    let row = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read historical generation: {error}")))?
        .ok_or({
            TursoStoreError::Store(StoreError::StaleBase {
                expected: Some(generation),
                actual: None,
            })
        })?;
    match row
        .get_value(0)
        .map_err(|error| TursoStoreError::Backend(format!("read historical sequence: {error}")))?
    {
        turso::Value::Integer(value) => Ok(value),
        turso::Value::Null
        | turso::Value::Real(_)
        | turso::Value::Text(_)
        | turso::Value::Blob(_) => Err(TursoStoreError::Snapshot(
            "historical sequence is not an integer".to_owned(),
        )),
    }
}

async fn read_node_history(
    connection: &turso::Connection,
    id: NodeId,
) -> Result<Vec<NodeHistoryVersion>, TursoStoreError> {
    let mut rows = connection.query(
        "SELECT start_entry.payload, end_entry.payload, versions.payload FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 ORDER BY versions.valid_from_sequence",
        (NODE_FACT, id.0.0.to_vec()),
    ).await.map_err(|error| TursoStoreError::Backend(format!("read node history: {error}")))?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read node history row: {error}")))?
    {
        let start: GenerationHistoryEntry = decode_blob(
            row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("read node history start: {error}"))
            })?,
            "node history start",
        )?;
        let until = match row
            .get_value(1)
            .map_err(|error| TursoStoreError::Backend(format!("read node history end: {error}")))?
        {
            turso::Value::Blob(payload) => Some(
                bincode::deserialize::<GenerationHistoryEntry>(&payload)
                    .map_err(|error| {
                        TursoStoreError::Snapshot(format!("decode node history end: {error}"))
                    })?
                    .manifest
                    .generation,
            ),
            turso::Value::Null => None,
            turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "node history end is not a blob or null".to_owned(),
                ));
            }
        };
        let node: Node = decode_blob(
            row.get_value(2).map_err(|error| {
                TursoStoreError::Backend(format!("read node history payload: {error}"))
            })?,
            "node history payload",
        )?;
        result.push(NodeHistoryVersion {
            valid_from: start.manifest.generation,
            valid_until: until,
            node,
        });
    }
    Ok(result)
}

async fn read_fact_history(
    connection: &turso::Connection,
    fact: FactRef,
) -> Result<Vec<FactHistoryVersion>, TursoStoreError> {
    let (kind, id) = match fact {
        FactRef::File(id) => (FILE_FACT, id.0.0.to_vec()),
        FactRef::Provenance(id) => (PROVENANCE_FACT, id.0.0.to_vec()),
        FactRef::Node(id) => (NODE_FACT, id.0.0.to_vec()),
        FactRef::Edge(id) => (EDGE_FACT, id.0.0.to_vec()),
    };
    let mut rows = connection.query(
        "SELECT start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 ORDER BY versions.valid_from_sequence",
        (kind, id),
    ).await.map_err(|error| TursoStoreError::Backend(format!("read fact history: {error}")))?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read fact history row: {error}")))?
    {
        let start: GenerationHistoryEntry = decode_blob(
            row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("read fact history start: {error}"))
            })?,
            "fact history start",
        )?;
        let valid_until = match row
            .get_value(1)
            .map_err(|error| TursoStoreError::Backend(format!("read fact history end: {error}")))?
        {
            turso::Value::Blob(payload) => Some(
                bincode::deserialize::<GenerationHistoryEntry>(&payload)
                    .map_err(|error| {
                        TursoStoreError::Snapshot(format!("decode fact history end: {error}"))
                    })?
                    .manifest
                    .generation,
            ),
            turso::Value::Null => None,
            turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "fact history end is not a blob or null".to_owned(),
                ));
            }
        };
        let payload_bytes = row.get_value(2).map_err(|error| {
            TursoStoreError::Backend(format!("read fact history payload: {error}"))
        })?;
        let observed_at = read_optional_time_value(
            row.get_value(3).map_err(|error| {
                TursoStoreError::Backend(format!("read fact observation time: {error}"))
            })?,
            "observation time",
        )?
        .map(ObservationTime);
        let accepted_at = read_optional_time_value(
            row.get_value(4).map_err(|error| {
                TursoStoreError::Backend(format!("read fact acceptance time: {error}"))
            })?,
            "acceptance time",
        )?
        .map(AcceptanceTime);
        let payload = match fact {
            FactRef::File(_) => {
                FactPayload::File(decode_blob(payload_bytes, "file history payload")?)
            }
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode_blob(payload_bytes, "provenance history payload")?)
            }
            FactRef::Node(_) => {
                FactPayload::Node(decode_blob(payload_bytes, "node history payload")?)
            }
            FactRef::Edge(_) => {
                FactPayload::Edge(decode_blob(payload_bytes, "edge history payload")?)
            }
        };
        result.push(FactHistoryVersion {
            valid_from: start.manifest.generation,
            valid_until,
            observed_at,
            accepted_at,
            payload,
        });
    }
    Ok(result)
}

async fn read_fact_versions_changed_at(
    connection: &turso::Connection,
    fact: FactRef,
    generation: GenerationId,
) -> Result<Vec<FactHistoryEntry>, TursoStoreError> {
    let changed_sequence = history_sequence_for_generation(connection, generation).await?;
    let (kind, id) = syntaxmesh_store::fact_storage_key(fact);
    let mut rows = connection
        .query(
            "SELECT versions.valid_from_sequence, versions.valid_until_sequence, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_time_idx JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 AND versions.valid_from_sequence = ?3 UNION ALL SELECT versions.valid_from_sequence, versions.valid_until_sequence, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions INDEXED BY syntaxmesh_fact_versions_identity_end_idx JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.fact_kind = ?1 AND versions.fact_id = ?2 AND versions.valid_until_sequence = ?3 AND versions.valid_from_sequence <> ?3 ORDER BY valid_from_sequence",
            (kind, id, changed_sequence),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read fact versions changed at generation: {error}"))
        })?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read changed fact version row: {error}"))
    })? {
        let valid_from_sequence = row.get::<i64>(0).map_err(|error| {
            TursoStoreError::Backend(format!("read fact start sequence: {error}"))
        })?;
        let valid_until_sequence = match row
            .get_value(1)
            .map_err(|error| TursoStoreError::Backend(format!("read fact end sequence: {error}")))?
        {
            turso::Value::Integer(raw_end_sequence) => {
                Some(u64::try_from(raw_end_sequence).map_err(|error| {
                    TursoStoreError::Snapshot(format!("invalid fact end sequence: {error}"))
                })?)
            }
            turso::Value::Null => None,
            other @ (turso::Value::Real(_) | turso::Value::Text(_) | turso::Value::Blob(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "fact end sequence has unexpected type: {other:?}"
                )));
            }
        };
        let start: GenerationHistoryEntry = decode_blob(
            row.get_value(2).map_err(|error| {
                TursoStoreError::Backend(format!("read changed fact start generation: {error}"))
            })?,
            "changed fact start generation",
        )?;
        let valid_until = match row.get_value(3).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact end generation: {error}"))
        })? {
            turso::Value::Blob(payload) => Some(
                bincode::deserialize::<GenerationHistoryEntry>(&payload)
                    .map_err(|error| {
                        TursoStoreError::Snapshot(format!("decode changed fact end: {error}"))
                    })?
                    .manifest
                    .generation,
            ),
            turso::Value::Null => None,
            other @ (turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "changed fact end generation has unexpected type: {other:?}"
                )));
            }
        };
        let payload_bytes = row.get_value(4).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact payload: {error}"))
        })?;
        let payload = match fact {
            FactRef::File(_) => FactPayload::File(decode_blob(payload_bytes, "file payload")?),
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode_blob(payload_bytes, "provenance payload")?)
            }
            FactRef::Node(_) => FactPayload::Node(decode_blob(payload_bytes, "node payload")?),
            FactRef::Edge(_) => FactPayload::Edge(decode_blob(payload_bytes, "edge payload")?),
        };
        let observed_at = read_optional_time_value(
            row.get_value(5).map_err(|error| {
                TursoStoreError::Backend(format!("read changed fact observation time: {error}"))
            })?,
            "observation time",
        )?
        .map(ObservationTime);
        let accepted_at = read_optional_time_value(
            row.get_value(6).map_err(|error| {
                TursoStoreError::Backend(format!("read changed fact acceptance time: {error}"))
            })?,
            "acceptance time",
        )?
        .map(AcceptanceTime);
        entries.push(FactHistoryEntry {
            fact,
            valid_from_sequence: u64::try_from(valid_from_sequence).map_err(|error| {
                TursoStoreError::Snapshot(format!("invalid fact start sequence: {error}"))
            })?,
            valid_until_sequence,
            version: FactHistoryVersion {
                valid_from: start.manifest.generation,
                valid_until,
                observed_at,
                accepted_at,
                payload,
            },
        });
    }
    Ok(entries)
}

async fn read_fact_version_changes_at_page(
    connection: &turso::Connection,
    generation: GenerationId,
    after: Option<FactVersionChangeCursor>,
    limit: usize,
) -> Result<FactVersionChangePage, TursoStoreError> {
    if limit > MAX_FACT_VERSION_CHANGE_PAGE_SIZE {
        return Err(StoreError::InvalidPageLimit.into());
    }
    if after.is_some_and(|cursor| cursor.generation != generation) {
        return Err(StoreError::InvalidDelta(
            "fact-version-change cursor is bound to a different generation".to_owned(),
        )
        .into());
    }
    if limit == 0 {
        return Ok(FactVersionChangePage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let changed_sequence = history_sequence_for_generation(connection, generation).await?;
    let (after_kind, after_id, after_sequence) =
        after.map_or((-1_i64, Vec::new(), 0_i64), |cursor| {
            let (kind, id) = syntaxmesh_store::fact_storage_key(cursor.after_fact);
            (
                kind,
                id,
                i64::try_from(cursor.after_valid_from_sequence).unwrap_or(i64::MAX),
            )
        });
    if after_sequence > changed_sequence {
        return Err(StoreError::InvalidDelta(
            "fact-version-change cursor is after its generation".to_owned(),
        )
        .into());
    }
    if after.is_some() && after_sequence == 0 {
        return Err(StoreError::InvalidDelta(
            "fact-version-change cursor has a zero start sequence".to_owned(),
        )
        .into());
    }
    let fetch_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut rows = connection
        .query(
            FACT_VERSION_CHANGE_PAGE_QUERY,
            (
                changed_sequence,
                after_kind,
                after_id,
                after_sequence,
                fetch_limit,
            ),
        )
        .await
        .map_err(|error| {
            TursoStoreError::Backend(format!("read fact-version-change page: {error}"))
        })?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read fact-version-change row: {error}"))
    })? {
        let kind = row.get::<i64>(0).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact kind: {error}"))
        })?;
        let id = match row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact identity: {error}"))
        })? {
            turso::Value::Blob(bytes) => bytes,
            other @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "changed fact identity is not a blob: {other:?}"
                )));
            }
        };
        let fact = fact_ref_from_parts(kind, id)?;
        let valid_from_sequence = row.get::<i64>(2).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact start sequence: {error}"))
        })?;
        let valid_until_sequence = match row.get_value(3).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact end sequence: {error}"))
        })? {
            turso::Value::Integer(sequence) => Some(u64::try_from(sequence).map_err(|error| {
                TursoStoreError::Snapshot(format!("invalid changed fact end sequence: {error}"))
            })?),
            turso::Value::Null => None,
            other @ (turso::Value::Real(_) | turso::Value::Text(_) | turso::Value::Blob(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "changed fact end sequence has unexpected type: {other:?}"
                )));
            }
        };
        let start: GenerationHistoryEntry = decode_blob(
            row.get_value(4).map_err(|error| {
                TursoStoreError::Backend(format!("read changed fact start generation: {error}"))
            })?,
            "changed fact start generation",
        )?;
        let valid_until = match row.get_value(5).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact end generation: {error}"))
        })? {
            turso::Value::Blob(bytes) => Some(
                bincode::deserialize::<GenerationHistoryEntry>(&bytes)
                    .map_err(|error| {
                        TursoStoreError::Snapshot(format!("decode end generation: {error}"))
                    })?
                    .manifest
                    .generation,
            ),
            turso::Value::Null => None,
            other @ (turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "changed fact end generation has unexpected type: {other:?}"
                )));
            }
        };
        let payload_bytes = row.get_value(6).map_err(|error| {
            TursoStoreError::Backend(format!("read changed fact payload: {error}"))
        })?;
        let payload = match fact {
            FactRef::File(_) => FactPayload::File(decode_blob(payload_bytes, "file payload")?),
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode_blob(payload_bytes, "provenance payload")?)
            }
            FactRef::Node(_) => FactPayload::Node(decode_blob(payload_bytes, "node payload")?),
            FactRef::Edge(_) => FactPayload::Edge(decode_blob(payload_bytes, "edge payload")?),
        };
        let observed_at = read_optional_time_value(
            row.get_value(7).map_err(|error| {
                TursoStoreError::Backend(format!("read changed fact observation time: {error}"))
            })?,
            "observation time",
        )?
        .map(ObservationTime);
        let accepted_at = read_optional_time_value(
            row.get_value(8).map_err(|error| {
                TursoStoreError::Backend(format!("read changed fact acceptance time: {error}"))
            })?,
            "acceptance time",
        )?
        .map(AcceptanceTime);
        items.push(FactHistoryEntry {
            fact,
            valid_from_sequence: u64::try_from(valid_from_sequence).map_err(|error| {
                TursoStoreError::Snapshot(format!("invalid changed fact start sequence: {error}"))
            })?,
            valid_until_sequence,
            version: FactHistoryVersion {
                valid_from: start.manifest.generation,
                valid_until,
                observed_at,
                accepted_at,
                payload,
            },
        });
    }
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|entry| FactVersionChangeCursor {
            generation,
            after_fact: entry.fact,
            after_valid_from_sequence: entry.valid_from_sequence,
        })
    } else {
        None
    };
    Ok(FactVersionChangePage { items, next_cursor })
}

async fn read_fact_history_page(
    connection: &turso::Connection,
    as_of_generation: GenerationId,
    after: Option<FactHistoryCursor>,
    limit: usize,
) -> Result<FactHistoryPage, TursoStoreError> {
    if limit == 0 {
        return Ok(FactHistoryPage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    if after.is_some_and(|cursor| cursor.as_of_generation != as_of_generation) {
        return Err(TursoStoreError::Backend(
            "fact-history cursor is bound to a different generation".to_owned(),
        ));
    }
    let as_of_sequence = history_sequence_for_generation(connection, as_of_generation).await?;
    let (after_kind, after_id, after_sequence) = if let Some(cursor) = after {
        let (kind, id) = syntaxmesh_store::fact_storage_key(cursor.after_fact);
        let sequence = history_sequence_for_generation(connection, cursor.after_valid_from).await?;
        if sequence > as_of_sequence {
            return Err(TursoStoreError::Backend(
                "fact-history cursor is outside its retained snapshot".to_owned(),
            ));
        }
        (Some(kind), Some(id), Some(sequence))
    } else {
        (None, None, None)
    };
    let sql_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut rows =
        if let (Some(kind), Some(id), Some(sequence)) = (after_kind, after_id, after_sequence) {
            connection
                .query(
                    FACT_HISTORY_AFTER_PAGE_QUERY,
                    (as_of_generation.0.0.to_vec(), kind, id, sequence, sql_limit),
                )
                .await
        } else {
            connection
                .query(
                    FACT_HISTORY_FIRST_PAGE_QUERY,
                    (as_of_generation.0.0.to_vec(), sql_limit),
                )
                .await
        }
        .map_err(|error| TursoStoreError::Backend(format!("page fact history: {error}")))?;
    let mut items = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read fact history page: {error}")))?
    {
        let kind = row
            .get::<i64>(0)
            .map_err(|error| TursoStoreError::Backend(format!("read fact family: {error}")))?;
        let id = match row
            .get_value(1)
            .map_err(|error| TursoStoreError::Backend(format!("read fact identity: {error}")))?
        {
            turso::Value::Blob(bytes) => bytes,
            other @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "fact identity is not a blob: {other:?}"
                )));
            }
        };
        let id: [u8; 32] = id.try_into().map_err(|bytes: Vec<u8>| {
            TursoStoreError::Snapshot(format!("fact history identity has {} bytes", bytes.len()))
        })?;
        let fact = match kind {
            FILE_FACT => FactRef::File(FileId(StableId(id))),
            PROVENANCE_FACT => FactRef::Provenance(ProvenanceId(StableId(id))),
            NODE_FACT => FactRef::Node(NodeId(StableId(id))),
            EDGE_FACT => FactRef::Edge(EdgeId(StableId(id))),
            _ => {
                return Err(TursoStoreError::Snapshot(format!(
                    "unknown fact history family {kind}"
                )));
            }
        };
        let start: GenerationHistoryEntry = decode_blob(
            row.get_value(2)
                .map_err(|error| TursoStoreError::Backend(format!("read fact start: {error}")))?,
            "fact history start",
        )?;
        let valid_until = match row
            .get_value(3)
            .map_err(|error| TursoStoreError::Backend(format!("read fact end: {error}")))?
        {
            turso::Value::Blob(bytes) => Some(
                bincode::deserialize::<GenerationHistoryEntry>(&bytes)
                    .map_err(|error| {
                        TursoStoreError::Snapshot(format!("decode fact history end: {error}"))
                    })?
                    .manifest
                    .generation,
            ),
            turso::Value::Null => None,
            other @ (turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "fact history end is not a blob or null: {other:?}"
                )));
            }
        };
        let payload_bytes = row
            .get_value(4)
            .map_err(|error| TursoStoreError::Backend(format!("read fact payload: {error}")))?;
        let payload = match fact {
            FactRef::File(_) => FactPayload::File(decode_blob(payload_bytes, "file fact")?),
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode_blob(payload_bytes, "provenance fact")?)
            }
            FactRef::Node(_) => FactPayload::Node(decode_blob(payload_bytes, "node fact")?),
            FactRef::Edge(_) => FactPayload::Edge(decode_blob(payload_bytes, "edge fact")?),
        };
        let observed_at = read_optional_time_value(
            row.get_value(5).map_err(|error| {
                TursoStoreError::Backend(format!("read observation time: {error}"))
            })?,
            "observation time",
        )?
        .map(ObservationTime);
        let accepted_at = read_optional_time_value(
            row.get_value(6).map_err(|error| {
                TursoStoreError::Backend(format!("read acceptance time: {error}"))
            })?,
            "acceptance time",
        )?
        .map(AcceptanceTime);
        let valid_from_sequence = row.get::<i64>(7).map_err(|error| {
            TursoStoreError::Backend(format!("read fact start sequence: {error}"))
        })?;
        let valid_from_sequence = u64::try_from(valid_from_sequence).map_err(|error| {
            TursoStoreError::Snapshot(format!("invalid fact start sequence: {error}"))
        })?;
        let valid_until_sequence = match row
            .get_value(8)
            .map_err(|error| TursoStoreError::Backend(format!("read fact end sequence: {error}")))?
        {
            turso::Value::Integer(sequence) => Some(u64::try_from(sequence).map_err(|error| {
                TursoStoreError::Snapshot(format!("invalid fact end sequence: {error}"))
            })?),
            turso::Value::Null => None,
            other @ (turso::Value::Real(_) | turso::Value::Text(_) | turso::Value::Blob(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "fact end sequence is not an integer or null: {other:?}"
                )));
            }
        };
        items.push(FactHistoryEntry {
            fact,
            valid_from_sequence,
            valid_until_sequence,
            version: FactHistoryVersion {
                valid_from: start.manifest.generation,
                valid_until,
                observed_at,
                accepted_at,
                payload,
            },
        });
    }
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|entry| FactHistoryCursor {
            as_of_generation,
            after_fact: entry.fact,
            after_valid_from: entry.version.valid_from,
        })
    } else {
        None
    };
    Ok(FactHistoryPage { items, next_cursor })
}

fn read_optional_time_value(
    value: turso::Value,
    name: &str,
) -> Result<Option<u64>, TursoStoreError> {
    match value {
        turso::Value::Blob(bytes) => {
            let bytes: [u8; 8] = bytes.try_into().map_err(|invalid: Vec<u8>| {
                TursoStoreError::Snapshot(format!(
                    "{name} has {} bytes instead of eight",
                    invalid.len()
                ))
            })?;
            Ok(Some(u64::from_be_bytes(bytes)))
        }
        turso::Value::Null => Ok(None),
        turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_) => Err(
            TursoStoreError::Snapshot(format!("{name} is not a timestamp blob or null")),
        ),
    }
}

fn fact_ref_parts(fact: FactRef) -> (i64, Vec<u8>) {
    match fact {
        FactRef::File(id) => (FILE_FACT, id.0.0.to_vec()),
        FactRef::Provenance(id) => (PROVENANCE_FACT, id.0.0.to_vec()),
        FactRef::Node(id) => (NODE_FACT, id.0.0.to_vec()),
        FactRef::Edge(id) => (EDGE_FACT, id.0.0.to_vec()),
    }
}

fn fact_ref_from_parts(kind: i64, bytes: Vec<u8>) -> Result<FactRef, TursoStoreError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|invalid: Vec<u8>| {
        TursoStoreError::Snapshot(format!(
            "fact history identity has {} bytes instead of 32",
            invalid.len()
        ))
    })?;
    let id = StableId(bytes);
    match kind {
        FILE_FACT => Ok(FactRef::File(FileId(id))),
        PROVENANCE_FACT => Ok(FactRef::Provenance(ProvenanceId(id))),
        NODE_FACT => Ok(FactRef::Node(NodeId(id))),
        EDGE_FACT => Ok(FactRef::Edge(EdgeId(id))),
        _ => Err(TursoStoreError::Snapshot(format!(
            "unknown temporal fact kind {kind}"
        ))),
    }
}

async fn read_observed_facts_between(
    connection: &turso::Connection,
    from_inclusive: ObservationTime,
    until_exclusive: ObservationTime,
    after: Option<ObservedFactCursor>,
    limit: usize,
) -> Result<ObservedFactPage, TursoStoreError> {
    if from_inclusive >= until_exclusive {
        return Err(TursoStoreError::Store(StoreError::InvalidTemporalRange));
    }
    if limit == 0 {
        return Ok(ObservedFactPage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let fetch_count = limit
        .checked_add(1)
        .ok_or_else(|| TursoStoreError::Snapshot("observation limit overflow".to_owned()))?;
    let fetch_count = i64::try_from(fetch_count)
        .map_err(|error| TursoStoreError::Snapshot(format!("observation limit: {error}")))?;
    let from = from_inclusive.0.to_be_bytes().to_vec();
    let until = until_exclusive.0.to_be_bytes().to_vec();
    let mut items = if let Some(cursor) = after {
        let (kind, id) = fact_ref_parts(cursor.fact);
        let sequence = history_sequence_for_generation(connection, cursor.valid_from).await?;
        let cursor_time = cursor.observed_at.0.to_be_bytes().to_vec();
        let mut rows = connection.query(
            "SELECT versions.fact_kind, versions.fact_id, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.observed_at_unix_nanos IS NOT NULL AND versions.observed_at_unix_nanos >= ?1 AND versions.observed_at_unix_nanos < ?2 AND (versions.observed_at_unix_nanos, versions.fact_kind, versions.fact_id, versions.valid_from_sequence) > (?3, ?4, ?5, ?6) ORDER BY versions.observed_at_unix_nanos, versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?7",
            (from, until, cursor_time, kind, id, sequence, fetch_count),
        ).await.map_err(|error| TursoStoreError::Backend(format!("seek observation timeline: {error}")))?;
        read_observed_rows(&mut rows).await?
    } else {
        let mut rows = connection.query(
            "SELECT versions.fact_kind, versions.fact_id, start_entry.payload, end_entry.payload, versions.payload, versions.observed_at_unix_nanos, acceptance.accepted_at_unix_nanos FROM syntaxmesh_fact_versions AS versions JOIN syntaxmesh_generation_history AS start_entry ON start_entry.sequence = versions.valid_from_sequence LEFT JOIN syntaxmesh_generation_history AS end_entry ON end_entry.sequence = versions.valid_until_sequence LEFT JOIN syntaxmesh_generation_acceptance AS acceptance ON acceptance.generation = start_entry.generation WHERE versions.observed_at_unix_nanos IS NOT NULL AND versions.observed_at_unix_nanos >= ?1 AND versions.observed_at_unix_nanos < ?2 ORDER BY versions.observed_at_unix_nanos, versions.fact_kind, versions.fact_id, versions.valid_from_sequence LIMIT ?3",
            (from, until, fetch_count),
        ).await.map_err(|error| TursoStoreError::Backend(format!("read observation timeline: {error}")))?;
        read_observed_rows(&mut rows).await?
    };
    let next_cursor = if items.len() > limit {
        limit
            .checked_sub(1)
            .and_then(|index| items.get(index))
            .and_then(|item| {
                item.version
                    .observed_at
                    .map(|observed_at| ObservedFactCursor {
                        observed_at,
                        fact: item.fact,
                        valid_from: item.version.valid_from,
                    })
            })
    } else {
        None
    };
    items.truncate(limit);
    Ok(ObservedFactPage { items, next_cursor })
}

async fn read_observed_rows(
    rows: &mut turso::Rows,
) -> Result<Vec<ObservedFactVersion>, TursoStoreError> {
    let mut items = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read observation row: {error}")))?
    {
        let kind = match row.get_value(0).map_err(|error| {
            TursoStoreError::Backend(format!("read observation fact kind: {error}"))
        })? {
            turso::Value::Integer(value) => value,
            turso::Value::Null
            | turso::Value::Real(_)
            | turso::Value::Text(_)
            | turso::Value::Blob(_) => {
                return Err(TursoStoreError::Snapshot(
                    "observation fact kind is not integer".to_owned(),
                ));
            }
        };
        let id = match row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read observation fact ID: {error}"))
        })? {
            turso::Value::Blob(value) => value,
            turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "observation fact ID is not a blob".to_owned(),
                ));
            }
        };
        let fact = fact_ref_from_parts(kind, id)?;
        let start: GenerationHistoryEntry = decode_blob(
            row.get_value(2).map_err(|error| {
                TursoStoreError::Backend(format!("read observation start generation: {error}"))
            })?,
            "observation start generation",
        )?;
        let valid_until = match row.get_value(3).map_err(|error| {
            TursoStoreError::Backend(format!("read observation end generation: {error}"))
        })? {
            turso::Value::Blob(payload) => Some(
                bincode::deserialize::<GenerationHistoryEntry>(&payload)
                    .map_err(|error| {
                        TursoStoreError::Snapshot(format!(
                            "decode observation end generation: {error}"
                        ))
                    })?
                    .manifest
                    .generation,
            ),
            turso::Value::Null => None,
            turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_) => {
                return Err(TursoStoreError::Snapshot(
                    "observation end generation is not blob or null".to_owned(),
                ));
            }
        };
        let payload_value = row.get_value(4).map_err(|error| {
            TursoStoreError::Backend(format!("read observation payload: {error}"))
        })?;
        let observed_at = read_optional_time_value(
            row.get_value(5).map_err(|error| {
                TursoStoreError::Backend(format!("read observation timestamp: {error}"))
            })?,
            "observation timestamp",
        )?
        .map(ObservationTime)
        .ok_or_else(|| {
            TursoStoreError::Snapshot("observation timeline returned NULL time".to_owned())
        })?;
        let accepted_at = read_optional_time_value(
            row.get_value(6).map_err(|error| {
                TursoStoreError::Backend(format!("read acceptance timestamp: {error}"))
            })?,
            "acceptance timestamp",
        )?
        .map(AcceptanceTime);
        let payload = match fact {
            FactRef::File(_) => {
                FactPayload::File(decode_blob(payload_value, "observed file payload")?)
            }
            FactRef::Provenance(_) => {
                FactPayload::Provenance(decode_blob(payload_value, "observed provenance payload")?)
            }
            FactRef::Node(_) => {
                FactPayload::Node(decode_blob(payload_value, "observed node payload")?)
            }
            FactRef::Edge(_) => {
                FactPayload::Edge(decode_blob(payload_value, "observed edge payload")?)
            }
        };
        items.push(ObservedFactVersion {
            fact,
            version: FactHistoryVersion {
                valid_from: start.manifest.generation,
                valid_until,
                observed_at: Some(observed_at),
                accepted_at,
                payload,
            },
        });
    }
    Ok(items)
}

async fn read_acceptance_time(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<Option<AcceptanceTime>, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
            [generation.0.0.to_vec()],
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read acceptance time: {error}")))?;
    if let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read acceptance time row: {error}")))?
    {
        let bytes = match row.get_value(0).map_err(|error| {
            TursoStoreError::Backend(format!("decode acceptance-time bytes: {error}"))
        })? {
            turso::Value::Blob(bytes) => bytes,
            other @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "acceptance time has invalid storage type: {other:?}"
                )));
            }
        };
        let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
            TursoStoreError::Snapshot(format!(
                "acceptance time must be eight bytes, got {}",
                bytes.len()
            ))
        })?;
        return Ok(Some(AcceptanceTime(u64::from_be_bytes(bytes))));
    }
    let known_generation = read_single::<GenerationHistoryEntry>(
        connection,
        "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
    )
    .await?;
    if known_generation.is_none() {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        }));
    }
    Ok(None)
}

async fn read_accepted_through(
    connection: &turso::Connection,
    generation: GenerationId,
) -> Result<Option<AcceptanceTime>, TursoStoreError> {
    let mut rows = connection.query(
        "SELECT accepted_through_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
        [generation.0.0.to_vec()],
    ).await.map_err(|error| TursoStoreError::Backend(format!("read acceptance prefix: {error}")))?;
    if let Some(row) = rows
        .next()
        .await
        .map_err(|error| TursoStoreError::Backend(format!("read acceptance-prefix row: {error}")))?
    {
        match row.get_value(0).map_err(|error| {
            TursoStoreError::Backend(format!("decode acceptance prefix: {error}"))
        })? {
            turso::Value::Blob(bytes) => {
                let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    TursoStoreError::Snapshot(format!(
                        "accepted-through watermark must be eight bytes, got {}",
                        bytes.len()
                    ))
                })?;
                return Ok(Some(AcceptanceTime(u64::from_be_bytes(bytes))));
            }
            turso::Value::Null => {}
            other @ (turso::Value::Integer(_) | turso::Value::Real(_) | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "accepted-through watermark has invalid storage type: {other:?}"
                )));
            }
        }
    }
    if read_single::<GenerationHistoryEntry>(
        connection,
        "SELECT payload FROM syntaxmesh_generation_history WHERE generation = ?1",
    )
    .await?
    .is_none()
    {
        return Err(TursoStoreError::Store(StoreError::StaleBase {
            expected: Some(generation),
            actual: None,
        }));
    }
    Ok(None)
}

async fn read_accepted_generations_between(
    connection: &turso::Connection,
    from_inclusive: AcceptanceTime,
    until_exclusive: AcceptanceTime,
    after: Option<AcceptedGenerationCursor>,
    limit: usize,
) -> Result<AcceptedGenerationPage, TursoStoreError> {
    if limit == 0 {
        return Ok(AcceptedGenerationPage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let sql_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut rows = if let Some(cursor) = after {
        connection
            .query(
                "SELECT history.payload, accepted.accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance AS accepted JOIN syntaxmesh_generation_history AS history ON history.generation = accepted.generation WHERE accepted.accepted_at_unix_nanos >= ?1 AND accepted.accepted_at_unix_nanos < ?2 AND (accepted.accepted_at_unix_nanos, accepted.generation) > (?3, ?4) ORDER BY accepted.accepted_at_unix_nanos, accepted.generation LIMIT ?5",
                (
                    from_inclusive.0.to_be_bytes().to_vec(),
                    until_exclusive.0.to_be_bytes().to_vec(),
                    cursor.accepted_at.0.to_be_bytes().to_vec(),
                    cursor.generation.0.0.to_vec(),
                    sql_limit,
                ),
            )
            .await
    } else {
        connection
            .query(
                "SELECT history.payload, accepted.accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance AS accepted JOIN syntaxmesh_generation_history AS history ON history.generation = accepted.generation WHERE accepted.accepted_at_unix_nanos >= ?1 AND accepted.accepted_at_unix_nanos < ?2 ORDER BY accepted.accepted_at_unix_nanos, accepted.generation LIMIT ?3",
                (
                    from_inclusive.0.to_be_bytes().to_vec(),
                    until_exclusive.0.to_be_bytes().to_vec(),
                    sql_limit,
                ),
            )
            .await
    }
    .map_err(|error| {
        TursoStoreError::Backend(format!("read accepted-generation range: {error}"))
    })?;
    let mut accepted = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read accepted-generation row: {error}"))
    })? {
        let history: GenerationHistoryEntry = decode_blob(
            row.get_value(0).map_err(|error| {
                TursoStoreError::Backend(format!("read accepted-generation manifest: {error}"))
            })?,
            "accepted-generation manifest",
        )?;
        let bytes = match row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("read accepted-generation timestamp: {error}"))
        })? {
            turso::Value::Blob(bytes) => bytes,
            other @ (turso::Value::Null
            | turso::Value::Integer(_)
            | turso::Value::Real(_)
            | turso::Value::Text(_)) => {
                return Err(TursoStoreError::Snapshot(format!(
                    "accepted-generation time has invalid storage type: {other:?}"
                )));
            }
        };
        let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
            TursoStoreError::Snapshot(format!(
                "accepted-generation time must be eight bytes, got {}",
                bytes.len()
            ))
        })?;
        accepted.push(AcceptedGeneration {
            accepted_at: AcceptanceTime(u64::from_be_bytes(bytes)),
            manifest: history.manifest,
        });
    }
    let next_cursor = if accepted.len() > limit {
        limit
            .checked_sub(1)
            .and_then(|last_index| accepted.get(last_index))
            .map(|item| AcceptedGenerationCursor {
                accepted_at: item.accepted_at,
                generation: item.manifest.generation,
            })
    } else {
        None
    };
    accepted.truncate(limit);
    Ok(AcceptedGenerationPage {
        items: accepted,
        next_cursor,
    })
}

async fn read_changes_between(
    connection: &turso::Connection,
    from: GenerationId,
    to: GenerationId,
) -> Result<Vec<GenerationChange>, TursoStoreError> {
    let from_sequence = history_sequence_for_generation(connection, from).await?;
    let to_sequence = history_sequence_for_generation(connection, to).await?;
    if from_sequence >= to_sequence {
        return Err(TursoStoreError::Store(StoreError::InvalidDelta(
            "change range must move forward between retained generations".to_owned(),
        )));
    }
    let entries =
        read_change_history_rows(connection, from_sequence, to_sequence, i64::MAX).await?;
    let mut parent = from;
    let mut expected_sequence = from_sequence
        .checked_add(1)
        .ok_or_else(|| TursoStoreError::Snapshot("generation sequence overflow".to_owned()))?;
    let mut changes = Vec::with_capacity(entries.len());
    for (sequence, entry) in entries {
        if sequence != expected_sequence || entry.manifest.parent != Some(parent) {
            return Err(TursoStoreError::Snapshot(
                "change range is not a contiguous generation chain".to_owned(),
            ));
        }
        let delta = entry.delta.ok_or_else(|| {
            TursoStoreError::Snapshot("change range crosses a non-transition anchor".to_owned())
        })?;
        parent = entry.manifest.generation;
        expected_sequence = expected_sequence
            .checked_add(1)
            .ok_or_else(|| TursoStoreError::Snapshot("generation sequence overflow".to_owned()))?;
        changes.push(GenerationChange {
            sequence: u64::try_from(sequence).map_err(|error| {
                TursoStoreError::Snapshot(format!("invalid generation sequence: {error}"))
            })?,
            manifest: entry.manifest,
            delta,
        });
    }
    if parent != to {
        return Err(TursoStoreError::Snapshot(
            "change range did not reach its requested end generation".to_owned(),
        ));
    }
    Ok(changes)
}

async fn read_changes_between_page(
    connection: &turso::Connection,
    from: GenerationId,
    to: GenerationId,
    after: Option<GenerationChangeCursor>,
    limit: usize,
) -> Result<GenerationChangePage, TursoStoreError> {
    if limit == 0 {
        return Ok(GenerationChangePage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    if limit > MAX_GENERATION_CHANGE_PAGE_SIZE {
        return Err(TursoStoreError::Store(StoreError::InvalidPageLimit));
    }
    let from_sequence = history_sequence_for_generation(connection, from).await?;
    let to_sequence = history_sequence_for_generation(connection, to).await?;
    if from_sequence >= to_sequence {
        return Err(TursoStoreError::Store(StoreError::InvalidDelta(
            "change range must move forward between retained generations".to_owned(),
        )));
    }
    let (after_sequence, mut parent) = match after {
        Some(cursor) if cursor.from_generation == from && cursor.to_generation == to => {
            let cursor_sequence =
                history_sequence_for_generation(connection, cursor.after_generation).await?;
            let stored_cursor_sequence = u64::try_from(cursor_sequence).map_err(|error| {
                TursoStoreError::Snapshot(format!("invalid generation sequence: {error}"))
            })?;
            if stored_cursor_sequence != cursor.after_sequence
                || cursor_sequence < from_sequence
                || cursor_sequence > to_sequence
            {
                return Err(TursoStoreError::Store(StoreError::InvalidDelta(
                    "generation-change cursor is outside its pinned range".to_owned(),
                )));
            }
            (cursor_sequence, cursor.after_generation)
        }
        Some(_) => {
            return Err(TursoStoreError::Store(StoreError::InvalidDelta(
                "generation-change cursor is bound to a different range".to_owned(),
            )));
        }
        None => (from_sequence, from),
    };
    let mut expected_sequence = after_sequence
        .checked_add(1)
        .ok_or_else(|| TursoStoreError::Snapshot("generation sequence overflow".to_owned()))?;
    let query_limit = i64::try_from(limit.saturating_add(1))
        .map_err(|_error| TursoStoreError::Store(StoreError::InvalidPageLimit))?;
    let mut entries =
        read_change_history_rows(connection, after_sequence, to_sequence, query_limit).await?;
    let has_more = entries.len() > limit;
    if has_more {
        entries.truncate(limit);
    }
    let mut items = Vec::with_capacity(entries.len());
    for (sequence, entry) in entries {
        if sequence != expected_sequence || entry.manifest.parent != Some(parent) {
            return Err(TursoStoreError::Snapshot(
                "change range is not a contiguous generation chain".to_owned(),
            ));
        }
        let delta = entry.delta.ok_or_else(|| {
            TursoStoreError::Snapshot("change range crosses a non-transition anchor".to_owned())
        })?;
        parent = entry.manifest.generation;
        expected_sequence = expected_sequence
            .checked_add(1)
            .ok_or_else(|| TursoStoreError::Snapshot("generation sequence overflow".to_owned()))?;
        items.push(GenerationChange {
            sequence: u64::try_from(sequence).map_err(|error| {
                TursoStoreError::Snapshot(format!("invalid generation sequence: {error}"))
            })?,
            manifest: entry.manifest,
            delta,
        });
    }
    if !has_more && parent != to {
        return Err(TursoStoreError::Snapshot(
            "change range did not reach its requested end generation".to_owned(),
        ));
    }
    let next_cursor = if has_more {
        items.last().map(|change| GenerationChangeCursor {
            from_generation: from,
            to_generation: to,
            after_sequence: change.sequence,
            after_generation: change.manifest.generation,
        })
    } else {
        None
    };
    Ok(GenerationChangePage { items, next_cursor })
}

async fn read_change_history_rows(
    connection: &turso::Connection,
    after_sequence: i64,
    to_sequence: i64,
    limit: i64,
) -> Result<Vec<(i64, GenerationHistoryEntry)>, TursoStoreError> {
    let mut rows = connection
        .query(
            "SELECT sequence, payload FROM syntaxmesh_generation_history WHERE sequence > ?1 AND sequence <= ?2 ORDER BY sequence LIMIT ?3",
            (after_sequence, to_sequence, limit),
        )
        .await
        .map_err(|error| TursoStoreError::Backend(format!("query generation change page: {error}")))?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(|error| {
        TursoStoreError::Backend(format!("read generation change page: {error}"))
    })? {
        let sequence = match row.get_value(0).map_err(|error| {
            TursoStoreError::Backend(format!("decode generation change sequence: {error}"))
        })? {
            turso::Value::Integer(value) => value,
            turso::Value::Null
            | turso::Value::Real(_)
            | turso::Value::Text(_)
            | turso::Value::Blob(_) => {
                return Err(TursoStoreError::Snapshot(
                    "generation change sequence is not an integer".to_owned(),
                ));
            }
        };
        let payload = row.get_value(1).map_err(|error| {
            TursoStoreError::Backend(format!("decode generation change payload: {error}"))
        })?;
        let entry = decode_blob(payload, "generation change payload")?;
        entries.push((sequence, entry));
    }
    Ok(entries)
}
