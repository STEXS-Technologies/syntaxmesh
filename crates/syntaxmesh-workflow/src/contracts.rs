//! Workflow and verification ports for the full SyntaxMesh engine.
//!
//! The direct coordinator is suitable for the reference host. The production
//! engine must provide adapters backed by Penelope and StateChronicle.

use syntaxmesh_core::{
    AcceptanceTime, GenerationId, GenerationManifest, GraphDelta, GraphDeltaWithConsequences,
    GraphDeltaWithLineage, IndexRunId, RepositoryId, WorktreeId,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowStatus {
    Pending,
    Running,
    Durable,
    VerificationPending,
    Verified,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationMode {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowReceipt {
    pub run_id: IndexRunId,
    pub generation: GenerationManifest,
    pub status: WorkflowStatus,
}

/// Typed reason retained for a terminal workflow rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRejectionReason {
    /// The indexed base generation changed before publication could commit.
    StaleBase {
        repository: RepositoryId,
        worktree: WorktreeId,
        expected: Option<GenerationId>,
        actual: Option<GenerationId>,
    },
    /// A legacy rejected record predates durable typed rejection details.
    UnknownLegacy,
}

/// Durable terminal rejection metadata for one indexing workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowRejection {
    /// Identity of the rejected indexing run.
    pub run_id: IndexRunId,
    /// Reason retained in the workflow journal.
    pub reason: WorkflowRejectionReason,
}

/// Cursor-paged, read-only view of terminal workflow rejections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRejectionPage {
    /// Rejected runs in deterministic run-ID order.
    pub items: Vec<WorkflowRejection>,
    /// Continue strictly after this run ID.
    pub next_cursor: Option<IndexRunId>,
}

#[derive(Debug)]
pub enum WorkflowError {
    Store(StoreError),
    Verification(String),
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for WorkflowError {}

impl From<StoreError> for WorkflowError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Durable workflow adapter boundary. Penelope owns the production
/// implementation; this trait deliberately contains no runtime or transport types.
pub trait DurableIndexWorkflow {
    /// Publish one accepted graph delta and return its durable receipt.
    ///
    /// # Errors
    /// Returns a workflow or store error when publication fails.
    fn publish(
        &mut self,
        delta: GraphDelta,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError>;

    /// Publish one accepted graph transition and its explicitly declared
    /// temporal lineage through the same durable workflow.
    ///
    /// # Errors
    /// Returns a workflow or store error when publication fails or the adapter
    /// cannot atomically journal lineage.
    fn publish_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        if !request.lineage.is_empty() {
            return Err(StoreError::Unsupported(
                "workflow does not persist explicit generation lineage".to_owned(),
            )
            .into());
        }
        self.publish(request.graph, accepted_at)
    }

    /// Publish graph, lineage, and explicitly declared temporal consequences atomically.
    ///
    /// # Errors
    /// Returns an error if the workflow cannot persist the complete publication.
    fn publish_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        if !request.consequences.is_empty() {
            return Err(StoreError::Unsupported(
                "workflow does not persist consequence history".to_owned(),
            )
            .into());
        }
        self.publish_with_lineage(request.publication, accepted_at)
    }
}

/// Optional generation verification boundary. StateChronicle owns the
/// production implementation; disabled verification must not change graph facts.
pub trait GenerationVerifier {
    /// Verify a durable generation and return the resulting status.
    ///
    /// # Errors
    /// Returns a verification error when the generation cannot be proven.
    fn verify(
        &mut self,
        generation: &GenerationManifest,
        records: &mut dyn DurableRecordStore,
    ) -> Result<WorkflowStatus, WorkflowError>;
}

/// Reference verifier used when verified history is disabled. It deliberately
/// leaves the durable generation unchanged; a StateChronicle adapter replaces
/// it when verification is enabled.
#[derive(Debug, Clone, Copy, Default)]
pub struct DisabledVerifier;

