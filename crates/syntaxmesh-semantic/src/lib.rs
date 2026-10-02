//! Optional semantic enrichment contracts, isolated from deterministic indexing.

mod semantic;

pub use semantic::{
    SEMANTIC_NAMESPACE, SemanticClaim, SemanticDocumentChunk, SemanticError, SemanticEvidence,
    SemanticOutput, SemanticPromptChunk, SemanticProvider, SemanticProviderIdentity,
    SemanticRequest,
};
