use super::StreamOrigin;
use crate::{InferenceMaterializer, Result};
use rusqlite::{Connection, OptionalExtension, params};
use syntaxmesh_core::{GenerationId, StableId};
use syntaxmesh_inference::ClaimSelection;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StreamCheckpoint {
    pub stream: StableId,
    pub selection: ClaimSelection,
    pub sequence: u64,
    pub previous: Option<GenerationId>,
    pub added_claims: usize,
    pub affected_claims: usize,
    pub closure_reasons: usize,
    pub active_claims: usize,
}

impl InferenceMaterializer {
    /// Query retained active-set membership without enumerating claims or replaying history.
    /// This verifies tree membership, not the claim payload or witness postings.
    ///
    /// # Errors
    /// Rejects unknown selections, inconsistent roots and exhausted page admission.
    pub fn stream_claim_active(
        &self,
        origin: StreamOrigin,
        generation: GenerationId,
        claim: StableId,
        page_budget: usize,
    ) -> Result<(StreamCheckpoint, bool)> {
        let transaction = self.connection.unchecked_transaction()?;
        let head = verify_origin(&transaction, origin)?;
        let selected = checkpoint(&transaction, origin, generation)?
            .ok_or("stream membership checkpoint not retained")?;
        if selected.sequence > head.sequence {
            return Err("stream membership exceeds accepted coverage".into());
        }
        let root = super::tree::root(&transaction, origin.stream, generation)?;
        let active = super::tree::ActiveTree::new(&transaction, page_budget)?.membership(
            root,
            claim,
            selected.active_claims,
        )?;
        transaction.commit()?;
        Ok((selected, active))
    }

    /// Read an exact retained stream checkpoint without canonical or rule replay.
    ///
    /// # Errors
    /// Rejects substituted roots and inconsistent checkpoint/head coordinates or counts.
    pub fn stream_checkpoint(
        &self,
        origin: StreamOrigin,
        generation: GenerationId,
    ) -> Result<Option<StreamCheckpoint>> {
        let transaction = self.connection.unchecked_transaction()?;
        let head = verify_origin(&transaction, origin)?;
        let retained = checkpoint(&transaction, origin, generation)?;
        if retained.is_some_and(|entry| entry.sequence > head.sequence) {
            return Err("stream checkpoint exceeds accepted coverage".into());
        }
        if let Some(entry) = retained {
            let root = super::tree::root(&transaction, origin.stream, generation)?;
            if root.is_none() != (entry.active_claims == 0) {
                return Err("stream checkpoint root emptiness inconsistent".into());
            }
        }
        transaction.commit()?;
        Ok(retained)
    }
}

pub(super) fn generation(text: &str) -> Result<GenerationId> {
    let mut bytes = [0_u8; 32];
    hex::decode_to_slice(text, &mut bytes)?;
    Ok(GenerationId(StableId(bytes)))
}

pub(super) fn verify_origin(
    connection: &Connection,
    origin: StreamOrigin,
) -> Result<StreamCheckpoint> {
    crate::read::verify_receipt(connection, origin.baseline)?;
    if origin.stream != StableId::derive("inference-stream-v1", &[&origin.baseline.operation.0]) {
        return Err("stream root identity mismatch".into());
    }
    let (operation, head_generation, head_sequence): (String, String, i64) = connection.query_row(
        "SELECT origin_operation, head_generation, head_sequence FROM inference_streams WHERE stream = ?1",
        [origin.stream.to_hex()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if operation != origin.baseline.operation.to_hex() {
        return Err("stream origin substitution".into());
    }
    let head = checkpoint(connection, origin, generation(&head_generation)?)?
        .ok_or("stream head checkpoint missing")?;
    if head.sequence != u64::try_from(head_sequence)? {
        return Err("stream head sequence mismatch".into());
    }
    Ok(head)
}

pub(super) fn checkpoint(
    connection: &Connection,
    origin: StreamOrigin,
    target: GenerationId,
) -> Result<Option<StreamCheckpoint>> {
    let mut statement = connection.prepare(
        "SELECT sequence, previous_generation, added_claims, affected_claims, closure_reasons, active_claims
         FROM inference_stream_checkpoints WHERE stream = ?1 AND generation = ?2",
    )?;
    let retained_row = statement
        .query_row(params![origin.stream.to_hex(), target.0.to_hex()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .optional()?;
    let Some((sequence, previous, added, affected, reasons, active)) = retained_row else {
        return Ok(None);
    };
    let receipt = StreamCheckpoint {
        stream: origin.stream,
        selection: ClaimSelection {
            generation: target,
            ..origin.baseline.selection
        },
        sequence: u64::try_from(sequence)?,
        previous: previous.as_deref().map(generation).transpose()?,
        added_claims: usize::try_from(added)?,
        affected_claims: usize::try_from(affected)?,
        closure_reasons: usize::try_from(reasons)?,
        active_claims: usize::try_from(active)?,
    };
    if receipt.sequence < origin.baseline.generation_sequence
        || receipt.closure_reasons < receipt.affected_claims
    {
        return Err("stream checkpoint coordinates or reasons inconsistent".into());
    }
    if target == origin.baseline.selection.generation {
        if receipt.sequence != origin.baseline.generation_sequence
            || receipt.previous.is_some()
            || receipt.added_claims != origin.baseline.claim_count
            || receipt.active_claims != origin.baseline.claim_count
            || receipt.affected_claims != 0
            || receipt.closure_reasons != 0
        {
            return Err("stream baseline checkpoint inconsistent".into());
        }
    } else {
        let predecessor = receipt
            .previous
            .ok_or("stream successor predecessor missing")?;
        let (prior_sequence, prior_active): (i64, i64) = connection.query_row(
            "SELECT sequence, active_claims FROM inference_stream_checkpoints WHERE stream = ?1 AND generation = ?2",
            params![origin.stream.to_hex(), predecessor.0.to_hex()], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if receipt.sequence
            != u64::try_from(prior_sequence)?
                .checked_add(1)
                .ok_or("stream sequence overflow")?
            || receipt.active_claims
                != usize::try_from(prior_active)?
                    .checked_sub(receipt.affected_claims)
                    .and_then(|remaining| remaining.checked_add(receipt.added_claims))
                    .ok_or("stream active count overflow")?
        {
            return Err("stream successor checkpoint inconsistent".into());
        }
    }
    Ok(Some(receipt))
}
