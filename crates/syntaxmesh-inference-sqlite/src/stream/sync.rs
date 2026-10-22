use super::checkpoint::{checkpoint, generation, verify_origin};
use super::{StreamCheckpoint, StreamOrigin};
use crate::{InferenceMaterializer, Result};
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{FactPayload, FactRef, FactVersionRef, GenerationId};
use syntaxmesh_inference::{
    BoundRuleSet, Claim, DiscoveryRange, InvalidationLimits, Limits, OntologyRegistry, Rule,
    rule_set_identity,
};
use syntaxmesh_query::Query;
use syntaxmesh_store::GraphStore;

pub struct StreamSyncRequest<'controls> {
    pub origin: StreamOrigin,
    pub through: GenerationId,
    pub rules: &'controls [Rule],
    pub limits: &'controls Limits,
    pub range_limits: &'controls InvalidationLimits,
    pub history_scan_budget: usize,
    pub rule_budget: usize,
    pub output_bytes: usize,
    pub premise_budget: usize,
    pub evidence_budget: usize,
    pub matched_claim_bytes: usize,
    pub page_budget: usize,
}

struct Affected {
    claim: Claim,
    reasons: BTreeSet<FactVersionRef>,
}

pub(super) const POSTINGS_SQL: &str =
    "SELECT DISTINCT c.claim, c.payload, c.payload_digest, c.born_generation, c.born_sequence
     FROM inference_stream_premises p JOIN inference_stream_claims c ON c.stream = p.stream AND c.claim = p.claim
     WHERE p.stream = ?1 AND p.fact = ?2 AND p.valid_from = ?3 AND c.closed_sequence IS NULL ORDER BY c.claim";

