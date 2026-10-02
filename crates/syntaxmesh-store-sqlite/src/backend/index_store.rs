//! Indexed graph reads and generation-pinned store operations for SQLite.

use super::tree_store::historical_incident_edge_page;
use super::*;

fn read_temporal_evidence<T: serde::de::DeserializeOwned>(
    connection: &rusqlite::Connection,
    generation: GenerationId,
    kind: i64,
    id: syntaxmesh_core::StableId,
) -> Result<Option<T>, StoreError> {
    let sequence = connection
        .query_row(
            "SELECT sequence FROM syntaxmesh_generation_history WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or(StoreError::StaleBase {
            expected: Some(generation),
            actual: read_manifest(connection)?.map(|manifest| manifest.generation),
        })?;
    let mut nodes = read_many_with_params::<T, _>(
        connection,
        "SELECT payload FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence = (SELECT MAX(valid_from_sequence) FROM syntaxmesh_fact_versions WHERE fact_kind = ?1 AND fact_id = ?2 AND valid_from_sequence <= ?3) AND (valid_until_sequence IS NULL OR valid_until_sequence > ?3) LIMIT 1",
        params![kind, id.0.as_slice(), sequence],
    )?;
    Ok(nodes.pop())
}

impl GraphStore for SqliteGraphStore {
    fn current_generation(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationManifest>, StoreError> {
        Ok(read_manifest(&self.connection)?
            .filter(|manifest| manifest.repository == repository && manifest.worktree == worktree))
    }

    fn manifest(&self, generation: GenerationId) -> Result<GenerationManifest, StoreError> {
        if let Some(current) = read_manifest(&self.connection)?
            && current.generation == generation
        {
            return Ok(current);
        }
        read_generation_history_entry(&self.connection, generation)?
            .map(|entry| entry.manifest)
            .ok_or(StoreError::StaleBase {
                expected: Some(generation),
                actual: read_manifest(&self.connection)?.map(|manifest| manifest.generation),
            })
    }

    fn generation_history(&self) -> Result<Vec<GenerationHistoryEntry>, StoreError> {
        self.memory.generation_history()
    }

    fn current_generation_entry(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<GenerationHistoryEntry>, StoreError> {
        let Some(current) = self.current_generation(repository, worktree)? else {
            return Ok(None);
        };
        read_generation_history_entry(&self.connection, current.generation)
    }

    fn generation_lineage_history(&self) -> Result<Vec<GenerationLineageEntry>, StoreError> {
        read_many(
            &self.connection,
            "SELECT payload FROM syntaxmesh_generation_lineage ORDER BY sequence",
        )
    }

    fn generation_consequence_history(
        &self,
    ) -> Result<Vec<GenerationConsequenceEntry>, StoreError> {
        read_many(
            &self.connection,
            "SELECT payload FROM syntaxmesh_generation_consequences ORDER BY sequence",
        )
    }

    fn generation_sequence(&self, generation: GenerationId) -> Result<u64, StoreError> {
        u64::try_from(history_sequence(&self.connection, generation)?)
            .map_err(|error| StoreError::Integrity(format!("invalid generation sequence: {error}")))
    }

    fn change_events_for_fact(
        &self,
        fact: FactRef,
        after: Option<ChangeEventCursor>,
        limit: usize,
    ) -> Result<ChangeEventPage, StoreError> {
        read_change_events_for_fact(&self.connection, fact, after, limit)
    }

    fn change_event(&self, generation: GenerationId) -> Result<Option<ChangeEvent>, StoreError> {
        read_change_event(&self.connection, generation)
    }

    fn events_for_change_set(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
        after: Option<EventsForChangeSetCursor>,
        limit: usize,
    ) -> Result<EventsForChangeSetPage, StoreError> {
        read_events_for_change_set(&self.connection, change_set, as_of, after, limit)
    }

    fn consequence_edges_for_endpoint(
        &self,
        endpoint: LineageEndpoint,
        as_of: GenerationId,
        after: Option<ConsequenceEdgeCursor>,
        limit: usize,
    ) -> Result<ConsequenceEdgePage, StoreError> {
        read_consequence_edges_for_endpoint(&self.connection, endpoint, as_of, after, limit)
    }

    fn consequence_edges_for_endpoint_range(
        &self,
        endpoint: LineageEndpoint,
        from_generation: GenerationId,
        until_generation: GenerationId,
        after: Option<ConsequenceRangeCursor>,
        limit: usize,
    ) -> Result<ConsequenceRangePage, StoreError> {
        read_consequence_edges_for_endpoint_range(
            &self.connection,
            endpoint,
            from_generation,
            until_generation,
            after,
            limit,
        )
    }

    fn change_set_at(
        &self,
        change_set: ChangeSetId,
        as_of: GenerationId,
    ) -> Result<Option<ChangeSetVersion>, StoreError> {
        read_change_set_at(&self.connection, change_set, as_of)
    }

    fn acceptance_time(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError> {
        if read_generation_history_entry(&self.connection, generation)?.is_none() {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: read_manifest(&self.connection)?.map(|manifest| manifest.generation),
            });
        }
        let value = self
            .connection
            .query_row(
                "SELECT accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
                [generation.0.0.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(sql_error)?;
        value
            .map(|bytes| {
                let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    StoreError::Integrity(format!(
                        "generation acceptance time has {} bytes",
                        bytes.len()
                    ))
                })?;
                Ok(AcceptanceTime(u64::from_be_bytes(bytes)))
            })
            .transpose()
    }

    fn accepted_through(
        &self,
        generation: GenerationId,
    ) -> Result<Option<AcceptanceTime>, StoreError> {
        if read_generation_history_entry(&self.connection, generation)?.is_none() {
            return Err(StoreError::StaleBase {
                expected: Some(generation),
                actual: read_manifest(&self.connection)?.map(|manifest| manifest.generation),
            });
        }
        let bytes = self.connection.query_row(
            "SELECT accepted_through_unix_nanos FROM syntaxmesh_generation_acceptance WHERE generation = ?1",
            [generation.0.0.as_slice()],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        ).optional().map_err(sql_error)?.flatten();
        bytes
            .map(|bytes| {
                let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    StoreError::Integrity(format!(
                        "accepted-through watermark has {} bytes",
                        bytes.len()
                    ))
                })?;
                Ok(AcceptanceTime(u64::from_be_bytes(bytes)))
            })
            .transpose()
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
        let sql = if after.is_some() {
            "SELECT history.payload, accepted.accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance AS accepted JOIN syntaxmesh_generation_history AS history ON history.generation = accepted.generation WHERE accepted.accepted_at_unix_nanos >= ?1 AND accepted.accepted_at_unix_nanos < ?2 AND (accepted.accepted_at_unix_nanos, accepted.generation) > (?3, ?4) ORDER BY accepted.accepted_at_unix_nanos, accepted.generation LIMIT ?5"
        } else {
            "SELECT history.payload, accepted.accepted_at_unix_nanos FROM syntaxmesh_generation_acceptance AS accepted JOIN syntaxmesh_generation_history AS history ON history.generation = accepted.generation WHERE accepted.accepted_at_unix_nanos >= ?1 AND accepted.accepted_at_unix_nanos < ?2 ORDER BY accepted.accepted_at_unix_nanos, accepted.generation LIMIT ?3"
        };
        let mut statement = self.connection.prepare(sql).map_err(sql_error)?;
        let row_mapper =
            |row: &rusqlite::Row<'_>| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?));
        let rows = if let Some(cursor) = after {
            statement.query_map(
                params![
                    from_inclusive.0.to_be_bytes().as_slice(),
                    until_exclusive.0.to_be_bytes().as_slice(),
                    cursor.accepted_at.0.to_be_bytes().as_slice(),
                    cursor.generation.0.0.as_slice(),
                    i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX),
                ],
                row_mapper,
            )
        } else {
            statement.query_map(
                params![
                    from_inclusive.0.to_be_bytes().as_slice(),
                    until_exclusive.0.to_be_bytes().as_slice(),
                    i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX),
                ],
                row_mapper,
            )
        }
        .map_err(sql_error)?;
        let mut items = rows
            .map(|row| {
                let (payload, bytes) = row.map_err(sql_error)?;
                let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    StoreError::Integrity(format!(
                        "generation acceptance time has {} bytes",
                        bytes.len()
                    ))
                })?;
                let entry: GenerationHistoryEntry = decode(&payload)?;
                Ok(AcceptedGeneration {
                    accepted_at: AcceptanceTime(u64::from_be_bytes(bytes)),
                    manifest: entry.manifest,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let next_cursor = if items.len() > limit {
            limit
                .checked_sub(1)
                .and_then(|last_index| items.get(last_index))
                .map(|item| AcceptedGenerationCursor {
                    accepted_at: item.accepted_at,
                    generation: item.manifest.generation,
                })
        } else {
            None
        };
        items.truncate(limit);
        Ok(AcceptedGenerationPage { items, next_cursor })
    }

    fn node_history(&self, id: NodeId) -> Result<Vec<NodeHistoryVersion>, StoreError> {
        read_node_history(&self.connection, id)
    }

    fn fact_history(&self, fact: FactRef) -> Result<Vec<FactHistoryVersion>, StoreError> {
        read_fact_history(&self.connection, fact)
    }

    fn fact_versions_changed_at(
        &self,
        fact: FactRef,
        generation: GenerationId,
    ) -> Result<Vec<FactHistoryEntry>, StoreError> {
        read_fact_versions_changed_at(&self.connection, fact, generation)
    }

    fn fact_version_changes_at_page(
        &self,
        generation: GenerationId,
        after: Option<FactVersionChangeCursor>,
        limit: usize,
    ) -> Result<FactVersionChangePage, StoreError> {
        read_fact_version_changes_at_page(&self.connection, generation, after, limit)
    }

    fn fact_history_page(
        &self,
        as_of_generation: GenerationId,
        after: Option<FactHistoryCursor>,
        limit: usize,
    ) -> Result<FactHistoryPage, StoreError> {
        read_fact_history_page(&self.connection, as_of_generation, after, limit)
    }

    fn observed_facts_between(
        &self,
        from_inclusive: ObservationTime,
        until_exclusive: ObservationTime,
        after: Option<ObservedFactCursor>,
        limit: usize,
    ) -> Result<ObservedFactPage, StoreError> {
        read_observed_facts_between(
            &self.connection,
            from_inclusive,
            until_exclusive,
            after,
            limit,
        )
    }

    fn changes_between(
        &self,
        from: GenerationId,
        to: GenerationId,
    ) -> Result<Vec<GenerationChange>, StoreError> {
        read_changes_between(&self.connection, from, to)
    }

    fn changes_between_page(
        &self,
        from: GenerationId,
        to: GenerationId,
        after: Option<GenerationChangeCursor>,
        limit: usize,
    ) -> Result<GenerationChangePage, StoreError> {
        read_changes_between_page(&self.connection, from, to, after, limit)
    }

    fn historical_snapshot(&self, generation: GenerationId) -> Result<GraphSnapshot, StoreError> {
        read_graph_at(&self.connection, generation)
    }

    fn historical_node(
        &self,
        generation: GenerationId,
        id: NodeId,
    ) -> Result<Option<Node>, StoreError> {
        read_temporal_evidence(&self.connection, generation, NODE_FACT, id.0)
    }

    fn historical_file(
        &self,
        generation: GenerationId,
        id: FileId,
    ) -> Result<Option<FileVersion>, StoreError> {
        read_temporal_evidence(&self.connection, generation, FILE_FACT, id.0)
    }

    fn historical_provenance(
        &self,
        generation: GenerationId,
        id: ProvenanceId,
    ) -> Result<Option<Provenance>, StoreError> {
        read_temporal_evidence(&self.connection, generation, PROVENANCE_FACT, id.0)
    }

    fn historical_search_nodes(
        &self,
        generation: GenerationId,
        text: &str,
        limit: usize,
    ) -> Result<Vec<Node>, StoreError> {
        super::tree_store::historical_search_nodes(&self.connection, generation, text, limit)
    }

    fn historical_files_page(
        &self,
        generation: GenerationId,
        after: Option<syntaxmesh_core::FileId>,
        limit: usize,
    ) -> Result<syntaxmesh_store::HistoricalFilePage, StoreError> {
        super::file_pages::historical_files_page(&self.connection, generation, after, limit)
    }

    fn historical_nodes_page(
        &self,
        generation: GenerationId,
        after: Option<NodeId>,
        limit: usize,
    ) -> Result<syntaxmesh_store::HistoricalNodePage, StoreError> {
        super::tree_store::historical_nodes_page(&self.connection, generation, after, limit)
    }

    fn historical_incident_edges(
        &self,
        generation: GenerationId,
        endpoint: NodeId,
        direction: EdgeDirection,
        after: Option<EdgeId>,
        limit: usize,
    ) -> Result<HistoricalEdgePage, StoreError> {
        historical_incident_edge_page(
            &self.connection,
            generation,
            endpoint,
            direction,
            after,
            limit,
        )
    }

    fn apply_delta(&mut self, delta: GraphDelta) -> Result<GenerationManifest, StoreError> {
        self.apply_delta_with_acceptance_time(delta, None)
    }

    fn apply_delta_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut wrapper_profile = TransactionProfile::new();
        let expected_database_manifest = self.memory.current_manifest();
        let candidate = self.memory.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("candidate_store_clone");
        let (candidate, manifest) =
            candidate.stage_delta_with_lineage(request.clone(), accepted_at)?;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("candidate_graph_delta");
        self.persist(
            &candidate,
            expected_database_manifest,
            Some(&request.graph),
            accepted_at,
        )?;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("persist_transaction");
        self.memory = candidate;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("publish_candidate_to_handle");
        Ok(manifest)
    }

    fn apply_delta_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut wrapper_profile = TransactionProfile::new();
        let expected_database_manifest = self.memory.current_manifest();
        let candidate = self.memory.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("candidate_store_clone");
        let (candidate, manifest) =
            candidate.stage_delta_with_consequences(request.clone(), accepted_at)?;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("candidate_graph_delta");
        self.persist(
            &candidate,
            expected_database_manifest,
            Some(&request.publication.graph),
            accepted_at,
        )?;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("persist_transaction");
        self.memory = candidate;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("publish_candidate_to_handle");
        Ok(manifest)
    }

    fn apply_delta_with_acceptance_time(
        &mut self,
        delta: GraphDelta,
        accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        #[cfg(feature = "benchmark-instrumentation")]
        let mut wrapper_profile = TransactionProfile::new();
        let expected_database_manifest = self.memory.current_manifest();
        let mut candidate = self.memory.clone();
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("candidate_store_clone");
        let manifest = candidate.apply_delta(delta.clone())?;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("candidate_graph_delta");
        self.persist(
            &candidate,
            expected_database_manifest,
            Some(&delta),
            accepted_at,
        )?;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("persist_transaction");
        self.memory = candidate;
        #[cfg(feature = "benchmark-instrumentation")]
        wrapper_profile.mark("publish_candidate_to_handle");
        Ok(manifest)
    }

    fn set_generation_status(
        &mut self,
        generation: GenerationId,
        status: GenerationStatus,
    ) -> Result<GenerationManifest, StoreError> {
        let expected_database_manifest = self.memory.current_manifest();
        let mut candidate = self.memory.clone();
        let manifest = candidate.set_generation_status(generation, status)?;
        self.persist(&candidate, expected_database_manifest, None, None)?;
        self.memory = candidate;
        Ok(manifest)
    }

    fn node(&self, generation: GenerationId, id: NodeId) -> Result<Option<Node>, StoreError> {
        self.require_current_generation(generation)?;
        let node = self.memory.node(generation, id)?;
        self.require_current_generation(generation)?;
        Ok(node)
    }

    fn nodes(&self, generation: GenerationId) -> Result<Vec<Node>, StoreError> {
        self.require_current_generation(generation)?;
        let nodes = self.memory.nodes(generation)?;
        self.require_current_generation(generation)?;
        Ok(nodes)
    }

    fn symbol_candidates(
        &self,
        generation: GenerationId,
        exact_names: &[String],
        terminal_names: &[String],
    ) -> Result<Vec<Node>, StoreError> {
        self.require_current_generation(generation)?;
        let nodes = self
            .memory
            .symbol_candidates(generation, exact_names, terminal_names)?;
        self.require_current_generation(generation)?;
        Ok(nodes)
    }

    fn files(
        &self,
        generation: GenerationId,
    ) -> Result<Vec<syntaxmesh_core::FileVersion>, StoreError> {
        self.require_current_generation(generation)?;
        let files = self.memory.files(generation)?;
        self.require_current_generation(generation)?;
        Ok(files)
    }

    fn edges(&self, generation: GenerationId) -> Result<Vec<Edge>, StoreError> {
        self.require_current_generation(generation)?;
        let edges = self.memory.edges(generation)?;
        self.require_current_generation(generation)?;
        Ok(edges)
    }

    fn provenance(&self, generation: GenerationId) -> Result<Vec<Provenance>, StoreError> {
        self.require_current_generation(generation)?;
        let provenance = self.memory.provenance(generation)?;
        self.require_current_generation(generation)?;
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
            records.extend(read_many_with_params::<Provenance, _>(
                &self.connection,
                &sql,
                rusqlite::params_from_iter(batch.iter().map(|id| id.0.0.to_vec())),
            )?);
        }
        records.sort_unstable_by_key(|provenance| provenance.id);
        Ok(records)
    }

    fn outgoing(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        self.require_current_generation(generation)?;
        let edges = self.memory.outgoing(generation, id)?;
        self.require_current_generation(generation)?;
        Ok(edges)
    }

    fn incoming(&self, generation: GenerationId, id: NodeId) -> Result<Vec<Edge>, StoreError> {
        self.require_current_generation(generation)?;
        let edges = self.memory.incoming(generation, id)?;
        self.require_current_generation(generation)?;
        Ok(edges)
    }

    fn nodes_for_file(
        &self,
        generation: GenerationId,
        file: syntaxmesh_core::FileId,
    ) -> Result<Vec<NodeId>, StoreError> {
        self.require_current_generation(generation)?;
        let nodes = self.memory.nodes_for_file(generation, file)?;
        self.require_current_generation(generation)?;
        Ok(nodes)
    }
}
