use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use syntaxmesh_core::{GenerationId, StableId};
use syntaxmesh_inference::{
    Claim, ClaimSelection, Limits, OntologyRegistry, Rule, materialization_identity,
    rule_set_identity,
};
use syntaxmesh_query::Query;
use syntaxmesh_store::GraphStore;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A complete, immutable retained selection, not a canonical verification proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Watermark {
    pub operation: StableId,
    pub selection: ClaimSelection,
    pub generation_sequence: u64,
    pub configuration: StableId,
    pub claim_count: usize,
}

/// Explicit execution controls are not part of semantic operation identity.
pub struct RebuildRequest<'rules> {
    pub generation: GenerationId,
    pub rules: &'rules [Rule],
    pub limits: &'rules Limits,
    pub history_scan_budget: usize,
    pub rule_budget: usize,
    pub output_bytes: usize,
    pub premise_budget: usize,
}

pub struct InferenceMaterializer {
    pub(crate) connection: Connection,
}

impl InferenceMaterializer {
    /// Explicitly bootstrap the downstream schema; ordinary open never migrates.
    ///
    /// # Errors
    /// Rejects SQL errors and incompatible or tampered migration ledgers.
    pub fn migrate(path: impl AsRef<Path>) -> Result<()> {
        crate::migrations::migrate(path.as_ref())
    }