impl GenerationVerifier for DisabledVerifier {
    fn verify(
        &mut self,
        _generation: &GenerationManifest,
        _records: &mut dyn DurableRecordStore,
    ) -> Result<WorkflowStatus, WorkflowError> {
        Ok(WorkflowStatus::Durable)
    }
}

/// Reference coordinator for tests and the file-backed host. It provides
/// atomic store publication but intentionally does not claim Penelope durability
/// or StateChronicle verification.
pub struct DirectStoreWorkflow<S> {
    store: S,
}

impl<S> DirectStoreWorkflow<S>
where
    S: GraphStore,
{
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    #[must_use]
    pub const fn store(&self) -> &S {
        &self.store
    }

    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }
}

impl<S> DurableIndexWorkflow for DirectStoreWorkflow<S>
where
    S: GraphStore,
{
    fn publish(
        &mut self,
        delta: GraphDelta,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let run_id = delta.run_id;
        let generation = self.store.apply_delta_at(delta, accepted_at)?;
        Ok(WorkflowReceipt {
            run_id,
            generation,
            status: WorkflowStatus::Durable,
        })
    }

    fn publish_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let run_id = request.graph.run_id;
        let generation = self
            .store
            .apply_delta_with_lineage(request, Some(accepted_at))?;
        Ok(WorkflowReceipt {
            run_id,
            generation,
            status: WorkflowStatus::Durable,
        })
    }

    fn publish_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let run_id = request.publication.graph.run_id;
        let generation = self
            .store
            .apply_delta_with_consequences(request, Some(accepted_at))?;
        Ok(WorkflowReceipt {
            run_id,
            generation,
            status: WorkflowStatus::Durable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxmesh_core::{GenerationId, GraphDelta, IndexRunId, RepositoryId, WorktreeId};
    use syntaxmesh_store::InMemoryGraphStore;

    #[test]
    fn direct_workflow_reports_durable_store_publication() -> Result<(), WorkflowError> {
        let repository = RepositoryId::derive(&[b"repo"]);
        let worktree = WorktreeId::derive(&[b"worktree"]);
        let run_id = IndexRunId::derive(&[b"run"]);
        let generation = GenerationId::derive(&[b"generation"]);
        let mut workflow = DirectStoreWorkflow::new(InMemoryGraphStore::new());
        let receipt = workflow.publish(
            GraphDelta {
                repository,
                worktree,
                run_id,
                expected_base: None,
                next_generation: generation,
                changed_files: Vec::new(),
                removed_files: Vec::new(),
                upsert_provenance: Vec::new(),
                upsert_nodes: Vec::new(),
                upsert_edges: Vec::new(),
                remove_nodes: Vec::new(),
                remove_edges: Vec::new(),
            },
            syntaxmesh_core::AcceptanceTime(123),
        )?;
        if receipt.run_id != run_id
            || receipt.generation.generation != generation
            || receipt.status != WorkflowStatus::Durable
        {
            return Err(WorkflowError::Verification(
                "invalid durable receipt".to_owned(),
            ));
        }
        Ok(())
    }

    #[test]
    fn disabled_verification_preserves_durable_status() -> Result<(), WorkflowError> {
        let manifest = GenerationManifest {
            repository: RepositoryId::derive(&[b"repo"]),
            worktree: WorktreeId::derive(&[b"worktree"]),
            generation: GenerationId::derive(&[b"generation"]),
            parent: None,
            graph_root: [0; 32],
            configuration_hash: [0; 32],
            extractor_set_hash: [0; 32],
            schema_version: 1,
            status: syntaxmesh_core::GenerationStatus::Durable,
        };
        let mut verifier = DisabledVerifier;
        if verifier.verify(&manifest, &mut InMemoryGraphStore::new())? != WorkflowStatus::Durable {
            return Err(WorkflowError::Verification(
                "disabled verification changed durable status".to_owned(),
            ));
        }
        Ok(())
    }
}
