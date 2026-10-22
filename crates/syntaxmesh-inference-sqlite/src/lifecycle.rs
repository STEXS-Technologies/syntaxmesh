use crate::read::{decode_claim, verify_receipt};
use crate::{InferenceMaterializer, Result, Watermark};
use bincode::Options;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;
use syntaxmesh_core::{FactVersionRef, GenerationId, StableId};
use syntaxmesh_inference::{Claim, InvalidationLimits, PremiseClosure};
use syntaxmesh_query::Query;
use syntaxmesh_store::GraphStore;

pub struct LifecycleRequest<'limits> {
    pub origin: Watermark,
    pub through: GenerationId,
    pub limits: &'limits InvalidationLimits,
    pub evidence_budget: usize,
    pub claim_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleReceipt {
    pub operation: StableId,
    pub checked_through: PremiseClosure,
    pub affected_claims: usize,
    pub closure_reasons: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureReason {
    pub premise: FactVersionRef,
    pub closed_at: PremiseClosure,
}

/// Known validity interval plus explicit coverage; unknown future validity is not asserted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimLifecycle {
    pub claim: StableId,
    pub valid_from: PremiseClosure,
    pub valid_until: Option<PremiseClosure>,
    pub checked_through: PremiseClosure,
    pub reasons: Vec<ClosureReason>,
}

struct Affected {
    claim: Claim,
    reasons: BTreeMap<FactVersionRef, PremiseClosure>,
}

const POSTINGS_SQL: &str =
    "SELECT DISTINCT c.claim, c.payload, c.payload_digest FROM inference_premises p
                 JOIN inference_claims c ON c.operation = p.operation AND c.claim = p.claim
                 WHERE p.operation = ?1 AND p.fact = ?2 AND p.valid_from = ?3 ORDER BY c.claim";