    /// Open an explicitly migrated downstream database without creating it.
    ///
    /// # Errors
    /// Rejects absent, unmigrated, newer or ledger-inconsistent databases.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            connection: crate::migrations::open(path.as_ref())?,
        })
    }

    /// Evaluate a pinned selection and atomically retain claims, postings and watermark.
    /// Historical rebuilds do not regress the latest configuration watermark.
    ///
    /// # Errors
    /// Evaluation, admission or SQL failures preserve every accepted selection.
    pub fn rebuild<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        request: &RebuildRequest<'_>,
    ) -> Result<Watermark> {
        if request.output_bytes == 0 || request.premise_budget == 0 {
            return Err("positive materialization output and premise budgets required".into());
        }
        let manifest = store.manifest(request.generation)?;
        let selection = ClaimSelection {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: request.generation,
        };
        let sequence = store.generation_sequence(request.generation)?;
        let claims = Query::new(store, request.generation).infer_rules(
            request.rules,
            request.limits,
            request.history_scan_budget,
            request.rule_budget,
        )?;
        self.publish_rebuild(request, selection, sequence, request.rules, &claims)
    }

    /// Evaluate and retain ontology-validated claims under their bound identity.
    ///
    /// # Errors
    /// Rejects invalid definitions/facts, exhausted bounds or SQL failures
    /// without publishing a partial materialization.
    pub fn rebuild_registered<S: GraphStore + ?Sized>(
        &mut self,
        store: &S,
        request: &RebuildRequest<'_>,
        registry: &OntologyRegistry,
        binding_budget: usize,
    ) -> Result<Watermark> {
        if request.output_bytes == 0 || request.premise_budget == 0 {
            return Err("positive materialization output and premise budgets required".into());
        }
        let bound_rules = registry.bind_rules(
            request.rules,
            request.rule_budget,
            binding_budget,
            request.limits.input_bytes,
        )?;
        let manifest = store.manifest(request.generation)?;
        let selection = ClaimSelection {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: request.generation,
        };
        let sequence = store.generation_sequence(request.generation)?;
        let claims = Query::new(store, request.generation).infer_bound(
            registry,
            &bound_rules,
            request.limits,
            request.history_scan_budget,
            request.rule_budget,
            binding_budget,
        )?;
        self.publish_rebuild(request, selection, sequence, bound_rules.rules(), &claims)
    }

    fn publish_rebuild(
        &mut self,
        request: &RebuildRequest<'_>,
        selection: ClaimSelection,
        sequence: u64,
        rules: &[Rule],
        claims: &[Claim],
    ) -> Result<Watermark> {
        let operation = materialization_identity(
            selection,
            rules,
            request.rule_budget,
            request.limits.input_bytes,
        )?;
        let configuration =
            rule_set_identity(rules, request.rule_budget, request.limits.input_bytes)?;
        let mut remaining_bytes = u64::try_from(request.output_bytes)?;
        let mut remaining_premises = request.premise_budget;
        let mut payloads = Vec::with_capacity(claims.len());
        for claim in claims {
            remaining_bytes = remaining_bytes
                .checked_sub(bincode::serialized_size(claim)?)
                .ok_or("materialization output byte budget exhausted")?;
            remaining_premises = remaining_premises
                .checked_sub(claim.premises.len())
                .ok_or("materialization premise budget exhausted")?;
            payloads.push(bincode::serialize(claim)?);
        }
        let watermark = Watermark {
            operation,
            selection,
            generation_sequence: sequence,
            configuration,
            claim_count: claims.len(),
        };
        self.publish(watermark, claims, &payloads)?;
        Ok(watermark)
    }

    fn publish(
        &mut self,
        watermark: Watermark,
        claims: &[Claim],
        payloads: &[Vec<u8>],
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let operation = watermark.operation.to_hex();
        let sequence = i64::try_from(watermark.generation_sequence)?;
        let claim_count = i64::try_from(watermark.claim_count)?;
        let existing: Option<(i64, i64)> = transaction.query_row(
            "SELECT generation_sequence, claim_count FROM inference_operations WHERE operation = ?1",
            [&operation], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        if let Some(previous) = existing {
            if previous != (sequence, claim_count) {
                return Err("existing inference operation differs from complete rebuild".into());
            }
            for (claim, payload) in claims.iter().zip(payloads) {
                let equal: bool = transaction.query_row(
                    "SELECT payload = ?3 AND payload_digest = ?4 FROM inference_claims WHERE operation = ?1 AND claim = ?2",
                    params![operation, claim.id.to_hex(), payload, blake3::hash(payload).to_hex().to_string()],
                    |row| row.get(0),
                )?;
                if !equal {
                    return Err("existing inference claim differs from complete rebuild".into());
                }
                let posting_count: i64 = transaction.query_row(
                    "SELECT COUNT(*) FROM inference_premises WHERE operation = ?1 AND claim = ?2",
                    params![operation, claim.id.to_hex()],
                    |row| row.get(0),
                )?;
                if posting_count != i64::try_from(claim.premises.len())? {
                    return Err("existing inference witness posting count differs".into());
                }
                for (ordinal, premise) in claim.premises.iter().enumerate() {
                    let matched: bool = transaction.query_row(
                        "SELECT fact = ?4 AND valid_from = ?5 FROM inference_premises
                         WHERE operation = ?1 AND claim = ?2 AND ordinal = ?3",
                        params![
                            operation,
                            claim.id.to_hex(),
                            i64::try_from(ordinal)?,
                            bincode::serialize(&premise.fact)?,
                            premise.valid_from.0.to_hex()
                        ],
                        |row| row.get(0),
                    )?;
                    if !matched {
                        return Err("existing inference witness posting differs".into());
                    }
                }
            }
            transaction.commit()?;
            return Ok(());
        }
        transaction.execute(
            "INSERT INTO inference_operations VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                operation,
                watermark.selection.repository.0.to_hex(),
                watermark.selection.worktree.0.to_hex(),
                watermark.selection.generation.0.to_hex(),
                sequence,
                watermark.configuration.to_hex(),
                claim_count
            ],
        )?;
        for (claim, payload) in claims.iter().zip(payloads) {
            let identity = claim.id.to_hex();
            transaction.execute(
                "INSERT INTO inference_claims VALUES (?1, ?2, ?3, ?4)",
                params![
                    operation,
                    identity,
                    payload,
                    blake3::hash(payload).to_hex().to_string()
                ],
            )?;
            for (ordinal, premise) in claim.premises.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO inference_premises VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        operation,
                        identity,
                        i64::try_from(ordinal)?,
                        bincode::serialize(&premise.fact)?,
                        premise.valid_from.0.to_hex()
                    ],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO inference_lifecycle(operation, through_generation, through_sequence) VALUES (?1, ?2, ?3)",
            params![operation, watermark.selection.generation.0.to_hex(), sequence],
        )?;
        transaction.execute(
            "INSERT INTO inference_lifecycle_receipts VALUES (?1, ?2, ?3, 0, 0)",
            params![
                operation,
                watermark.selection.generation.0.to_hex(),
                sequence
            ],
        )?;
        transaction.execute(
            "INSERT INTO inference_watermarks VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(repository, worktree, configuration) DO UPDATE SET
             operation = excluded.operation, generation_sequence = excluded.generation_sequence
             WHERE excluded.generation_sequence >= inference_watermarks.generation_sequence",
            params![
                watermark.selection.repository.0.to_hex(),
                watermark.selection.worktree.0.to_hex(),
                watermark.configuration.to_hex(),
                operation,
                sequence
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
