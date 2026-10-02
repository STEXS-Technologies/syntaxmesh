//! Explicit processing coverage over one retained canonical snapshot.

use std::collections::BTreeMap;
use syntaxmesh_core::{FileId, GenerationId, GraphSnapshot, NodeKind};
use syntaxmesh_language_sdk::{SOURCE_PROCESSING_NAMESPACE, SourceProcessing, SyntaxDiagnostic};
use syntaxmesh_store::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProcessingCoverage {
    pub generation: GenerationId,
    pub completed: Vec<FileId>,
    pub syntax_failed: Vec<(FileId, SyntaxDiagnostic)>,
    /// Observed files with no explicit processing evidence, including legacy facts.
    pub unclassified: Vec<FileId>,
}

pub(crate) fn summarize(
    generation: GenerationId,
    snapshot: &GraphSnapshot,
) -> Result<SourceProcessingCoverage, StoreError> {
    let files = snapshot
        .files
        .iter()
        .map(|file| (file.file_id, file))
        .collect::<BTreeMap<_, _>>();
    let provenance = snapshot
        .provenance
        .iter()
        .map(|item| (item.id, item))
        .collect::<BTreeMap<_, _>>();
    let mut outcomes = BTreeMap::new();
    for node in &snapshot.nodes {
        if !matches!(&node.kind, NodeKind::External { namespace, .. } if namespace == SOURCE_PROCESSING_NAMESPACE)
        {
            continue;
        }
        let file = node
            .owner_file
            .and_then(|id| files.get(&id))
            .ok_or_else(|| {
                StoreError::Integrity("processing evidence has no observed owner file".to_owned())
            })?;
        let evidence = provenance.get(&node.provenance).ok_or_else(|| {
            StoreError::Integrity("processing evidence has no provenance".to_owned())
        })?;
        let outcome = SourceProcessing::from_facts(file, node, evidence)
            .map_err(|error| StoreError::Integrity(error.to_string()))?
            .ok_or_else(|| {
                StoreError::Integrity("recognized processing fact has no outcome".to_owned())
            })?;
        if outcomes.insert(file.file_id, outcome).is_some() {
            return Err(StoreError::Integrity(
                "duplicate file processing evidence".to_owned(),
            ));
        }
    }
    let mut result = SourceProcessingCoverage {
        generation,
        completed: Vec::new(),
        syntax_failed: Vec::new(),
        unclassified: Vec::new(),
    };
    for file_id in files.keys() {
        match outcomes.remove(file_id) {
            Some(SourceProcessing::Completed) => result.completed.push(*file_id),
            Some(SourceProcessing::SyntaxFailed(diagnostic)) => {
                result.syntax_failed.push((*file_id, diagnostic))
            }
            None => result.unclassified.push(*file_id),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