impl InferenceMaterializer {
    /// Persist direct premise invalidations and checked coverage without rule replay.
    ///
    /// # Errors
    /// Rejects foreign/reversed coverage, malformed canonical feeds, corrupt matched
    /// claims or exhausted generation/version/posting/claim/evidence/byte admission.
    pub fn invalidate<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        request: &LifecycleRequest<'_>,
    ) -> Result<LifecycleReceipt> {
        if request.evidence_budget == 0
            || request.claim_bytes == 0
            || request.limits.postings == 0
            || request.limits.claims == 0
            || request.limits.versions == 0
            || request.limits.generations == 0
        {
            return Err("positive lifecycle admission required".into());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        verify_receipt(&transaction, request.origin)?;
        let operation = request.origin.operation.to_hex();
        let coverage = coverage(&transaction, request.origin)?;
        let manifest = store.manifest(request.through)?;
        let sequence = store.generation_sequence(request.through)?;
        let origin_manifest = store.manifest(request.origin.selection.generation)?;
        if manifest.repository != request.origin.selection.repository
            || manifest.worktree != request.origin.selection.worktree
            || origin_manifest.repository != manifest.repository
            || origin_manifest.worktree != manifest.worktree
            || store.generation_sequence(request.origin.selection.generation)?
                != request.origin.generation_sequence
            || store.generation_sequence(coverage.checked_through.generation)?
                != coverage.checked_through.generation_sequence
        {
            return Err("lifecycle canonical scope or coverage mismatch".into());
        }
        if let Some(retained) = retained_receipt(&transaction, request.origin, request.through)? {
            if retained.checked_through.generation_sequence != sequence
                || retained.affected_claims > request.limits.claims
                || retained.closure_reasons > request.evidence_budget
            {
                return Err("retained lifecycle receipt exceeds scope/output admission".into());
            }
            transaction.commit()?;
            return Ok(retained);
        }
        if sequence < coverage.checked_through.generation_sequence {
            return Err("unrecorded lifecycle target would regress coverage".into());
        }
        if sequence == coverage.checked_through.generation_sequence {
            if request.through != coverage.checked_through.generation {
                return Err("lifecycle generation substitution".into());
            }
            if coverage.affected_claims > request.limits.claims
                || coverage.closure_reasons > request.evidence_budget
            {
                return Err("cached lifecycle receipt exceeds output admission".into());
            }
            transaction.commit()?;
            return Ok(coverage);
        }
        let closed = Query::new(store, request.through)
            .inference_premise_closures(coverage.checked_through.generation, request.limits)?;
        let mut affected = BTreeMap::<String, Affected>::new();
        let mut remaining_postings = request.limits.postings;
        let mut remaining_evidence = request.evidence_budget;
        let mut remaining_bytes = request.claim_bytes;
        for (reference, closure) in &closed {
            remaining_postings = remaining_postings
                .checked_sub(1)
                .ok_or("lifecycle posting budget exhausted")?;
            let mut statement = transaction.prepare(POSTINGS_SQL)?;
            let mut rows = statement.query(params![
                operation,
                bincode::serialize(&reference.fact)?,
                reference.valid_from.0.to_hex()
            ])?;
            while let Some(row) = rows.next()? {
                remaining_postings = remaining_postings
                    .checked_sub(1)
                    .ok_or("lifecycle posting budget exhausted")?;
                let identity: String = row.get(0)?;
                if !affected.contains_key(&identity) {
                    if affected.len() >= request.limits.claims {
                        return Err("lifecycle claim budget exhausted".into());
                    }
                    let claim = decode_claim(row, request.origin, &mut remaining_bytes)?;
                    affected.insert(
                        identity.clone(),
                        Affected {
                            claim,
                            reasons: BTreeMap::new(),
                        },
                    );
                }
                let matched = affected
                    .get_mut(&identity)
                    .ok_or("missing affected claim")?;
                if !matched.claim.premises.contains(reference) {
                    return Err("lifecycle posting is not a claim witness".into());
                }
                remaining_evidence = remaining_evidence
                    .checked_sub(1)
                    .ok_or("lifecycle evidence budget exhausted")?;
                matched.reasons.insert(*reference, *closure);
            }
        }
        let reason_count = request
            .evidence_budget
            .checked_sub(remaining_evidence)
            .ok_or("invalid lifecycle evidence accounting")?;
        for (identity, entry) in &affected {
            let earliest = entry
                .reasons
                .values()
                .min_by_key(|closure| closure.generation_sequence)
                .ok_or("affected claim has no closure reason")?;
            transaction.execute(
                "INSERT INTO inference_invalidations VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(operation, claim) DO UPDATE SET invalidated_generation = excluded.invalidated_generation,
                 invalidated_sequence = excluded.invalidated_sequence
                 WHERE excluded.invalidated_sequence < inference_invalidations.invalidated_sequence",
                params![operation, identity, earliest.generation.0.to_hex(), i64::try_from(earliest.generation_sequence)?],
            )?;
            for (reference, closure) in &entry.reasons {
                transaction.execute(
                    "INSERT INTO inference_invalidation_reasons VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        operation,
                        identity,
                        bincode::serialize(&reference.fact)?,
                        reference.valid_from.0.to_hex(),
                        closure.generation.0.to_hex(),
                        i64::try_from(closure.generation_sequence)?
                    ],
                )?;
            }
        }
        transaction.execute("UPDATE inference_lifecycle SET through_generation = ?2, through_sequence = ?3, last_claim_count = ?4, last_reason_count = ?5 WHERE operation = ?1",
            params![operation, request.through.0.to_hex(), i64::try_from(sequence)?, i64::try_from(affected.len())?, i64::try_from(reason_count)?])?;
        let receipt = LifecycleReceipt {
            operation: request.origin.operation,
            checked_through: PremiseClosure {
                generation: request.through,
                generation_sequence: sequence,
            },
            affected_claims: affected.len(),
            closure_reasons: reason_count,
        };
        transaction.execute(
            "INSERT INTO inference_lifecycle_receipts VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                operation,
                request.through.0.to_hex(),
                i64::try_from(sequence)?,
                i64::try_from(affected.len())?,
                i64::try_from(reason_count)?
            ],
        )?;
        transaction.commit()?;
        Ok(receipt)
    }

    /// Find an exact completed lifecycle effect, including historical retries.
    ///
    /// # Errors
    /// Rejects substituted origin receipts or effects beyond checked coverage.
    pub fn lifecycle_receipt(
        &self,
        origin: Watermark,
        through: GenerationId,
    ) -> Result<Option<LifecycleReceipt>> {
        verify_receipt(&self.connection, origin)?;
        retained_receipt(&self.connection, origin, through)
    }

    /// Read one claim's known validity, coverage and exact bounded closure reasons.
    ///
    /// # Errors
    /// Rejects foreign/unknown receipts or claims, corrupt coordinates and exhausted evidence bounds.
    pub fn claim_lifecycle(
        &self,
        origin: Watermark,
        claim: StableId,
        evidence_budget: usize,
    ) -> Result<ClaimLifecycle> {
        if evidence_budget == 0 {
            return Err("positive lifecycle evidence budget required".into());
        }
        verify_receipt(&self.connection, origin)?;
        let operation = origin.operation.to_hex();
        let identity = claim.to_hex();
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM inference_claims WHERE operation = ?1 AND claim = ?2)",
            params![operation, identity],
            |row| row.get(0),
        )?;
        if !exists {
            return Err("unknown lifecycle claim".into());
        }
        let coverage = coverage(&self.connection, origin)?;
        let closed: Option<(String, i64)> = self.connection.query_row("SELECT invalidated_generation, invalidated_sequence FROM inference_invalidations WHERE operation = ?1 AND claim = ?2",
            params![operation, identity], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
        let valid_until = closed
            .map(|(generation, sequence)| coordinates(&generation, sequence))
            .transpose()?;
        let mut statement = self.connection.prepare("SELECT fact, valid_from, closed_generation, closed_sequence FROM inference_invalidation_reasons WHERE operation = ?1 AND claim = ?2 ORDER BY closed_sequence, fact, valid_from")?;
        let mut rows = statement.query(params![operation, identity])?;
        let mut reasons = Vec::new();
        while let Some(row) = rows.next()? {
            if reasons.len() >= evidence_budget {
                return Err("lifecycle evidence output exhausted".into());
            }
            let fact = row.get_ref(0)?.as_blob()?;
            let fact = bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .with_limit(36)
                .reject_trailing_bytes()
                .deserialize(fact)?;
            let from: String = row.get(1)?;
            let closed_generation: String = row.get(2)?;
            let sequence: i64 = row.get(3)?;
            let closed_at = coordinates(&closed_generation, sequence)?;
            if closed_at.generation_sequence <= origin.generation_sequence
                || closed_at.generation_sequence > coverage.checked_through.generation_sequence
            {
                return Err("lifecycle reason outside checked interval".into());
            }
            let premise = FactVersionRef {
                fact,
                valid_from: generation(&from)?,
            };
            let posted: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM inference_premises WHERE operation = ?1 AND claim = ?2 AND fact = ?3 AND valid_from = ?4)",
                params![operation, identity, bincode::serialize(&premise.fact)?, from], |posting_row| posting_row.get(0),
            )?;
            if !posted {
                return Err("lifecycle closure reason is not a stored premise".into());
            }
            reasons.push(ClosureReason { premise, closed_at });
        }
        if valid_until != reasons.first().map(|reason| reason.closed_at) {
            return Err("lifecycle interval differs from earliest closure reason".into());
        }
        Ok(ClaimLifecycle {
            claim,
            valid_from: PremiseClosure {
                generation: origin.selection.generation,
                generation_sequence: origin.generation_sequence,
            },
            valid_until,
            checked_through: coverage.checked_through,
            reasons,
        })
    }
}

