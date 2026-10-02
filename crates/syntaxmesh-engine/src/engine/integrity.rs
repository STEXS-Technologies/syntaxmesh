use super::{EngineError, SyntaxMeshEngine};
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_store::{
    BackendIntegrityCheck, BackendIntegrityReport, DurableRecordStore, GraphStore,
};

impl<S, E> SyntaxMeshEngine<S, E>
where
    S: GraphStore + DurableRecordStore + BackendIntegrityCheck,
    E: LanguageExtractor,
{
    /// Inspect the owned backend's durable representation without recovery or publication.
    ///
    /// # Errors
    /// Propagates backend inspection errors. A completed failed check is a report.
    pub fn backend_integrity_check(&self) -> Result<BackendIntegrityReport, EngineError> {
        Ok(self.indexer.store().backend_integrity_check()?)
    }
}
