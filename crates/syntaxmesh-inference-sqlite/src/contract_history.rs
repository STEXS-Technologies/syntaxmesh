use crate::{InferenceMaterializer, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use syntaxmesh_core::{GenerationId, RepositoryId, StableId, WorktreeId};
use syntaxmesh_inference::{Contract, ContractEvaluation, ContractLimits};
use syntaxmesh_query::Query;
use syntaxmesh_store::GraphStore;

/// Complete pinned contract input admitted for downstream historical retention.
pub struct ContractHistoryRequest<'contracts> {
    pub generation: GenerationId,
    pub contracts: &'contracts [Contract],
    pub limits: &'contracts ContractLimits,
    pub history_scan_budget: usize,
    pub output_bytes: usize,
}

/// Receipt for one complete generation-scoped contract evaluation set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractHistoryWatermark {
    pub operation: StableId,
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub generation: GenerationId,
    pub generation_sequence: u64,
    pub result_count: usize,
}

/// Authored contract and its exact retained-generation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedContractEvaluation {
    pub contract: Contract,
    pub evaluation: ContractEvaluation,
    pub generation_sequence: u64,
}

/// Latest complete retained declaration set for a repository/worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedContractSet {
    pub watermark: ContractHistoryWatermark,
    pub contracts: Vec<Contract>,
}

/// Compact run of one outcome across retained evaluations for a contract version.
/// These are observation bounds, not a claim about unevaluated generations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOutcomeEpisode {
    pub contract_id: String,
    pub contract_version: String,
    pub outcome: syntaxmesh_inference::ContractOutcome,
    pub first_generation: GenerationId,
    pub first_sequence: u64,
    pub last_generation: GenerationId,
    pub last_sequence: u64,
}

/// Select an outcome episode using the exact coordinates of a retained evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractOutcomeEpisodeQuery {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub evaluation_sequence: u64,
    pub evaluation_generation: GenerationId,
    pub expected_outcome: syntaxmesh_inference::ContractOutcome,
    pub contract_id: String,
    pub contract_version: String,
}

/// Keyset cursor scoped to one contract outcome timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOutcomeTimelineCursor {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub contract_id: String,
    pub contract_version: String,
    pub first_sequence: u64,
}

/// Bounded query for the retained outcome episodes of one contract version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractOutcomeTimelineQuery {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub contract_id: String,
    pub contract_version: String,
    pub after: Option<ContractOutcomeTimelineCursor>,
    pub limit: usize,
}

/// Chronological page of observed outcome episodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOutcomeTimelinePage {
    pub items: Vec<ContractOutcomeEpisode>,
    pub next_cursor: Option<ContractOutcomeTimelineCursor>,
}

#[derive(Clone, Copy)]
struct OutcomeEpisodeAdvance<'request> {
    repository: &'request str,
    worktree: &'request str,
    contract_id: &'request str,
    contract_version: &'request str,
    outcome: &'request str,
    generation: &'request str,
    sequence: i64,
}

/// Keyset cursor for one contract's chronological result history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractHistoryCursor {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub contract_id: String,
    pub contract_version: String,
    pub generation_sequence: u64,
    pub generation: GenerationId,
}

/// Bounded input for an indexed chronological contract-result read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractHistoryQuery {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub contract_id: String,
    pub contract_version: String,
    pub after: Option<ContractHistoryCursor>,
    pub limit: usize,
}

/// Ordered page of materialized results; no canonical-history replay occurs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractHistoryPage {
    pub items: Vec<RetainedContractEvaluation>,
    pub next_cursor: Option<ContractHistoryCursor>,
}

impl InferenceMaterializer {
    /// Read indexed outcome episodes in bounded chronological pages.
    ///
    /// Every returned episode boundary is checked against the retained result
    /// at that generation. This is a latest-history timeline, not an as-of
    /// query; use `contract_outcome_episode_at` for generation-pinned answers.
    ///
    /// # Errors
    /// Rejects invalid page bounds, cross-scope cursors, and inconsistent rows.
    pub fn contract_outcome_timeline(
        &self,
        query: &ContractOutcomeTimelineQuery,
    ) -> Result<ContractOutcomeTimelinePage> {
        self.contract_outcome_timeline_filtered(query, None)
    }

