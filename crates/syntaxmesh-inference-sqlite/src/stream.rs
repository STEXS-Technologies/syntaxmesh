use crate::{InferenceMaterializer, Result, Watermark};
use rusqlite::{OptionalExtension, params};
use syntaxmesh_core::StableId;

mod checkpoint;
mod read;
mod sync;
pub(crate) mod tree;
pub use checkpoint::StreamCheckpoint;
pub use read::{StreamClaim, StreamClaims, StreamCursor, StreamPage, StreamPageRequest};
pub use sync::StreamSyncRequest;

/// Immutable stream root; not a later synchronization checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StreamOrigin {
    pub stream: StableId,
    pub baseline: Watermark,
}

impl InferenceMaterializer {
    /// Atomically initialize independent temporal claims from a complete selection.
    /// Initialization is bounded and never resets a previously initialized stream.
    ///
    /// # Errors
    /// Rejects corrupt baselines, exhausted count/byte/posting bounds and SQL failures.
    pub fn begin_stream(
        &mut self,
        baseline: Watermark,
        claim_budget: usize,
        byte_budget: usize,
        premise_budget: usize,
        page_budget: usize,
    ) -> Result<StreamOrigin> {
        if premise_budget == 0 || page_budget == 0 {
            return Err("positive stream premise admission required".into());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let claims = crate::read::read_claims(&transaction, baseline, claim_budget, byte_budget)?;
        let mut remaining = premise_budget;
        for claim in &claims {
            remaining = remaining
                .checked_sub(claim.premises.len())
                .ok_or("stream premise budget exhausted")?;
        }
        let receipt = StreamOrigin {
            stream: StableId::derive("inference-stream-v1", &[&baseline.operation.0]),
            baseline,
        };
        let stream = receipt.stream.to_hex();
        let operation = baseline.operation.to_hex();
        let sequence = i64::try_from(baseline.generation_sequence)?;
        let generation = baseline.selection.generation.0.to_hex();
        let count = i64::try_from(baseline.claim_count)?;
        let mut tree = tree::ActiveTree::new(&transaction, page_budget)?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT origin_operation FROM inference_streams WHERE stream = ?1",
                [&stream],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(origin) = existing {
            if origin != operation {
                return Err("stream baseline substitution".into());
            }
            let valid: bool = transaction.query_row(
                "SELECT s.head_sequence >= ?3 AND (s.head_sequence != ?3 OR s.head_generation = ?2)
                 AND EXISTS (SELECT 1 FROM inference_stream_checkpoints p WHERE p.stream = s.stream AND p.generation = s.head_generation AND p.sequence = s.head_sequence)
                 AND EXISTS (SELECT 1 FROM inference_stream_checkpoints p WHERE p.stream = s.stream AND p.generation = ?2 AND p.sequence = ?3 AND p.previous_generation IS NULL AND p.added_claims = ?4 AND p.active_claims = ?4 AND p.affected_claims = 0 AND p.closure_reasons = 0)
                 FROM inference_streams s WHERE s.stream = ?1",
                params![stream, generation, sequence, count], |row| row.get(0),
            )?;
            if !valid {
                return Err("stream baseline checkpoint or head mismatch".into());
            }
            let retained: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM inference_stream_claims WHERE stream = ?1 AND born_sequence = ?2",
                params![stream, sequence], |row| row.get(0),
            )?;
            if retained != count {
                return Err("stream baseline claim count mismatch".into());
            }
            let retained_root =
                tree::root(&transaction, receipt.stream, baseline.selection.generation)?;
            if tree.keys(retained_root, claim_budget)?
                != claims.iter().map(|claim| claim.id).collect::<Vec<_>>()
            {
                return Err("stream baseline active root differs from complete selection".into());
            }
            for claim in &claims {
                let identity = claim.id.to_hex();
                let equal: bool = transaction.query_row(
                    "SELECT s.payload = c.payload AND s.payload_digest = c.payload_digest AND s.born_generation = ?4 AND s.born_sequence = ?5
                     FROM inference_stream_claims s JOIN inference_claims c ON c.claim = s.claim AND c.operation = ?2
                     WHERE s.stream = ?1 AND s.claim = ?3",
                    params![stream, operation, identity, generation, sequence], |row| row.get(0),
                )?;
                if !equal {
                    return Err("stream retained baseline payload mismatch".into());
                }
                let posting_count: i64 = transaction.query_row(
                    "SELECT COUNT(*) FROM inference_stream_premises WHERE stream = ?1 AND claim = ?2",
                    params![stream, identity], |row| row.get(0),
                )?;
                if posting_count != i64::try_from(claim.premises.len())? {
                    return Err("stream retained baseline posting count mismatch".into());
                }
                for (ordinal, premise) in claim.premises.iter().enumerate() {
                    let posting_matches: bool = transaction.query_row(
                        "SELECT fact = ?4 AND valid_from = ?5 FROM inference_stream_premises WHERE stream = ?1 AND claim = ?2 AND ordinal = ?3",
                        params![stream, identity, i64::try_from(ordinal)?, bincode::serialize(&premise.fact)?, premise.valid_from.0.to_hex()],
                        |row| row.get(0),
                    )?;
                    if !posting_matches {
                        return Err("stream retained baseline posting mismatch".into());
                    }
                }
            }
        } else {
            transaction.execute(
                "INSERT INTO inference_streams VALUES (?1, ?2, ?3, ?4)",
                params![stream, operation, generation, sequence],
            )?;
            transaction.execute(
                "INSERT INTO inference_stream_checkpoints VALUES (?1, ?2, ?3, NULL, ?4, 0, 0, ?4)",
                params![stream, generation, sequence, count],
            )?;
            transaction.execute(
                "INSERT INTO inference_stream_claims SELECT ?1, claim, payload, payload_digest, ?3, ?4, NULL, NULL FROM inference_claims WHERE operation = ?2",
                params![stream, operation, generation, sequence],
            )?;
            transaction.execute(
                "INSERT INTO inference_stream_premises SELECT ?1, claim, ordinal, fact, valid_from FROM inference_premises WHERE operation = ?2",
                params![stream, operation],
            )?;
            let mut root = None;
            for claim in &claims {
                root = Some(tree.insert(root, claim.id)?);
            }
            transaction.execute(
                "INSERT INTO inference_stream_roots VALUES (?1, ?2, ?3)",
                params![stream, generation, root.map(StableId::to_hex)],
            )?;
        }
        transaction.commit()?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