fn retained_receipt(
    connection: &Connection,
    origin: Watermark,
    through: GenerationId,
) -> Result<Option<LifecycleReceipt>> {
    let stored: Option<(i64, i64, i64)> = connection.query_row(
        "SELECT through_sequence, affected_claims, closure_reasons FROM inference_lifecycle_receipts WHERE operation = ?1 AND through_generation = ?2",
        params![origin.operation.to_hex(), through.0.to_hex()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    let Some((sequence, claims, reasons)) = stored else {
        return Ok(None);
    };
    let selected = coordinates(&through.0.to_hex(), sequence)?;
    let coverage = coverage(connection, origin)?;
    if selected.generation_sequence < origin.generation_sequence
        || selected.generation_sequence > coverage.checked_through.generation_sequence
        || usize::try_from(claims)? > origin.claim_count
        || (selected.generation_sequence == coverage.checked_through.generation_sequence
            && through != coverage.checked_through.generation)
    {
        return Err("retained lifecycle receipt outside verified coverage".into());
    }
    Ok(Some(LifecycleReceipt {
        operation: origin.operation,
        checked_through: selected,
        affected_claims: usize::try_from(claims)?,
        closure_reasons: usize::try_from(reasons)?,
    }))
}

fn coverage(connection: &Connection, origin: Watermark) -> Result<LifecycleReceipt> {
    let (generation, sequence, claims, reasons): (String, i64, i64, i64) = connection.query_row(
        "SELECT through_generation, through_sequence, last_claim_count, last_reason_count FROM inference_lifecycle WHERE operation = ?1",
        [origin.operation.to_hex()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let checked_through = coordinates(&generation, sequence)?;
    if checked_through.generation_sequence < origin.generation_sequence {
        return Err("lifecycle coverage precedes origin".into());
    }
    if checked_through.generation_sequence == origin.generation_sequence
        && checked_through.generation != origin.selection.generation
    {
        return Err("lifecycle origin generation substitution".into());
    }
    Ok(LifecycleReceipt {
        operation: origin.operation,
        checked_through,
        affected_claims: usize::try_from(claims)?,
        closure_reasons: usize::try_from(reasons)?,
    })
}

fn coordinates(encoded: &str, sequence: i64) -> Result<PremiseClosure> {
    let generation_sequence = u64::try_from(sequence)?;
    if generation_sequence == 0 {
        return Err("zero lifecycle generation sequence".into());
    }
    Ok(PremiseClosure {
        generation: generation(encoded)?,
        generation_sequence,
    })
}

fn generation(encoded: &str) -> Result<GenerationId> {
    let mut bytes = [0_u8; 32];
    hex::decode_to_slice(encoded, &mut bytes)?;
    Ok(GenerationId(StableId(bytes)))
}

#[cfg(test)]
mod tests;