    /// Read only observed contradicted episodes from the indexed outcome projection.
    /// Insufficient evidence is deliberately not classified as a violation.
    ///
    /// # Errors
    /// Rejects invalid bounds, cross-scope cursors, and inconsistent retained rows.
    pub fn contract_violation_timeline(
        &self,
        query: &ContractOutcomeTimelineQuery,
    ) -> Result<ContractOutcomeTimelinePage> {
        self.contract_outcome_timeline_filtered(
            query,
            Some(syntaxmesh_inference::ContractOutcome::Contradicted),
        )
    }

    fn contract_outcome_timeline_filtered(
        &self,
        query: &ContractOutcomeTimelineQuery,
        outcome_filter: Option<syntaxmesh_inference::ContractOutcome>,
    ) -> Result<ContractOutcomeTimelinePage> {
        if query.limit == 0 || query.limit > 1_000 {
            return Err("invalid contract outcome timeline page limit".into());
        }
        if query.after.as_ref().is_some_and(|cursor| {
            cursor.repository != query.repository
                || cursor.worktree != query.worktree
                || cursor.contract_id != query.contract_id
                || cursor.contract_version != query.contract_version
        }) {
            return Err("contract outcome timeline cursor scope mismatch".into());
        }
        if let Some(cursor) = &query.after {
            let exists: bool = self.connection.query_row(
                "SELECT EXISTS (SELECT 1 FROM inference_contract_outcome_episodes
                 WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
                   AND contract_version = ?4 AND first_sequence = ?5
                   AND (?6 IS NULL OR outcome = ?6))",
                params![
                    query.repository.0.to_hex(),
                    query.worktree.0.to_hex(),
                    query.contract_id,
                    query.contract_version,
                    i64::try_from(cursor.first_sequence)?,
                    outcome_filter.map(outcome_key)
                ],
                |row| row.get(0),
            )?;
            if !exists {
                return Err("contract outcome timeline cursor is not a retained position".into());
            }
        }
        let sql = if outcome_filter.is_some() {
            "SELECT outcome, first_generation, first_sequence, last_generation, last_sequence
             FROM inference_contract_outcome_episodes
             WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
               AND contract_version = ?4 AND outcome = 'contradicted'
               AND (?5 IS NULL OR first_sequence > ?5)
             ORDER BY first_sequence LIMIT ?6"
        } else {
            "SELECT outcome, first_generation, first_sequence, last_generation, last_sequence
             FROM inference_contract_outcome_episodes
             WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
               AND contract_version = ?4 AND (?5 IS NULL OR first_sequence > ?5)
             ORDER BY first_sequence LIMIT ?6"
        };
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query(params![
            query.repository.0.to_hex(),
            query.worktree.0.to_hex(),
            query.contract_id,
            query.contract_version,
            query
                .after
                .as_ref()
                .map(|cursor| i64::try_from(cursor.first_sequence))
                .transpose()?,
            i64::try_from(
                query
                    .limit
                    .checked_add(1)
                    .ok_or("timeline limit overflow")?
            )?,
        ])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let outcome = parse_outcome(&row.get::<_, String>(0)?)?;
            let first_generation = parse_generation(&row.get::<_, String>(1)?)?;
            let first_sequence = u64::try_from(row.get::<_, i64>(2)?)?;
            let last_generation = parse_generation(&row.get::<_, String>(3)?)?;
            let last_sequence = u64::try_from(row.get::<_, i64>(4)?)?;
            if first_sequence > last_sequence {
                return Err("contract outcome episode has reversed sequence bounds".into());
            }
            for (generation, sequence) in [
                (first_generation, first_sequence),
                (last_generation, last_sequence),
            ] {
                let retained = self
                    .contract_result_at(
                        query.repository,
                        query.worktree,
                        generation,
                        &query.contract_id,
                        &query.contract_version,
                    )?
                    .ok_or("contract outcome timeline boundary has no retained result")?;
                if retained.generation_sequence != sequence
                    || retained.evaluation.outcome != outcome
                {
                    return Err(
                        "contract outcome timeline boundary differs from retained result".into(),
                    );
                }
            }
            items.push(ContractOutcomeEpisode {
                contract_id: query.contract_id.clone(),
                contract_version: query.contract_version.clone(),
                outcome,
                first_generation,
                first_sequence,
                last_generation,
                last_sequence,
            });
        }
        let next_cursor = if items.len() > query.limit {
            items.truncate(query.limit);
            items.last().map(|episode| ContractOutcomeTimelineCursor {
                repository: query.repository,
                worktree: query.worktree,
                contract_id: query.contract_id.clone(),
                contract_version: query.contract_version.clone(),
                first_sequence: episode.first_sequence,
            })
        } else {
            None
        };
        Ok(ContractOutcomeTimelinePage { items, next_cursor })
    }

    /// Backfill the compact outcome projection while applying migration 7.
    pub(crate) fn backfill_outcome_episodes(connection: &rusqlite::Connection) -> Result<()> {
        let mut sets_statement = connection.prepare(
            "SELECT repository, worktree, operation, generation, generation_sequence,
                    contract_count, payload_digest
             FROM inference_contract_evaluation_sets
             ORDER BY repository, worktree, generation_sequence, generation",
        )?;
        let sets = sets_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(sets_statement);
        for (
            repository,
            worktree,
            operation,
            generation,
            sequence,
            expected_count,
            expected_digest,
        ) in sets
        {
            let peers: i64 = connection.query_row(
                "SELECT COUNT(*) FROM inference_contract_evaluation_sets
                 WHERE repository = ?1 AND worktree = ?2 AND generation_sequence = ?3",
                rusqlite::params![repository, worktree, sequence],
                |row| row.get(0),
            )?;
            if peers != 1 {
                return Err(
                    "cannot backfill outcome episodes from ambiguous retained contract sets".into(),
                );
            }
            let mut rows_statement = connection.prepare(
                "SELECT contract_id, contract_version, evaluation_id, payload, payload_digest,
                        generation, generation_sequence
                 FROM inference_contract_evaluations WHERE operation = ?1
                 ORDER BY contract_id, contract_version",
            )?;
            let rows = rows_statement
                .query_map([&operation], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(rows_statement);
            if i64::try_from(rows.len())? != expected_count {
                return Err("contract outcome backfill found an incomplete retained set".into());
            }
            let mut digest_entries = Vec::with_capacity(rows.len());
            let mut decoded = Vec::with_capacity(rows.len());
            for (
                contract_id,
                contract_version,
                evaluation_id,
                payload,
                digest,
                row_generation,
                row_sequence,
            ) in rows
            {
                let retained = decode_payload(sequence, &payload, &digest)?;
                if retained.contract.id != contract_id
                    || retained.contract.version != contract_version
                    || retained.evaluation.generation != parse_generation(&generation)?
                    || row_generation != generation
                    || row_sequence != sequence
                    || retained.evaluation.id.to_hex() != evaluation_id
                {
                    return Err(
                        "contract outcome backfill identity differs from retained row".into(),
                    );
                }
                digest_entries.push((
                    contract_id.clone(),
                    contract_version.clone(),
                    retained.evaluation.id,
                    blake3::hash(&payload).to_hex().to_string(),
                ));
                decoded.push((contract_id, contract_version, retained));
            }
            let digest_bytes = bincode::serialize(&digest_entries)?;
            if blake3::hash(&digest_bytes).to_hex().as_str() != expected_digest {
                return Err("contract outcome backfill set checksum mismatch".into());
            }
            let active = decoded
                .iter()
                .map(|(contract_id, contract_version, _)| {
                    (contract_id.clone(), contract_version.clone())
                })
                .collect::<BTreeSet<_>>();
            close_omitted_episodes(
                connection,
                &repository,
                &worktree,
                &generation,
                sequence,
                &active,
            )?;
            for (contract_id, contract_version, retained) in decoded {
                advance_outcome_episode(
                    connection,
                    OutcomeEpisodeAdvance {
                        repository: &repository,
                        worktree: &worktree,
                        contract_id: &contract_id,
                        contract_version: &contract_version,
                        outcome: outcome_key(retained.evaluation.outcome),
                        generation: &generation,
                        sequence,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Evaluate and atomically retain one complete contract batch for a pinned generation.
    ///
    /// This downstream projection is independent of canonical graph publication.
    /// Repeating an identical operation is idempotent; conflicting retained
    /// contract/version/generation results fail without replacing prior history.
    ///
    /// # Errors
    /// Rejects invalid bounds, malformed selection history, conflicting repeats,
    /// or SQL failures without publishing a partial result set.
    pub fn retain_contract_history<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        request: &ContractHistoryRequest<'_>,
    ) -> Result<ContractHistoryWatermark> {
        if request.contracts.is_empty()
            || request.history_scan_budget == 0
            || request.output_bytes == 0
        {
            return Err("non-empty contracts and positive history/output budgets required".into());
        }
        let manifest = store.manifest(request.generation)?;
        let sequence = store.generation_sequence(request.generation)?;
        let evaluations = Query::new(store, request.generation).evaluate_contracts(
            request.contracts,
            request.limits,
            request.history_scan_budget,
        )?;
        let mut contracts = request.contracts.iter().collect::<Vec<_>>();
        contracts
            .sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
        let operation_input = bincode::serialize(&(
            manifest.repository,
            manifest.worktree,
            request.generation,
            &contracts,
        ))?;
        let operation = StableId::derive("contract-history-operation-v1", &[&operation_input]);
        let watermark = ContractHistoryWatermark {
            operation,
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: request.generation,
            generation_sequence: sequence,
            result_count: evaluations.len(),
        };
        let mut payloads = Vec::with_capacity(evaluations.len());
        let mut remaining = request.output_bytes;
        for (contract, evaluation) in contracts.into_iter().zip(evaluations) {
            let payload = bincode::serialize(&(contract, &evaluation))?;
            remaining = remaining
                .checked_sub(payload.len())
                .ok_or("contract history output byte budget exhausted")?;
            payloads.push((
                contract.id.clone(),
                contract.version.clone(),
                evaluation,
                payload,
            ));
        }
        let digest_bytes = bincode::serialize(
            &payloads
                .iter()
                .map(|(id, version, evaluation, payload)| {
                    (
                        id,
                        version,
                        evaluation.id,
                        blake3::hash(payload).to_hex().to_string(),
                    )
                })
                .collect::<Vec<_>>(),
        )?;
        let payload_digest = blake3::hash(&digest_bytes).to_hex().to_string();
        self.publish_contract_history(watermark, &payloads, &payload_digest)?;
        Ok(watermark)
    }

    /// Read and integrity-check the latest complete retained declaration set.
    ///
    /// This is the source input for explicit incremental catch-up. It never
    /// infers a set from a partial page of results.
    ///
    /// # Errors
    /// Returns errors for incomplete sets, tampered rows/digests, or malformed
    /// generation and operation identities.
    pub fn latest_contract_set(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Option<RetainedContractSet>> {
        let header = self
            .connection
            .query_row(
                "SELECT operation, generation, generation_sequence, contract_count, payload_digest
                 FROM inference_contract_evaluation_sets
                 WHERE repository = ?1 AND worktree = ?2
                 ORDER BY generation_sequence DESC, generation DESC LIMIT 1",
                params![repository.0.to_hex(), worktree.0.to_hex()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((operation, generation, sequence, expected_count, expected_digest)) = header
        else {
            return Ok(None);
        };
        let generation = parse_generation(&generation)?;
        let sequence = u64::try_from(sequence)?;
        let expected_count = usize::try_from(expected_count)?;
        let stored_operation = operation.clone();
        let peer_sets: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM inference_contract_evaluation_sets
             WHERE repository = ?1 AND worktree = ?2 AND generation_sequence = ?3",
            params![
                repository.0.to_hex(),
                worktree.0.to_hex(),
                i64::try_from(sequence)?
            ],
            |row| row.get(0),
        )?;
        if peer_sets != 1 {
            return Err("latest retained generation has ambiguous contract sets".into());
        }
        let mut statement = self.connection.prepare(
            "SELECT contract_id, contract_version, evaluation_id, payload, payload_digest,
                    generation, generation_sequence
             FROM inference_contract_evaluations WHERE operation = ?1
             ORDER BY contract_id, contract_version",
        )?;
        let mut rows = statement.query([&operation])?;
        let mut retained = Vec::with_capacity(expected_count);
        let mut digest_rows = Vec::with_capacity(expected_count);
        while let Some(row) = rows.next()? {
            let id = row.get::<_, String>(0)?;
            let version = row.get::<_, String>(1)?;
            let evaluation_id = row.get::<_, String>(2)?;
            let payload = row.get::<_, Vec<u8>>(3)?;
            let payload_digest = row.get::<_, String>(4)?;
            let row_generation = parse_generation(&row.get::<_, String>(5)?)?;
            let row_sequence = u64::try_from(row.get::<_, i64>(6)?)?;
            let item = decode_payload(i64::try_from(row_sequence)?, &payload, &payload_digest)?;
            if item.contract.id != id
                || item.contract.version != version
                || item.evaluation.id.to_hex() != evaluation_id
                || item.evaluation.generation != generation
                || row_generation != generation
                || row_sequence != sequence
            {
                return Err("retained contract set row differs from its indexed identity".into());
            }
            digest_rows.push((
                id,
                version,
                item.evaluation.id,
                blake3::hash(&payload).to_hex().to_string(),
            ));
            retained.push(item.contract);
        }
        if retained.len() != expected_count {
            return Err("retained contract set is incomplete".into());
        }
        let digest_bytes = bincode::serialize(&digest_rows)?;
        if blake3::hash(&digest_bytes).to_hex().as_str() != expected_digest {
            return Err("retained contract set aggregate digest mismatch".into());
        }
        let operation_input = bincode::serialize(&(repository, worktree, generation, &retained))?;
        let expected_operation =
            StableId::derive("contract-history-operation-v1", &[&operation_input]);
        if expected_operation.to_hex() != stored_operation {
            return Err("retained contract set operation identity mismatch".into());
        }
        Ok(Some(RetainedContractSet {
            watermark: ContractHistoryWatermark {
                operation: expected_operation,
                repository,
                worktree,
                generation,
                generation_sequence: sequence,
                result_count: expected_count,
            },
            contracts: retained,
        }))
    }

    /// Read one exact already-materialized contract result by generation.
    ///
    /// # Errors
    /// Returns an error for inconsistent stored payloads or checksums.
    pub fn contract_result_at(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
        generation: GenerationId,
        contract_id: &str,
        contract_version: &str,
    ) -> Result<Option<RetainedContractEvaluation>> {
        let row = self.connection.query_row(
            "SELECT generation_sequence, evaluation_id, payload, payload_digest FROM inference_contract_evaluations
             WHERE repository = ?1 AND worktree = ?2 AND generation = ?3 AND contract_id = ?4 AND contract_version = ?5",
            params![repository.0.to_hex(), worktree.0.to_hex(), generation.0.to_hex(), contract_id, contract_version],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Vec<u8>>(2)?, row.get::<_, String>(3)?)),
        ).optional()?;
        row.map(|(sequence, id, payload, digest)| {
            let item = decode_payload(sequence, &payload, &digest)?;
            if item.evaluation.id.to_hex() != id || item.evaluation.generation != generation {
                return Err("contract result key differs from retained payload".into());
            }
            Ok(item)
        })
        .transpose()
    }

    /// Read the latest retained result for one contract at or before a canonical sequence.
    ///
    /// The returned generation is the generation actually evaluated; callers must not treat an
    /// older result as compliance at the requested sequence.
    ///
    /// # Errors
    /// Returns an error for invalid identities or inconsistent stored payloads/checksums.
    pub fn contract_result_at_or_before(
        &self,
        repository: RepositoryId,
        worktree: WorktreeId,
        through_sequence: u64,
        contract_id: &str,
        contract_version: &str,
    ) -> Result<Option<RetainedContractEvaluation>> {
        if contract_id.is_empty() || contract_version.is_empty() {
            return Err("contract identity must not be empty".into());
        }
        let row = self
            .connection
            .query_row(
                "SELECT generation_sequence, generation, evaluation_id, payload, payload_digest
             FROM inference_contract_evaluations
             WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
               AND contract_version = ?4 AND generation_sequence <= ?5
             ORDER BY generation_sequence DESC, generation DESC LIMIT 1",
                params![
                    repository.0.to_hex(),
                    worktree.0.to_hex(),
                    contract_id,
                    contract_version,
                    i64::try_from(through_sequence)?
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(sequence, generation, evaluation_id, payload, digest)| {
            let generation = parse_generation(&generation)?;
            let item = decode_payload(sequence, &payload, &digest)?;
            if item.evaluation.generation != generation
                || item.evaluation.id.to_hex() != evaluation_id
                || item.contract.id != contract_id
                || item.contract.version != contract_version
            {
                return Err("as-of contract result key differs from retained payload".into());
            }
            Ok(item)
        })
        .transpose()
    }

    /// Read the compact outcome episode containing one retained evaluation.
    ///
    /// `evaluation_sequence` must be the sequence of the latest retained result
    /// selected for the requested as-of generation, not the target sequence.
    ///
    /// # Errors
    /// Rejects invalid bounds or episode rows that disagree with retained results.
    pub fn contract_outcome_episode_at(
        &self,
        query: &ContractOutcomeEpisodeQuery,
    ) -> Result<Option<ContractOutcomeEpisode>> {
        let row = self
            .connection
            .query_row(
                "SELECT outcome, first_generation, first_sequence, last_generation, last_sequence,
                        ended_sequence
             FROM inference_contract_outcome_episodes
             WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
               AND contract_version = ?4 AND first_sequence <= ?5
               AND (ended_sequence IS NULL OR ended_sequence > ?5)
             ORDER BY first_sequence DESC LIMIT 1",
                rusqlite::params![
                    query.repository.0.to_hex(),
                    query.worktree.0.to_hex(),
                    query.contract_id,
                    query.contract_version,
                    i64::try_from(query.evaluation_sequence)?
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(
                outcome,
                first_generation,
                first_sequence,
                last_generation,
                last_sequence,
                ended_sequence,
            )| {
                let outcome = parse_outcome(&outcome)?;
                let first_sequence = u64::try_from(first_sequence)?;
                let stored_last_sequence = u64::try_from(last_sequence)?;
                if first_sequence > query.evaluation_sequence
                    || stored_last_sequence < query.evaluation_sequence
                    || ended_sequence.is_some_and(|ended| {
                        u64::try_from(ended).is_ok_and(|value| value <= query.evaluation_sequence)
                    })
                    || outcome != query.expected_outcome
                {
                    return Err(
                        "contract outcome episode bounds do not contain selected evaluation".into(),
                    );
                }
                let first_generation = parse_generation(&first_generation)?;
                let last_generation = parse_generation(&last_generation)?;
                for (generation, sequence) in [
                    (first_generation, first_sequence),
                    (last_generation, stored_last_sequence),
                ] {
                    let retained = self
                        .contract_result_at(
                            query.repository,
                            query.worktree,
                            generation,
                            &query.contract_id,
                            &query.contract_version,
                        )?
                        .ok_or("contract outcome episode boundary has no retained result")?;
                    if retained.generation_sequence != sequence
                        || retained.evaluation.outcome != outcome
                    {
                        return Err(
                            "contract outcome episode boundary differs from retained result".into(),
                        );
                    }
                }
                Ok(ContractOutcomeEpisode {
                    contract_id: query.contract_id.clone(),
                    contract_version: query.contract_version.clone(),
                    outcome,
                    first_generation,
                    first_sequence,
                    last_generation: query.evaluation_generation,
                    last_sequence: query.evaluation_sequence,
                })
            },
        )
        .transpose()
    }

    /// Read a bounded chronological page for one stable contract identity/version.
    ///
    /// # Errors
    /// Rejects invalid page bounds/cursors or malformed retained payloads.
    pub fn contract_history(&self, query: &ContractHistoryQuery) -> Result<ContractHistoryPage> {
        if query.limit == 0 || query.limit > 1_000 {
            return Err("invalid contract history page limit".into());
        }
        if query.after.as_ref().is_some_and(|cursor| {
            cursor.repository != query.repository
                || cursor.worktree != query.worktree
                || cursor.contract_id != query.contract_id
                || cursor.contract_version != query.contract_version
        }) {
            return Err("contract history cursor scope mismatch".into());
        }
        if let Some(cursor) = &query.after {
            let exists: bool = self.connection.query_row(
                "SELECT EXISTS (SELECT 1 FROM inference_contract_evaluations
                 WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3 AND contract_version = ?4
                   AND generation_sequence = ?5 AND generation = ?6)",
                params![query.repository.0.to_hex(), query.worktree.0.to_hex(), query.contract_id, query.contract_version,
                    i64::try_from(cursor.generation_sequence)?, cursor.generation.0.to_hex()],
                |row| row.get(0),
            )?;
            if !exists {
                return Err("contract history cursor is not a retained position".into());
            }
        }
        let mut statement = self.connection.prepare(
            "SELECT generation_sequence, generation, evaluation_id, payload, payload_digest
             FROM inference_contract_evaluations
             WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3 AND contract_version = ?4
               AND (?5 IS NULL OR generation_sequence > ?5 OR (generation_sequence = ?5 AND generation > ?6))
             ORDER BY generation_sequence, generation LIMIT ?7",
        )?;
        let mut rows = statement.query(params![
            query.repository.0.to_hex(),
            query.worktree.0.to_hex(),
            query.contract_id,
            query.contract_version,
            query
                .after
                .as_ref()
                .map(|cursor| i64::try_from(cursor.generation_sequence))
                .transpose()?,
            query
                .after
                .as_ref()
                .map(|cursor| cursor.generation.0.to_hex()),
            i64::try_from(
                query
                    .limit
                    .checked_add(1)
                    .ok_or("contract history limit overflow")?
            )?
        ])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let sequence = row.get::<_, i64>(0)?;
            let generation = parse_generation(&row.get::<_, String>(1)?)?;
            let evaluation_id = row.get::<_, String>(2)?;
            let payload = row.get::<_, Vec<u8>>(3)?;
            let digest = row.get::<_, String>(4)?;
            let item = decode_payload(sequence, &payload, &digest)?;
            if item.evaluation.generation != generation
                || item.evaluation.id.to_hex() != evaluation_id
                || item.contract.id != query.contract_id
                || item.contract.version != query.contract_version
            {
                return Err("contract history key does not match retained payload".into());
            }
            items.push(item);
        }
        let next_cursor = if items.len() > query.limit {
            items.truncate(query.limit);
            items.last().map(|item| ContractHistoryCursor {
                repository: query.repository,
                worktree: query.worktree,
                contract_id: query.contract_id.clone(),
                contract_version: query.contract_version.clone(),
                generation_sequence: item.generation_sequence,
                generation: item.evaluation.generation,
            })
        } else {
            None
        };
        Ok(ContractHistoryPage { items, next_cursor })
    }

    fn publish_contract_history(
        &mut self,
        watermark: ContractHistoryWatermark,
        payloads: &[(String, String, ContractEvaluation, Vec<u8>)],
        digest: &str,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let operation = watermark.operation.to_hex();
        let existing: Option<(String, String, String, i64, i64, String)> = transaction.query_row(
            "SELECT repository, worktree, generation, generation_sequence, contract_count, payload_digest FROM inference_contract_evaluation_sets WHERE operation = ?1",
            [&operation],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        ).optional()?;
        if let Some(existing) = existing {
            if existing
                != (
                    watermark.repository.0.to_hex(),
                    watermark.worktree.0.to_hex(),
                    watermark.generation.0.to_hex(),
                    i64::try_from(watermark.generation_sequence)?,
                    i64::try_from(watermark.result_count)?,
                    digest.to_owned(),
                )
            {
                return Err(
                    "retained contract evaluation set differs from deterministic retry".into(),
                );
            }
            let stored_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM inference_contract_evaluations WHERE operation = ?1",
                [&operation],
                |row| row.get(0),
            )?;
            if stored_count != i64::try_from(payloads.len())? {
                return Err("retained contract evaluation set is incomplete".into());
            }
            for (contract_id, contract_version, _evaluation, payload) in payloads {
                let matches: bool = transaction.query_row(
                    "SELECT payload = ?4 AND payload_digest = ?5 FROM inference_contract_evaluations
                     WHERE operation = ?1 AND contract_id = ?2 AND contract_version = ?3",
                    params![operation, contract_id, contract_version, payload, blake3::hash(payload).to_hex().to_string()],
                    |row| row.get(0),
                )?;
                if !matches {
                    return Err(
                        "retained contract evaluation differs from deterministic retry".into(),
                    );
                }
            }
            transaction.commit()?;
            return Ok(());
        }
        transaction.execute(
            "INSERT INTO inference_contract_evaluation_sets VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                operation,
                watermark.repository.0.to_hex(),
                watermark.worktree.0.to_hex(),
                watermark.generation.0.to_hex(),
                i64::try_from(watermark.generation_sequence)?,
                i64::try_from(watermark.result_count)?,
                digest
            ],
        )?;
        let active = payloads
            .iter()
            .map(|(contract_id, contract_version, _, _)| {
                (contract_id.clone(), contract_version.clone())
            })
            .collect::<BTreeSet<_>>();
        close_omitted_episodes(
            &transaction,
            &watermark.repository.0.to_hex(),
            &watermark.worktree.0.to_hex(),
            &watermark.generation.0.to_hex(),
            i64::try_from(watermark.generation_sequence)?,
            &active,
        )?;
        for (contract_id, contract_version, evaluation, payload) in payloads {
            transaction.execute(
                "INSERT INTO inference_contract_evaluations VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![operation, watermark.repository.0.to_hex(), watermark.worktree.0.to_hex(), watermark.generation.0.to_hex(), i64::try_from(watermark.generation_sequence)?, contract_id, contract_version, evaluation.id.to_hex(), payload, blake3::hash(payload).to_hex().to_string()],
            )?;
            advance_outcome_episode(
                &transaction,
                OutcomeEpisodeAdvance {
                    repository: &watermark.repository.0.to_hex(),
                    worktree: &watermark.worktree.0.to_hex(),
                    contract_id,
                    contract_version,
                    outcome: outcome_key(evaluation.outcome),
                    generation: &watermark.generation.0.to_hex(),
                    sequence: i64::try_from(watermark.generation_sequence)?,
                },
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn close_omitted_episodes(
    connection: &rusqlite::Connection,
    repository: &str,
    worktree: &str,
    generation: &str,
    sequence: i64,
    active: &BTreeSet<(String, String)>,
) -> Result<()> {
    let mut statement = connection.prepare(
        "SELECT contract_id, contract_version FROM inference_contract_outcome_episodes
         WHERE repository = ?1 AND worktree = ?2 AND ended_sequence IS NULL",
    )?;
    let open = statement
        .query_map([repository, worktree], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    for (contract_id, contract_version) in open {
        if !active.contains(&(contract_id.clone(), contract_version.clone())) {
            connection.execute(
                "UPDATE inference_contract_outcome_episodes
                 SET ended_generation = ?5, ended_sequence = ?6
                 WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
                   AND contract_version = ?4 AND ended_sequence IS NULL",
                rusqlite::params![
                    repository,
                    worktree,
                    contract_id,
                    contract_version,
                    generation,
                    sequence
                ],
            )?;
        }
    }
    Ok(())
}

fn advance_outcome_episode(
    connection: &rusqlite::Connection,
    advance: OutcomeEpisodeAdvance<'_>,
) -> Result<()> {
    let OutcomeEpisodeAdvance {
        repository,
        worktree,
        contract_id,
        contract_version,
        outcome,
        generation,
        sequence,
    } = advance;
    let latest: Option<(String, i64)> = connection
        .query_row(
            "SELECT outcome, last_sequence FROM inference_contract_outcome_episodes
             WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
               AND contract_version = ?4 AND ended_sequence IS NULL
             ORDER BY first_sequence DESC LIMIT 1",
            rusqlite::params![repository, worktree, contract_id, contract_version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((latest_outcome, last_sequence)) = latest {
        if last_sequence >= sequence {
            return Err("contract outcome episodes must advance by generation sequence".into());
        }
        if latest_outcome == outcome {
            connection.execute(
                "UPDATE inference_contract_outcome_episodes
                 SET last_generation = ?5, last_sequence = ?6
                 WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
                   AND contract_version = ?4 AND ended_sequence IS NULL",
                rusqlite::params![
                    repository,
                    worktree,
                    contract_id,
                    contract_version,
                    generation,
                    sequence
                ],
            )?;
            return Ok(());
        }
        connection.execute(
            "UPDATE inference_contract_outcome_episodes
             SET ended_generation = ?5, ended_sequence = ?6
             WHERE repository = ?1 AND worktree = ?2 AND contract_id = ?3
               AND contract_version = ?4 AND ended_sequence IS NULL",
            rusqlite::params![
                repository,
                worktree,
                contract_id,
                contract_version,
                generation,
                sequence
            ],
        )?;
    }
    connection.execute(
        "INSERT INTO inference_contract_outcome_episodes (
            repository, worktree, contract_id, contract_version, outcome,
            first_generation, first_sequence, last_generation, last_sequence
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6, ?7)",
        rusqlite::params![
            repository,
            worktree,
            contract_id,
            contract_version,
            outcome,
            generation,
            sequence
        ],
    )?;
    Ok(())
}

const fn outcome_key(outcome: syntaxmesh_inference::ContractOutcome) -> &'static str {
    match outcome {
        syntaxmesh_inference::ContractOutcome::Supported => "supported",
        syntaxmesh_inference::ContractOutcome::Contradicted => "contradicted",
        syntaxmesh_inference::ContractOutcome::InsufficientEvidence => "insufficient_evidence",
    }
}

fn parse_outcome(value: &str) -> Result<syntaxmesh_inference::ContractOutcome> {
    match value {
        "supported" => Ok(syntaxmesh_inference::ContractOutcome::Supported),
        "contradicted" => Ok(syntaxmesh_inference::ContractOutcome::Contradicted),
        "insufficient_evidence" => Ok(syntaxmesh_inference::ContractOutcome::InsufficientEvidence),
        _ => Err("unknown retained contract outcome episode".into()),
    }
}

#[cfg(test)]
mod tests;

fn parse_generation(value: &str) -> Result<GenerationId> {
    let bytes: [u8; 32] = hex::decode(value)?
        .try_into()
        .map_err(|bytes: Vec<u8>| format!("generation ID has {} bytes", bytes.len()))?;
    Ok(GenerationId(StableId(bytes)))
}

fn decode_payload(
    sequence: i64,
    payload: &[u8],
    digest: &str,
) -> Result<RetainedContractEvaluation> {
    if digest != blake3::hash(payload).to_hex().as_str() {
        return Err("contract history payload checksum mismatch".into());
    }
    let (contract, evaluation): (Contract, ContractEvaluation) = bincode::deserialize(payload)?;
    if contract.id != evaluation.contract_id || contract.version != evaluation.contract_version {
        return Err("contract history payload identity mismatch".into());
    }
    Ok(RetainedContractEvaluation {
        contract,
        evaluation,
        generation_sequence: u64::try_from(sequence)?,
    })
}
