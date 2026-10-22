use crate::{InferenceMaterializer, Result, Watermark};
use bincode::Options;
use rusqlite::params;
use syntaxmesh_inference::{Claim, ClaimSelection};

fn decode_claim_payload(payload: &[u8]) -> Result<Claim> {
    let limit = u64::try_from(payload.len())?;
    let options = || {
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(limit)
            .reject_trailing_bytes()
    };
    Ok(options().deserialize(payload)?)
}

impl InferenceMaterializer {
    /// Read a complete retained materialization without canonical-history replay.
    /// The receipt binds scope, configuration and sequence; all output is bounded.
    ///
    /// # Errors
    /// Rejects unknown/substituted receipts, exhausted bounds and inconsistent rows.
    pub fn claims(
        &self,
        watermark: Watermark,
        claim_budget: usize,
        byte_budget: usize,
    ) -> Result<Vec<Claim>> {
        read_claims(&self.connection, watermark, claim_budget, byte_budget)
    }
}

pub(crate) fn read_claims(
    connection: &rusqlite::Connection,
    watermark: Watermark,
    claim_budget: usize,
    byte_budget: usize,
) -> Result<Vec<Claim>> {
    if claim_budget == 0 || byte_budget == 0 || watermark.claim_count > claim_budget {
        return Err("materialized claim count/byte admission rejected".into());
    }
    verify_receipt(connection, watermark)?;
    let operation = watermark.operation.to_hex();
    let mut statement = connection.prepare(
            "SELECT claim, payload, payload_digest FROM inference_claims WHERE operation = ?1 ORDER BY claim",
        )?;
    let mut rows = statement.query([&operation])?;
    let mut remaining = byte_budget;
    let mut claims = Vec::new();
    while let Some(row) = rows.next()? {
        if claims.len() >= watermark.claim_count {
            return Err("materialized selection contains excess claims".into());
        }
        let claim = decode_claim(row, watermark, &mut remaining)?;
        let identity = claim.id.to_hex();
        let mut postings = connection.prepare("SELECT ordinal, fact, valid_from FROM inference_premises WHERE operation = ?1 AND claim = ?2 ORDER BY ordinal")?;
        let mut posted = postings.query(params![operation, identity])?;
        for (ordinal, premise) in claim.premises.iter().enumerate() {
            let retained = posted
                .next()?
                .ok_or("materialized witness posting missing")?;
            let actual_ordinal: i64 = retained.get(0)?;
            let fact = retained.get_ref(1)?.as_blob()?;
            let valid_from: String = retained.get(2)?;
            if actual_ordinal != i64::try_from(ordinal)?
                || fact != bincode::serialize(&premise.fact)?
                || valid_from != premise.valid_from.0.to_hex()
            {
                return Err("materialized witness posting mismatch".into());
            }
        }
        if posted.next()?.is_some() {
            return Err("materialized witness has excess postings".into());
        }
        claims.push(claim);
    }
    if claims.len() != watermark.claim_count {
        return Err("materialized selection is incomplete".into());
    }
    Ok(claims)
}

pub(crate) fn verify_receipt(
    connection: &rusqlite::Connection,
    watermark: Watermark,
) -> Result<()> {
    let operation = watermark.operation.to_hex();
    let count: i64 = connection.query_row(
            "SELECT claim_count FROM inference_operations WHERE operation = ?1 AND repository = ?2
             AND worktree = ?3 AND generation = ?4 AND generation_sequence = ?5 AND configuration = ?6",
            params![operation, watermark.selection.repository.0.to_hex(), watermark.selection.worktree.0.to_hex(), watermark.selection.generation.0.to_hex(), i64::try_from(watermark.generation_sequence)?, watermark.configuration.to_hex()],
            |row| row.get(0),
        )?;
    if usize::try_from(count)? != watermark.claim_count {
        return Err("materialization receipt claim count mismatch".into());
    }
    Ok(())
}

pub(crate) fn decode_claim(
    row: &rusqlite::Row<'_>,
    watermark: Watermark,
    remaining: &mut usize,
) -> Result<Claim> {
    decode_claim_at(row, watermark.selection, remaining)
}

pub(crate) fn decode_claim_at(
    row: &rusqlite::Row<'_>,
    selection: ClaimSelection,
    remaining: &mut usize,
) -> Result<Claim> {
    let payload = row.get_ref(1)?.as_blob()?;
    *remaining = remaining
        .checked_sub(payload.len())
        .ok_or("materialized claim byte budget exhausted")?;
    let digest = row.get_ref(2)?.as_str()?;
    if digest != blake3::hash(payload).to_hex().as_str() {
        return Err("materialized claim payload checksum mismatch".into());
    }
    let claim = decode_claim_payload(payload)?;
    let identity: String = row.get(0)?;
    if identity != claim.id.to_hex()
        || claim.repository != selection.repository
        || claim.worktree != selection.worktree
        || claim.generation != selection.generation
    {
        return Err("materialized claim identity or selection mismatch".into());
    }
    Ok(claim)
}
