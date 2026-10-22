use super::checkpoint::{checkpoint, generation, verify_origin};
use super::{StreamCheckpoint, StreamOrigin};
use crate::{InferenceMaterializer, Result};
use rusqlite::params;
use syntaxmesh_core::{GenerationId, StableId};
use syntaxmesh_inference::Claim;

mod page;
pub use page::{StreamCursor, StreamPage, StreamPageRequest};

/// The queried checkpoint is distinct from each retained claim's birth generation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StreamClaims {
    pub checkpoint: StreamCheckpoint,
    pub claims: Vec<Claim>,
}

/// One selected witness, preserving its original birth generation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StreamClaim {
    pub checkpoint: StreamCheckpoint,
    pub claim: Option<Claim>,
}

const CLAIMS_SQL: &str =
    "SELECT claim, payload, payload_digest, born_generation, born_sequence, closed_generation, closed_sequence
     FROM inference_stream_claims WHERE stream = ?1 AND claim = ?2";

impl InferenceMaterializer {
    /// Read one retained active witness without enumerating the active set.
    ///
    /// # Errors
    /// Rejects unknown selections, corrupt selected data and exhausted admission.
    pub fn stream_claim(
        &self,
        origin: StreamOrigin,
        at: GenerationId,
        identity: StableId,
        byte_budget: usize,
        premise_budget: usize,
        page_budget: usize,
    ) -> Result<StreamClaim> {
        if byte_budget == 0 || premise_budget == 0 || page_budget == 0 {
            return Err("positive stream point read admission required".into());
        }
        let transaction = self.connection.unchecked_transaction()?;
        let head = verify_origin(&transaction, origin)?;
        let selected =
            checkpoint(&transaction, origin, at)?.ok_or("stream point checkpoint not retained")?;
        if selected.sequence > head.sequence {
            return Err("stream point selection exceeds coverage".into());
        }
        let root = super::tree::root(&transaction, origin.stream, at)?;
        let active = super::tree::ActiveTree::new(&transaction, page_budget)?.membership(
            root,
            identity,
            selected.active_claims,
        )?;
        let mut remaining_bytes = byte_budget;
        let mut remaining_premises = premise_budget;
        let claim = if active {
            let mut statement = transaction.prepare(CLAIMS_SQL)?;
            let mut rows = statement.query(params![origin.stream.to_hex(), identity.to_hex()])?;
            let row = rows.next()?.ok_or("stream active root claim missing")?;
            Some(decode_active(
                &transaction,
                row,
                origin,
                selected,
                head,
                &mut remaining_bytes,
                &mut remaining_premises,
            )?)
        } else {
            None
        };
        transaction.commit()?;
        Ok(StreamClaim {
            checkpoint: selected,
            claim,
        })
    }

