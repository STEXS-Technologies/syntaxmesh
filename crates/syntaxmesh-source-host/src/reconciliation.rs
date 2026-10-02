use std::path::Path;
use syntaxmesh_core::{ChangeSetDelta, GenerationId, GraphDeltaWithLineage, IndexRunId};
use syntaxmesh_engine::{EngineError, SyntaxMeshEngine, source_inventory_fingerprint};
use syntaxmesh_language_sdk::{LanguageExtractor, SourceFile};
use syntaxmesh_scanner::{ScanError, scan};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};

#[cfg(test)]
mod tests;

/// Selected source generation and publication outcome for one structural pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceReconciliation {
    pub generation: GenerationId,
    pub published: bool,
}

/// Recovery, discovery, planning, or publication failure.
#[derive(Debug)]
pub enum SourceReconciliationError {
    Engine(EngineError),
    Scan(ScanError),
    Store(StoreError),
}

impl std::fmt::Display for SourceReconciliationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(error) => error.fmt(formatter),
            Self::Scan(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SourceReconciliationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Engine(error) => Some(error),
            Self::Scan(error) => Some(error),
            Self::Store(error) => Some(error),
        }
    }
}

/// Reconcile structural sources against an exclusively owned, configured Engine.
/// Callers hold the writer lease and supply fingerprints matching the installed
/// extractor and resolver. This does not run AI semantic extraction or reload
/// configuration. The root must use the same canonical scope as the Engine.
///
/// # Errors
/// Returns recovery, scanning, durable planning, or publication errors.
pub fn reconcile_structural_sources<S, E>(
    engine: &mut SyntaxMeshEngine<S, E>,
    root: &Path,
    extensions: &[&str],
    extractor_fingerprint: &[u8],
    resolver_fingerprint: &[u8],
) -> Result<SourceReconciliation, SourceReconciliationError>
where
    S: GraphStore + DurableRecordStore,
    E: LanguageExtractor,
{
    engine
        .recover_pending_workflows()
        .map_err(SourceReconciliationError::Engine)?;
    let files = scan(root, extensions)
        .map_err(SourceReconciliationError::Scan)?
        .files;
    reconcile_scanned_sources(engine, &files, extractor_fingerprint, resolver_fingerprint)
}

/// Reconcile an immutable scanned inventory using the shared planning and input
/// validation path. Callers retain exclusive writer ownership and provide policy
/// fingerprints matching the installed providers.
///
/// # Errors
/// Returns recovery, resolver input, planning, or publication errors.
pub fn reconcile_scanned_sources<S, E>(
    engine: &mut SyntaxMeshEngine<S, E>,
    files: &[SourceFile],
    extractor_fingerprint: &[u8],
    resolver_fingerprint: &[u8],
) -> Result<SourceReconciliation, SourceReconciliationError>
where
    S: GraphStore + DurableRecordStore,
    E: LanguageExtractor,
{
    engine
        .recover_pending_workflows()
        .map_err(SourceReconciliationError::Engine)?;
    let mut source_fingerprint =
        source_inventory_fingerprint(files).map_err(SourceReconciliationError::Store)?;
    source_fingerprint.extend_from_slice(resolver_fingerprint);
    let mut inputs = engine
        .persisted_module_inputs(resolver_fingerprint)
        .map_err(SourceReconciliationError::Store)?;
    for _ in 0..3 {
        inputs.extend(engine.observed_module_inputs().map_err(input_error)?);
        inputs.sort();
        inputs.dedup();
        let before = crate::module_input_fingerprint(&inputs).map_err(input_error)?;
        let mut fingerprint = source_fingerprint.clone();
        if !inputs.is_empty() {
            fingerprint.extend_from_slice(b"resolver-inputs-v2");
            fingerprint.extend_from_slice(&before);
        }
        let (generation, published) = engine
            .prepare_source_index(files, extractor_fingerprint, &fingerprint)
            .map_err(SourceReconciliationError::Store)?;
        let retry_failed = !published
            && engine.source_syntax_policy()
                == syntaxmesh_engine::SourceSyntaxPolicy::RecordFailures
            && !engine
                .source_processing_coverage_at(generation)
                .map_err(SourceReconciliationError::Engine)?
                .syntax_failed
                .is_empty();
        if !published && !retry_failed {
            return Ok(SourceReconciliation {
                generation,
                published,
            });
        }
        let prepared_generation = if retry_failed {
            GenerationId::derive(&[
                b"source-syntax-retry-v1",
                &generation.0.0,
                extractor_fingerprint,
                &fingerprint,
            ])
        } else {
            generation
        };
        let graph = engine
            .prepare_source_delta(
                files,
                IndexRunId::derive(&[prepared_generation.0.0.as_slice()]),
                prepared_generation,
            )
            .map_err(SourceReconciliationError::Engine)?;
        let observed = engine.observed_module_inputs().map_err(input_error)?;
        if observed
            .iter()
            .any(|input| inputs.binary_search(input).is_err())
        {
            inputs.extend(observed);
            continue;
        }
        if before != crate::module_input_fingerprint(&inputs).map_err(input_error)? {
            return Err(input_error(
                "resolver inputs changed during source preparation",
            ));
        }
        if retry_failed {
            if !graph.changed_files.is_empty()
                || !graph.removed_files.is_empty()
                || !graph.upsert_nodes.is_empty()
                || !graph.remove_nodes.is_empty()
                || !graph.upsert_edges.is_empty()
                || !graph.remove_edges.is_empty()
                || !graph.upsert_provenance.is_empty()
            {
                return Err(SourceReconciliationError::Store(StoreError::Integrity("unchanged failed-source retry changed canonical facts; update producer identity".to_owned())));
            }
            return Ok(SourceReconciliation {
                generation,
                published: false,
            });
        }
        engine
            .persist_module_inputs(resolver_fingerprint)
            .map_err(SourceReconciliationError::Store)?;
        engine
            .publish_prepared_with_lineage(GraphDeltaWithLineage {
                graph,
                lineage: ChangeSetDelta::default(),
            })
            .map_err(SourceReconciliationError::Engine)?;
        return Ok(SourceReconciliation {
            generation,
            published,
        });
    }
    Err(input_error(
        "resolver input discovery did not converge in three passes",
    ))
}

fn input_error(error: impl std::fmt::Display) -> SourceReconciliationError {
    SourceReconciliationError::Store(StoreError::Backend(format!("resolver inputs: {error}")))
}