impl InferenceMaterializer {
    /// Atomically close and discover witnesses for exactly one accepted successor.
    /// Previously recorded targets return their receipt without rewinding coverage.
    ///
    /// # Errors
    /// Rejects changed configuration, skipped/reversed transitions, corrupt witnesses,
    /// exhausted independent bounds or SQL failures without partial publication.
    pub fn sync_stream<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        request: &StreamSyncRequest<'_>,
    ) -> Result<StreamCheckpoint> {
        self.sync_stream_inner(store, request, None)
    }

    /// Atomically synchronize a successor using registered seeded inference.
    /// Membership assertion retractions fail closed until validation dependencies
    /// can be retained by the stream index.
    ///
    /// # Errors
    /// Rejects invalid current ontology facts, membership retractions, skipped
    /// transitions, exhausted bounds or SQL failures without partial publication.
    pub fn sync_stream_registered<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        request: &StreamSyncRequest<'_>,
        registry: &OntologyRegistry,
        binding_budget: usize,
    ) -> Result<StreamCheckpoint> {
        let bound = registry.bind_rules(
            request.rules,
            request.rule_budget,
            binding_budget,
            request.limits.input_bytes,
        )?;
        let bound_request = StreamSyncRequest {
            origin: request.origin,
            through: request.through,
            rules: bound.rules(),
            limits: request.limits,
            range_limits: request.range_limits,
            history_scan_budget: request.history_scan_budget,
            rule_budget: request.rule_budget,
            output_bytes: request.output_bytes,
            premise_budget: request.premise_budget,
            evidence_budget: request.evidence_budget,
            matched_claim_bytes: request.matched_claim_bytes,
            page_budget: request.page_budget,
        };
        self.sync_stream_inner(
            store,
            &bound_request,
            Some((registry, &bound, binding_budget)),
        )
    }

    fn sync_stream_inner<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        request: &StreamSyncRequest<'_>,
        registered: Option<(&OntologyRegistry, &BoundRuleSet, usize)>,
    ) -> Result<StreamCheckpoint> {
        if request.page_budget == 0
            || request.output_bytes == 0
            || request.premise_budget == 0
            || request.evidence_budget == 0
            || request.matched_claim_bytes == 0
            || request.history_scan_budget == 0
            || request.range_limits.generations == 0
            || request.range_limits.versions == 0
            || request.range_limits.postings == 0
            || request.range_limits.claims == 0
            || request.limits.facts == 0
            || request.limits.edge_visits == 0
            || request.limits.depth == 0
            || request.limits.claims == 0
        {
            return Err("positive stream synchronization admission required".into());
        }
        if rule_set_identity(
            request.rules,
            request.rule_budget,
            request.limits.input_bytes,
        )? != request.origin.baseline.configuration
        {
            return Err("stream rule configuration changed; explicit rebuild required".into());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let head = verify_origin(&transaction, request.origin)?;
        let selected = store.manifest(request.through)?;
        let sequence = store.generation_sequence(request.through)?;
        let baseline = request.origin.baseline;
        let baseline_manifest = store.manifest(baseline.selection.generation)?;
        if selected.repository != baseline.selection.repository
            || selected.worktree != baseline.selection.worktree
            || baseline_manifest.repository != selected.repository
            || baseline_manifest.worktree != selected.worktree
            || store.generation_sequence(baseline.selection.generation)?
                != baseline.generation_sequence
        {
            return Err("stream canonical scope mismatch".into());
        }
        if let Some(retained) = checkpoint(&transaction, request.origin, request.through)? {
            let retained_root =
                super::tree::root(&transaction, request.origin.stream, request.through)?;
            if super::tree::ActiveTree::new(&transaction, request.page_budget)?
                .count(retained_root)?
                != retained.active_claims
            {
                return Err("retained stream root count differs from checkpoint".into());
            }
            if let Some(previous) = retained.previous
                && store.generation_sequence(previous)?.checked_add(1) != Some(sequence)
            {
                return Err("retained stream predecessor sequence mismatch".into());
            }
            if retained.sequence != sequence
                || retained.sequence > head.sequence
                || retained
                    .previous
                    .is_some_and(|previous| selected.parent != Some(previous))
                || retained.added_claims > request.limits.claims
                || retained.affected_claims > request.range_limits.claims
                || retained.closure_reasons > request.evidence_budget
            {
                return Err("retained stream checkpoint exceeds scope or admission".into());
            }
            transaction.commit()?;
            return Ok(retained);
        }
        if selected.parent != Some(head.selection.generation)
            || sequence
                != head
                    .sequence
                    .checked_add(1)
                    .ok_or("stream sequence overflow")?
            || store.generation_sequence(head.selection.generation)? != head.sequence
        {
            return Err("stream synchronization requires the immediate canonical successor".into());
        }
        let query = Query::new(store, request.through);
        let closed =
            query.inference_premise_closures(head.selection.generation, request.range_limits)?;
        for closure in closed.values() {
            if closure.generation != request.through || closure.generation_sequence != sequence {
                return Err("stream closure coordinate mismatch".into());
            }
        }
        let mut membership_retractions = BTreeSet::new();
        if let Some((registry, _bound, _binding_budget)) = registered {
            for reference in closed.keys() {
                if !matches!(reference.fact, FactRef::Edge(_)) {
                    continue;
                }
                let versions =
                    store.fact_versions_changed_at(reference.fact, reference.valid_from)?;
                if versions.len() > 2 {
                    return Err("malformed closed ontology membership lookup".into());
                }
                let mut matching = versions.into_iter().filter(|entry| {
                    entry.fact == reference.fact && entry.version.valid_from == reference.valid_from
                });
                let entry = matching
                    .next()
                    .ok_or("closed inference premise version missing")?;
                if matching.next().is_some()
                    || entry.version.valid_until != Some(request.through)
                    || !matches!(&entry.version.payload, FactPayload::Edge(edge) if reference.fact == FactRef::Edge(edge.id))
                {
                    return Err("malformed closed ontology membership version".into());
                }
                if let FactPayload::Edge(edge) = &entry.version.payload
                    && registry.is_instance_membership_relation(&edge.relation)
                {
                    membership_retractions.insert(*reference);
                }
            }
        }
        let mut affected = BTreeMap::<String, Affected>::new();
        let mut remaining_postings = request.range_limits.postings;
        let mut remaining_bytes = request.matched_claim_bytes;
        let mut remaining_reasons = request.evidence_budget;
        let stream = request.origin.stream.to_hex();
        if membership_retractions.is_empty() {
            for reference in closed.keys() {
                remaining_postings = remaining_postings
                    .checked_sub(1)
                    .ok_or("stream posting budget exhausted")?;
                let mut statement = transaction.prepare(POSTINGS_SQL)?;
                let mut rows = statement.query(params![
                    stream,
                    bincode::serialize(&reference.fact)?,
                    reference.valid_from.0.to_hex()
                ])?;
                while let Some(row) = rows.next()? {
                    remaining_postings = remaining_postings
                        .checked_sub(1)
                        .ok_or("stream posting budget exhausted")?;
                    let identity: String = row.get(0)?;
                    if !affected.contains_key(&identity) {
                        if affected.len() >= request.range_limits.claims {
                            return Err("stream affected claim budget exhausted".into());
                        }
                        let born = generation(row.get_ref(3)?.as_str()?)?;
                        let born_sequence = u64::try_from(row.get::<_, i64>(4)?)?;
                        let birth = checkpoint(&transaction, request.origin, born)?
                            .ok_or("stream claim birth checkpoint missing")?;
                        if birth.sequence != born_sequence || born_sequence > head.sequence {
                            return Err("stream claim birth coordinate mismatch".into());
                        }
                        let claim = crate::read::decode_claim_at(
                            row,
                            birth.selection,
                            &mut remaining_bytes,
                        )?;
                        affected.insert(
                            identity.clone(),
                            Affected {
                                claim,
                                reasons: BTreeSet::new(),
                            },
                        );
                    }
                    let entry = affected
                        .get_mut(&identity)
                        .ok_or("stream affected witness missing")?;
                    if !entry.claim.premises.contains(reference) {
                        return Err("stream posting not in witness".into());
                    }
                    remaining_reasons = remaining_reasons
                        .checked_sub(1)
                        .ok_or("stream evidence budget exhausted")?;
                    entry.reasons.insert(*reference);
                }
            }
        } else {
            let mut statement = transaction.prepare(
                "SELECT claim, payload, payload_digest, born_generation, born_sequence
                 FROM inference_stream_claims
                 WHERE stream = ?1 AND closed_sequence IS NULL ORDER BY claim",
            )?;
            let mut rows = statement.query([&stream])?;
            while let Some(row) = rows.next()? {
                remaining_postings = remaining_postings
                    .checked_sub(1)
                    .ok_or("stream posting budget exhausted")?;
                if affected.len() >= request.range_limits.claims {
                    return Err("stream affected claim budget exhausted".into());
                }
                let identity: String = row.get(0)?;
                let born = generation(row.get_ref(3)?.as_str()?)?;
                let born_sequence = u64::try_from(row.get::<_, i64>(4)?)?;
                let birth = checkpoint(&transaction, request.origin, born)?
                    .ok_or("stream claim birth checkpoint missing")?;
                if birth.sequence != born_sequence || born_sequence > head.sequence {
                    return Err("stream claim birth coordinate mismatch".into());
                }
                let claim =
                    crate::read::decode_claim_at(row, birth.selection, &mut remaining_bytes)?;
                let mut reasons = membership_retractions.clone();
                reasons.extend(
                    closed
                        .keys()
                        .filter(|reference| claim.premises.contains(reference))
                        .copied(),
                );
                for _ in &reasons {
                    remaining_reasons = remaining_reasons
                        .checked_sub(1)
                        .ok_or("stream evidence budget exhausted")?;
                }
                affected.insert(identity, Affected { claim, reasons });
            }
        }
        let discovery_range = DiscoveryRange {
            from: head.selection.generation,
            generations: request.range_limits.generations,
            versions: request.range_limits.versions,
        };
        let discovered = if let Some((registry, bound, binding_budget)) = registered {
            if membership_retractions.is_empty() {
                query.infer_discovered_bound_rules(
                    &discovery_range,
                    registry,
                    bound,
                    request.limits,
                    syntaxmesh_inference::RegisteredDiscoveryBudgets {
                        history_scan: request.history_scan_budget,
                        rules: request.rule_budget,
                        ontology_bindings: binding_budget,
                    },
                )?
            } else {
                query.infer_bound(
                    registry,
                    bound,
                    request.limits,
                    request.history_scan_budget,
                    request.rule_budget,
                    binding_budget,
                )?
            }
        } else {
            query.infer_discovered_rules(
                &discovery_range,
                request.rules,
                request.limits,
                request.history_scan_budget,
                request.rule_budget,
            )?
        };
        let mut remaining_output = u64::try_from(request.output_bytes)?;
        let mut remaining_premises = request.premise_budget;
        let mut payloads = Vec::with_capacity(discovered.len());
        for claim in &discovered {
            remaining_output = remaining_output
                .checked_sub(bincode::serialized_size(claim)?)
                .ok_or("stream output byte budget exhausted")?;
            remaining_premises = remaining_premises
                .checked_sub(claim.premises.len())
                .ok_or("stream new premise budget exhausted")?;
            payloads.push(bincode::serialize(claim)?);
        }
        let target = request.through.0.to_hex();
        let sql_sequence = i64::try_from(sequence)?;
        let mut tree = super::tree::ActiveTree::new(&transaction, request.page_budget)?;
        let mut active_root = super::tree::root(
            &transaction,
            request.origin.stream,
            head.selection.generation,
        )?;
        if tree.count(active_root)? != head.active_claims {
            return Err("stream head active root count inconsistent".into());
        }
        for (identity, entry) in &affected {
            active_root = tree.remove(active_root, entry.claim.id)?;
            let changed = transaction.execute("UPDATE inference_stream_claims SET closed_generation = ?3, closed_sequence = ?4 WHERE stream = ?1 AND claim = ?2 AND closed_sequence IS NULL", params![stream, identity, target, sql_sequence])?;
            if changed != 1 {
                return Err("stream claim closure lost active witness".into());
            }
            for reference in &entry.reasons {
                transaction.execute(
                    "INSERT INTO inference_stream_closures VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        stream,
                        identity,
                        bincode::serialize(&reference.fact)?,
                        reference.valid_from.0.to_hex(),
                        target,
                        sql_sequence
                    ],
                )?;
            }
        }
        for (claim, payload) in discovered.iter().zip(payloads) {
            active_root = Some(tree.insert(active_root, claim.id)?);
            let identity = claim.id.to_hex();
            transaction.execute(
                "INSERT INTO inference_stream_claims VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL)",
                params![
                    stream,
                    identity,
                    payload,
                    blake3::hash(&payload).to_hex().to_string(),
                    target,
                    sql_sequence
                ],
            )?;
            for (ordinal, reference) in claim.premises.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO inference_stream_premises VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        stream,
                        identity,
                        i64::try_from(ordinal)?,
                        bincode::serialize(&reference.fact)?,
                        reference.valid_from.0.to_hex()
                    ],
                )?;
            }
        }
        let receipt = StreamCheckpoint {
            stream: request.origin.stream,
            selection: syntaxmesh_inference::ClaimSelection {
                generation: request.through,
                ..baseline.selection
            },
            sequence,
            previous: Some(head.selection.generation),
            added_claims: discovered.len(),
            affected_claims: affected.len(),
            closure_reasons: request
                .evidence_budget
                .checked_sub(remaining_reasons)
                .ok_or("stream evidence accounting underflow")?,
            active_claims: head
                .active_claims
                .checked_sub(affected.len())
                .and_then(|remaining| remaining.checked_add(discovered.len()))
                .ok_or("stream active count overflow")?,
        };
        if tree.count(active_root)? != receipt.active_claims {
            return Err("updated stream active root count inconsistent".into());
        }
        transaction.execute(
            "INSERT INTO inference_stream_checkpoints VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                stream,
                target,
                sql_sequence,
                head.selection.generation.0.to_hex(),
                i64::try_from(receipt.added_claims)?,
                i64::try_from(receipt.affected_claims)?,
                i64::try_from(receipt.closure_reasons)?,
                i64::try_from(receipt.active_claims)?
            ],
        )?;
        transaction.execute(
            "INSERT INTO inference_stream_roots VALUES (?1, ?2, ?3)",
            params![
                stream,
                target,
                active_root.map(syntaxmesh_core::StableId::to_hex)
            ],
        )?;
        transaction.execute("UPDATE inference_streams SET head_generation = ?2, head_sequence = ?3 WHERE stream = ?1", params![stream, target, sql_sequence])?;
        transaction.commit()?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
