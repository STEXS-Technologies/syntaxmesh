use super::{CLAIMS_SQL, checkpoint, decode_active, verify_origin};
use crate::{InferenceMaterializer, Result, StreamCheckpoint, StreamOrigin};
use rusqlite::params;
use syntaxmesh_core::{GenerationId, StableId};
use syntaxmesh_inference::{Claim, ClaimSelection};

/// Exclusive continuation bound to an immutable historical selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StreamCursor {
    pub stream: StableId,
    pub selection: ClaimSelection,
    pub root: Option<StableId>,
    pub after: StableId,
}

/// Independent limits apply to each page, including cursor verification.
#[derive(Debug, Clone, Copy)]
pub struct StreamPageRequest {
    pub origin: StreamOrigin,
    pub generation: GenerationId,
    pub cursor: Option<StreamCursor>,
    pub claim_budget: usize,
    pub byte_budget: usize,
    pub premise_budget: usize,
    pub page_budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StreamPage {
    pub checkpoint: StreamCheckpoint,
    pub claims: Vec<Claim>,
    pub next: Option<StreamCursor>,
}

impl InferenceMaterializer {
    /// Seek an identity-ordered page at an exact retained checkpoint.
    ///
    /// # Errors
    /// Rejects substituted cursors, corrupt selected data and exhausted bounds.
    pub fn stream_claims_page(&self, request: &StreamPageRequest) -> Result<StreamPage> {
        if request.claim_budget == 0
            || request.byte_budget == 0
            || request.premise_budget == 0
            || request.page_budget == 0
        {
            return Err("positive stream page admission required".into());
        }
        let transaction = self.connection.unchecked_transaction()?;
        let origin = request.origin;
        let head = verify_origin(&transaction, origin)?;
        let selected = checkpoint(&transaction, origin, request.generation)?
            .ok_or("stream page checkpoint not retained")?;
        if selected.sequence > head.sequence {
            return Err("stream page exceeds accepted coverage".into());
        }
        let root = crate::stream::tree::root(&transaction, origin.stream, request.generation)?;
        let mut tree = crate::stream::tree::ActiveTree::new(&transaction, request.page_budget)?;
        if let Some(cursor) = request.cursor {
            if cursor.stream != origin.stream
                || cursor.selection != selected.selection
                || cursor.root != root
            {
                return Err("stream cursor selection substitution".into());
            }
            if !tree.membership(root, cursor.after, selected.active_claims)? {
                return Err("stream cursor key not active".into());
            }
        }
        let lookahead = request
            .claim_budget
            .checked_add(1)
            .ok_or("stream page output limit overflow")?;
        let mut keys = tree.page(
            root,
            request.cursor.map(|cursor| cursor.after),
            lookahead,
            selected.active_claims,
        )?;
        let more = keys.len() > request.claim_budget;
        if more {
            keys.pop();
        }
        let next = if more {
            Some(StreamCursor {
                stream: origin.stream,
                selection: selected.selection,
                root,
                after: *keys.last().ok_or("stream page continuation missing key")?,
            })
        } else {
            None
        };
        let mut remaining_bytes = request.byte_budget;
        let mut remaining_premises = request.premise_budget;
        let mut claims = Vec::new();
        let mut statement = transaction.prepare(CLAIMS_SQL)?;
        for identity in keys {
            let mut rows = statement.query(params![origin.stream.to_hex(), identity.to_hex()])?;
            let row = rows.next()?.ok_or("stream page active claim missing")?;
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
        drop(statement);
        transaction.commit()?;
        Ok(StreamPage {
            checkpoint: selected,
            claims,
            next,
        })
    }
}

#[cfg(test)]
mod tests;