    /// Read active witnesses at a retained checkpoint, without history/rule replay.
    /// One deferred transaction pins receipts, intervals and ordered witness postings.
    /// Claims preserve their birth generation; the enclosing checkpoint pins the query.
    ///
    /// # Errors
    /// Rejects unrecorded/substituted checkpoints, inconsistent interval/payload/posting
    /// data, exhausted independent output bounds and incomplete/excess active rows.
    pub fn stream_claims(
        &self,
        origin: StreamOrigin,
        at: GenerationId,
        claim_budget: usize,
        byte_budget: usize,
        premise_budget: usize,
        page_budget: usize,
    ) -> Result<StreamClaims> {
        if claim_budget == 0 || byte_budget == 0 || premise_budget == 0 || page_budget == 0 {
            return Err("positive stream read admission required".into());
        }
        let transaction = self.connection.unchecked_transaction()?;
        let head = verify_origin(&transaction, origin)?;
        let selected =
            checkpoint(&transaction, origin, at)?.ok_or("stream read checkpoint not retained")?;
        if selected.sequence > head.sequence || selected.active_claims > claim_budget {
            return Err("stream selection exceeds coverage or claim admission".into());
        }
        let stream = origin.stream.to_hex();
        let root = super::tree::root(&transaction, origin.stream, at)?;
        let keys =
            super::tree::ActiveTree::new(&transaction, page_budget)?.keys(root, claim_budget)?;
        if keys.len() != selected.active_claims {
            return Err("stream root active count differs from checkpoint".into());
        }
        let mut statement = transaction.prepare(CLAIMS_SQL)?;
        let mut claims = Vec::new();
        let mut remaining_bytes = byte_budget;
        let mut remaining_premises = premise_budget;
        for identity in keys {
            let mut rows = statement.query(params![stream, identity.to_hex()])?;
            let row = rows.next()?.ok_or("stream active root claim missing")?;
            if claims.len() >= selected.active_claims {
                return Err("stream selection contains excess active claims".into());
            }
            claims.push(decode_active(
                &transaction,
                row,
                origin,
                selected,
                head,
                &mut remaining_bytes,
                &mut remaining_premises,
            )?);
        }
        if claims.len() != selected.active_claims {
            return Err("stream active selection is incomplete".into());
        }
        drop(statement);
        transaction.commit()?;
        Ok(StreamClaims {
            checkpoint: selected,
            claims,
        })
    }
}

pub(super) fn decode_active(
    transaction: &rusqlite::Connection,
    row: &rusqlite::Row<'_>,
    origin: StreamOrigin,
    selected: StreamCheckpoint,
    head: StreamCheckpoint,
    remaining_bytes: &mut usize,
    remaining_premises: &mut usize,
) -> Result<Claim> {
    let stream = origin.stream.to_hex();
    let born = generation(row.get_ref(3)?.as_str()?)?;
    let born_sequence = u64::try_from(row.get::<_, i64>(4)?)?;
    let birth = checkpoint(transaction, origin, born)?.ok_or("stream birth checkpoint missing")?;
    if birth.sequence != born_sequence || born_sequence > selected.sequence {
        return Err("stream birth interval coordinates inconsistent".into());
    }
    let closed_generation: Option<String> = row.get(5)?;
    let closed_sequence: Option<i64> = row.get(6)?;
    match (closed_generation, closed_sequence) {
        (Some(closed), Some(sequence)) => {
            let closure = checkpoint(transaction, origin, generation(&closed)?)?
                .ok_or("stream closure checkpoint missing")?;
            let closing = u64::try_from(sequence)?;
            if closure.sequence != closing
                || closing <= born_sequence
                || closing <= selected.sequence
                || closing > head.sequence
            {
                return Err("stream closure interval coordinates inconsistent".into());
            }
        }
        (None, None) => {}
        _ => return Err("stream closure interval is incomplete".into()),
    }
    let claim = crate::read::decode_claim_at(row, birth.selection, remaining_bytes)?;
    *remaining_premises = remaining_premises
        .checked_sub(claim.premises.len())
        .ok_or("stream premise read budget exhausted")?;
    let mut postings = transaction.prepare(
                "SELECT ordinal, fact, valid_from FROM inference_stream_premises WHERE stream = ?1 AND claim = ?2 ORDER BY ordinal",
            )?;
    let mut witnesses = postings.query(params![stream, claim.id.to_hex()])?;
    for (ordinal, premise) in claim.premises.iter().enumerate() {
        let witness = witnesses.next()?.ok_or("stream witness posting missing")?;
        if witness.get::<_, i64>(0)? != i64::try_from(ordinal)?
            || witness.get_ref(1)?.as_blob()? != bincode::serialize(&premise.fact)?
            || witness.get_ref(2)?.as_str()? != premise.valid_from.0.to_hex()
        {
            return Err("stream witness posting inconsistent".into());
        }
    }
    if witnesses.next()?.is_some() {
        return Err("stream witness has excess postings".into());
    }
    Ok(claim)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_sparse;
