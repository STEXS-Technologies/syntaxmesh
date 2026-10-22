use super::{EngineError, SyntaxMeshEngine};
use std::path::Path;
use syntaxmesh_core::GenerationId;
use syntaxmesh_inference_sqlite::{
    InferenceMaterializer, StreamCheckpoint, StreamOrigin, StreamPage, StreamPageRequest,
};
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_store::GraphStore;

impl<S: GraphStore, E: LanguageExtractor> SyntaxMeshEngine<S, E> {
    /// Read a retained downstream checkpoint without canonical or rule replay.
    /// Opening does not migrate, recover or publish effects.
    ///
    /// # Errors
    /// Rejects foreign scope, unavailable storage and corrupt retained coordinates.
    pub fn inference_checkpoint(
        &self,
        database: impl AsRef<Path>,
        origin: StreamOrigin,
        generation: GenerationId,
    ) -> Result<Option<StreamCheckpoint>, EngineError> {
        self.inference_read_scope(origin)?;
        let materializer = InferenceMaterializer::open(database).map_err(read_error)?;
        materializer
            .stream_checkpoint(origin, generation)
            .map_err(read_error)
    }

    /// Seek a bounded page at an immutable downstream historical selection.
    /// Cursor and selected payload validation are performed by the materializer.
    ///
    /// # Errors
    /// Rejects foreign scope, substituted cursors, corrupt data and exhausted bounds.
    pub fn inference_claims_page(
        &self,
        database: impl AsRef<Path>,
        request: &StreamPageRequest,
    ) -> Result<StreamPage, EngineError> {
        self.inference_read_scope(request.origin)?;
        let materializer = InferenceMaterializer::open(database).map_err(read_error)?;
        materializer.stream_claims_page(request).map_err(read_error)
    }

    fn inference_read_scope(&self, origin: StreamOrigin) -> Result<(), EngineError> {
        let selection = origin.baseline.selection;
        if selection.repository != self.repository || selection.worktree != self.worktree {
            return Err(read_error("inference read outside engine scope"));
        }
        Ok(())
    }
}

fn read_error(error: impl std::fmt::Display) -> EngineError {
    syntaxmesh_workflow::WorkflowError::Store(syntaxmesh_store::StoreError::InvalidDelta(
        error.to_string(),
    ))
    .into()
}
