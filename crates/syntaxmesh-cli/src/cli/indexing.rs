use std::path::Path;

use syntaxmesh_core::{GenerationId, IndexRunId, RepositoryId, WorktreeId};
use syntaxmesh_engine::{SemanticBatchLimits, SourceSyntaxPolicy, SyntaxMeshEngine};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_scanner::{SUPPORTED_SOURCE_EXTENSIONS, scan};
use syntaxmesh_semantic::{SemanticOutput, SemanticProvider};
use syntaxmesh_store::{DurableRecordStore, FileGraphStore, GraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

use super::semantic::OpenAiCompatibleProvider;
use super::{CliError, project_config, write_stdout};

#[cfg(test)]
mod tests;

const DEFAULT_SEMANTIC_MODEL: &str = "qwen3:latest";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct IndexOptions {
    verification_override: Option<bool>,
    semantic: Option<SemanticOptions>,
    syntax_policy: SourceSyntaxPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticOptions {
    model: String,
    model_revision: Option<String>,
    endpoint: Option<String>,
    api_key_env: Option<String>,
    allow_network: bool,
    offline: bool,
    cross_document: bool,
    command: Option<Vec<String>>,
}

pub(super) fn index(root: &Path, snapshot: &Path, options: &IndexOptions) -> Result<(), CliError> {
    let _ownership = super::ownership::IndexOwnership::acquire(snapshot)?;
    index_owned(root, snapshot, options, false)
}

pub(super) fn index_owned(
    root: &Path,
    target: &Path,
    options: &IndexOptions,
    turso: bool,
) -> Result<(), CliError> {
    let resolved = match std::fs::canonicalize(target) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = target
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let name = target
                .file_name()
                .ok_or_else(|| CliError::Usage("store needs a filename".to_owned()))?;
            std::fs::canonicalize(parent)?.join(name)
        }
        Err(error) => return Err(error.into()),
    };
    let verify = verification_policy(root, options.verification_override)?;
    let provider = make_semantic_provider(options.semantic.as_ref())?;
    if turso {
        index_with_store(
            root,
            TursoGraphStore::open(&resolved)?,
            verify,
            provider.as_ref(),
            options.syntax_policy,
        )
    } else {
        index_with_store(
            root,
            FileGraphStore::open(&resolved)?,
            verify,
            provider.as_ref(),
            options.syntax_policy,
        )
    }
}

pub(super) fn index_turso(
    root: &Path,
    database: &Path,
    options: &IndexOptions,
) -> Result<(), CliError> {
    let _ownership = super::ownership::IndexOwnership::acquire(database)?;
    index_owned(root, database, options, true)
}

pub(super) fn index_options(
    arguments: impl IntoIterator<Item = String>,
) -> Result<IndexOptions, CliError> {
    let mut options = IndexOptions::default();
    let mut semantic_model = None;
    let mut semantic_model_explicit = false;
    let mut model_revision = None;
    let mut endpoint = None;
    let mut api_key_env = None;
    let mut allow_network = false;
    let mut offline = false;
    let mut cross_document = false;
    let mut command = None;
    let mut verify = false;
    let mut arguments = arguments.into_iter().peekable();
    while let Some(argument) = arguments.next() {
        if argument == "--record-syntax-failures"
            && options.syntax_policy == SourceSyntaxPolicy::Strict
        {
            options.syntax_policy = SourceSyntaxPolicy::RecordFailures;
        } else if argument == "--verify" && !verify {
            verify = true;
            options.verification_override = Some(true);
        } else if argument == "--allow-network" && !allow_network {
            allow_network = true;
        } else if argument == "--semantic-offline" && !offline {
            offline = true;
        } else if argument == "--semantic-cross-document" && !cross_document {
            cross_document = true;
        } else if argument == "--semantic-command" || argument.starts_with("--semantic-command=") {
            let value = if let Some(value) = argument.strip_prefix("--semantic-command=") {
                value.to_owned()
            } else {
                arguments.next().ok_or_else(|| {
                    CliError::Usage("--semantic-command requires a JSON argv array".to_owned())
                })?
            };
            let argv = OpenAiCompatibleProvider::command_argv(&value)?;
            if command.replace(argv).is_some() {
                return Err(CliError::Usage(
                    "--semantic-command may be specified once".to_owned(),
                ));
            }
        } else if argument == "--semantic" {
            if semantic_model.is_some() {
                return Err(CliError::Usage(
                    "--semantic may be specified once".to_owned(),
                ));
            }
            let selected = arguments.next_if(|value| !value.starts_with('-'));
            semantic_model_explicit = selected.is_some();
            let model = selected.unwrap_or_else(|| DEFAULT_SEMANTIC_MODEL.to_owned());
            if model.trim().is_empty() {
                return Err(CliError::Usage(
                    "--semantic model name must not be empty".to_owned(),
                ));
            }
            semantic_model = Some(model);
        } else if let Some(model) = argument.strip_prefix("--semantic=") {
            semantic_model_explicit = true;
            if semantic_model.replace(model.to_owned()).is_some() || model.trim().is_empty() {
                return Err(CliError::Usage(
                    "--semantic requires one non-empty model name".to_owned(),
                ));
            }
        } else if argument == "--semantic-model-revision" {
            let value = arguments.next().ok_or_else(|| {
                CliError::Usage("--semantic-model-revision requires a revision".to_owned())
            })?;
            if value.starts_with('-')
                || value.trim().is_empty()
                || model_revision.replace(value).is_some()
            {
                return Err(CliError::Usage(
                    "--semantic-model-revision requires one non-empty revision".to_owned(),
                ));
            }
        } else if let Some(value) = argument.strip_prefix("--semantic-model-revision=") {
            if value.trim().is_empty() || model_revision.replace(value.to_owned()).is_some() {
                return Err(CliError::Usage(
                    "--semantic-model-revision requires one non-empty revision".to_owned(),
                ));
            }
        } else if argument == "--semantic-endpoint" {
            let value = arguments
                .next()
                .ok_or_else(|| CliError::Usage("--semantic-endpoint requires a URL".to_owned()))?;
            if value.starts_with('-') || endpoint.replace(value).is_some() {
                return Err(CliError::Usage(
                    "--semantic-endpoint requires one URL".to_owned(),
                ));
            }
        } else if let Some(value) = argument.strip_prefix("--semantic-endpoint=") {
            if endpoint.replace(value.to_owned()).is_some() {
                return Err(CliError::Usage(
                    "--semantic-endpoint may be specified once".to_owned(),
                ));
            }
        } else if argument == "--semantic-api-key-env" {
            let value = arguments.next().ok_or_else(|| {
                CliError::Usage("--semantic-api-key-env requires a variable name".to_owned())
            })?;
            if value.starts_with('-') || api_key_env.replace(value).is_some() {
                return Err(CliError::Usage(
                    "--semantic-api-key-env requires one variable name".to_owned(),
                ));
            }
        } else if let Some(value) = argument.strip_prefix("--semantic-api-key-env=") {
            if api_key_env.replace(value.to_owned()).is_some() {
                return Err(CliError::Usage(
                    "--semantic-api-key-env may be specified once".to_owned(),
                ));
            }
        } else if argument.starts_with('-') {
            return Err(CliError::Usage(format!(
                "unknown or duplicate index option: {argument}"
            )));
        } else {
            return Err(CliError::Usage(
                "index accepts [root] [snapshot] and documented options".to_owned(),
            ));
        }
    }
    if command.is_some() && semantic_model.is_some() && !semantic_model_explicit {
        semantic_model = Some("gpt-6-luna".to_owned());
    }
    if semantic_model.is_none()
        && (endpoint.is_some()
            || api_key_env.is_some()
            || model_revision.is_some()
            || allow_network
            || offline
            || cross_document
            || command.is_some())
    {
        return Err(CliError::Usage(
            "semantic endpoint, key, and network options require --semantic".to_owned(),
        ));
    }
    if offline && (allow_network || model_revision.is_none()) {
        return Err(CliError::Usage(
            "--semantic-offline requires --semantic-model-revision and conflicts with --allow-network".to_owned(),
        ));
    }
    options.semantic = semantic_model.map(|model| SemanticOptions {
        model,
        model_revision,
        endpoint,
        api_key_env,
        allow_network,
        offline,
        cross_document,
        command,
    });
    Ok(options)
}

pub(super) fn index_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<(std::path::PathBuf, std::path::PathBuf, IndexOptions), CliError> {
    let mut arguments = arguments.into_iter().peekable();
    let root = match arguments.next_if(|value| !value.starts_with('-')) {
        Some(root) => std::path::PathBuf::from(root),
        None => std::env::current_dir()?,
    };
    let explicit_snapshot = arguments.next_if(|value| !value.starts_with('-'));
    let options = index_options(arguments)?;
    let snapshot = match explicit_snapshot {
        Some(snapshot) => std::path::PathBuf::from(snapshot),
        None => {
            let directory = root.join(".syntaxmesh");
            std::fs::create_dir_all(&directory)?;
            directory.join("index.snapshot")
        }
    };
    Ok((root, snapshot, options))
}

fn make_semantic_provider(
    options: Option<&SemanticOptions>,
) -> Result<Option<OpenAiCompatibleProvider>, CliError> {
    let Some(options) = options else {
        return Ok(None);
    };
    if let Some(argv) = &options.command {
        if options.endpoint.is_some() || options.api_key_env.is_some() || options.allow_network {
            return Err(CliError::Usage("semantic command uses harness-owned auth/network, not HTTP endpoint/key/network options".to_owned()));
        }
        let mut provider = OpenAiCompatibleProvider::for_command(
            argv.clone(),
            &options.model,
            options.model_revision.as_deref(),
        )?;
        provider.set_offline(options.offline);
        provider.set_cross_document(options.cross_document);
        return Ok(Some(provider));
    }
    let key_variable = options
        .api_key_env
        .as_deref()
        .unwrap_or("SYNTAXMESH_SEMANTIC_API_KEY");
    let api_key = match std::env::var(key_variable) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        Ok(_) => None,
        Err(std::env::VarError::NotPresent) if options.api_key_env.is_some() => {
            return Err(CliError::Usage(format!(
                "semantic API key environment variable {key_variable} is not set"
            )));
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(CliError::Usage(format!(
                "semantic API key environment variable {key_variable} is not valid Unicode"
            )));
        }
    };
    let mut provider = OpenAiCompatibleProvider::new(
        options.endpoint.as_deref(),
        &options.model,
        api_key,
        options.allow_network || options.offline,
    )?;
    provider.set_offline(options.offline);
    provider.set_cross_document(options.cross_document);
    provider.resolve_model_revision(options.model_revision.as_deref())?;
    Ok(Some(provider))
}

pub(super) fn init_project(arguments: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    let mut root = None;
    let mut verified_history = false;
    let mut saw_verified_flag = false;
    let mut node_module_resolution = false;
    let mut saw_node_module_resolution_flag = false;
    let mut python_module_resolution = false;
    let mut saw_python_module_resolution_flag = false;
    for argument in arguments {
        if argument == "--verified-history" && !saw_verified_flag {
            verified_history = true;
            saw_verified_flag = true;
        } else if argument == "--node-module-resolution" && !saw_node_module_resolution_flag {
            node_module_resolution = true;
            saw_node_module_resolution_flag = true;
        } else if argument == "--python-module-resolution" && !saw_python_module_resolution_flag {
            python_module_resolution = true;
            saw_python_module_resolution_flag = true;
        } else if argument.starts_with('-') {
            return Err(CliError::Usage(format!(
                "unknown or duplicate init option: {argument}"
            )));
        } else if root.is_none() {
            root = Some(argument);
        } else {
            return Err(CliError::Usage(
                "init accepts at most one project-root".to_owned(),
            ));
        }
    }
    let root = match root {
        Some(root) => std::path::PathBuf::from(root),
        None => std::env::current_dir()?,
    };
    let mut module_resolution = Vec::new();
    if node_module_resolution {
        module_resolution.push(project_config::ModuleResolutionProfile::Node);
    }
    if python_module_resolution {
        module_resolution.push(project_config::ModuleResolutionProfile::Python);
    }
    let config_path = project_config::init_project(&root, verified_history, &module_resolution)?;
    write_stdout(&format!(
        "initialized {} (verified_history={verified_history}, node_module_resolution={node_module_resolution}, python_module_resolution={python_module_resolution})",
        config_path.display()
    ))
}

pub(super) use syntaxmesh_source_host::repository_scope;

fn index_with_store<S>(
    root: &Path,
    opened_store: S,
    verify: bool,
    semantic_provider: Option<&OpenAiCompatibleProvider>,
    syntax_policy: SourceSyntaxPolicy,
) -> Result<(), CliError>
where
    S: GraphStore + DurableRecordStore,
{
    let canonical_root = std::fs::canonicalize(root)?;
    let (repository, worktree) = repository_scope(&canonical_root);
    let mut recovery = SyntaxMeshEngine::new(opened_store, RustExtractor, repository, worktree);
    recovery.recover_pending_workflows()?;
    let store = recovery.into_store();
    let project_config = project_config::ProjectConfig::load(&canonical_root)?;
    let resolver_setup =
        syntaxmesh_source_host::project_resolvers(&canonical_root, &project_config).map_err(
            |error| match error {
                syntaxmesh_source_host::ProjectResolverError::Node(cause) => {
                    CliError::ModuleResolution(cause)
                }
                syntaxmesh_source_host::ProjectResolverError::Python(message) => {
                    CliError::PythonModuleResolution(message)
                }
            },
        )?;
    let module_resolution_provider = resolver_setup.provider;
    let scan_report = scan(&canonical_root, SUPPORTED_SOURCE_EXTENSIONS)?;
    let scan_metrics = scan_report.metrics;
    let files = scan_report.files;
    let extractor = syntaxmesh_source_host::supported_source_extractors()
        .map_err(|error| CliError::Usage(error.to_string()))?;
    let extractor_fingerprint = extractor.configuration_fingerprint();
    let mut engine = SyntaxMeshEngine::new(store, extractor, repository, worktree)
        .with_source_syntax_policy(syntax_policy);
    if let Some(provider) = module_resolution_provider {
        engine = engine.with_module_resolution_provider(provider);
    }
    if verify {
        engine = engine.with_statechronicle_verification();
    }
    let reconciliation = syntaxmesh_source_host::reconcile_scanned_sources(
        &mut engine,
        &files,
        &extractor_fingerprint,
        &resolver_setup.fingerprint,
    )
    .map_err(|error| match error {
        syntaxmesh_source_host::SourceReconciliationError::Engine(cause) => CliError::Engine(cause),
        syntaxmesh_source_host::SourceReconciliationError::Scan(cause) => CliError::Scan(cause),
        syntaxmesh_source_host::SourceReconciliationError::Store(cause) => CliError::Store(cause),
    })?;
    let generation = reconciliation.generation;
    let needs_index = reconciliation.published;
    let source_generation = if needs_index {
        let manifest = engine
            .current_generation(repository, worktree)?
            .ok_or_else(|| {
                CliError::Integrity("source publication lost its current generation".to_owned())
            })?;
        println!(
            "scan files={} yielded_entries={} elapsed_us={}",
            scan_metrics.files_read,
            scan_metrics.entries_visited,
            scan_metrics.elapsed.as_micros()
        );
        println!(
            "{} {} files into generation {} ({:?})",
            if syntax_policy == SourceSyntaxPolicy::Strict {
                "indexed"
            } else {
                "observed"
            },
            files.len(),
            manifest.generation.0.to_hex(),
            manifest.status
        );
        manifest.generation
    } else {
        if semantic_provider.is_none() {
            println!(
                "{} {} files into current generation {} ({})",
                if syntax_policy == SourceSyntaxPolicy::Strict {
                    "indexed"
                } else {
                    "observed"
                },
                files.len(),
                engine
                    .current_generation(repository, worktree)?
                    .map_or_else(
                        || generation.0.to_hex(),
                        |current_manifest| current_manifest.generation.0.to_hex()
                    ),
                if syntax_policy == SourceSyntaxPolicy::Strict {
                    "already up to date"
                } else {
                    "source inventory unchanged"
                }
            );
        }
        if semantic_provider.is_some() {
            engine
                .current_generation(repository, worktree)?
                .map(|manifest| manifest.generation)
                .ok_or_else(|| {
                    CliError::Integrity("semantic indexing lost its current generation".to_owned())
                })?
        } else {
            generation
        }
    };

    if syntax_policy == SourceSyntaxPolicy::RecordFailures {
        let coverage = engine.source_processing_coverage_at(source_generation)?;
        println!(
            "processing completed={} syntax_failed={} unclassified={}",
            coverage.completed.len(),
            coverage.syntax_failed.len(),
            coverage.unclassified.len()
        );
    }

    if let Some(provider) = semantic_provider {
        let result = enrich_and_publish_semantics(
            &mut engine,
            repository,
            worktree,
            source_generation,
            provider,
        );
        println!("{}", provider.usage_summary());
        result?;
    }
    Ok(())
}

fn enrich_and_publish_semantics<S, E>(
    engine: &mut SyntaxMeshEngine<S, E>,
    repository: RepositoryId,
    worktree: WorktreeId,
    source_generation: GenerationId,
    provider: &OpenAiCompatibleProvider,
) -> Result<(), CliError>
where
    S: GraphStore + DurableRecordStore,
    E: LanguageExtractor,
{
    let identity = provider.identity();
    let requests = engine.semantic_document_requests_for_generation(
        source_generation,
        &identity,
        SemanticBatchLimits {
            max_chunks: 32,
            max_bytes: 32 * 1024,
        },
    )?;
    let mut batch_count = requests.len();
    let semantic_batch = if requests.is_empty() {
        SemanticOutput::empty_fact_batch(&identity)
    } else {
        let (source_request, document_output) =
            engine.enrich_document_semantic_outputs_many(requests, provider)?;
        let output = if provider.cross_document_enabled() {
            let compound_requests = engine
                .semantic_requests_for_generation(
                    source_generation,
                    &identity,
                    SemanticBatchLimits {
                        max_chunks: 32,
                        max_bytes: 32 * 1024,
                    },
                )?
                .into_iter()
                .filter(|request| request.source_document_count() > 1)
                .collect::<Vec<_>>();
            batch_count = batch_count.saturating_add(compound_requests.len());
            if compound_requests.is_empty() {
                document_output
            } else {
                let (_, joint_output) =
                    engine.enrich_document_semantic_outputs_many(compound_requests, provider)?;
                SemanticOutput::merge([document_output, joint_output])
            }
        } else {
            document_output
        };
        source_request
            .into_fact_batch(output)
            .map_err(|error| CliError::SemanticProvider(format!("semantic composition: {error}")))?
    };
    let fact_bytes = serde_json::to_vec(&semantic_batch).map_err(CliError::Encode)?;
    let facts_hash = *blake3::hash(&fact_bytes).as_bytes();
    let current = engine
        .current_generation(repository, worktree)?
        .ok_or_else(|| {
            CliError::Integrity("semantic indexing lost its source generation".to_owned())
        })?;
    let run_id = IndexRunId::derive(&[
        b"syntaxmesh-cli-semantic-index-v1",
        &current.generation.0.0,
        &facts_hash,
    ]);
    let next_generation = GenerationId::derive(&[
        b"syntaxmesh-cli-semantic-generation-v1",
        &current.generation.0.0,
        &facts_hash,
    ]);
    let requests_made = provider.requests_made();
    let requests_cached = batch_count.saturating_sub(provider.semantic_requests_submitted());
    let claim_count = semantic_batch
        .nodes
        .iter()
        .filter(|node| {
            matches!(&node.kind,
                syntaxmesh_core::NodeKind::External { namespace, kind }
                    if namespace == syntaxmesh_semantic::SEMANTIC_NAMESPACE && kind == "claim"
            )
        })
        .count();
    let concept_count = semantic_batch
        .nodes
        .iter()
        .filter(|node| {
            matches!(&node.kind,
                syntaxmesh_core::NodeKind::External { namespace, kind }
                    if namespace == syntaxmesh_semantic::SEMANTIC_NAMESPACE && kind == "concept"
            )
        })
        .count();
    provider
        .verify_model_revision()
        .map_err(CliError::SemanticProvider)?;
    let publication = engine.replace_semantic_facts(semantic_batch, run_id, next_generation)?;
    if let Some(receipt) = publication {
        println!(
            "semantic batches={batch_count} provider_requests={requests_made} metadata_requests={} cache_reused={requests_cached} claims={} concepts={} published_generation={}",
            provider.metadata_requests_made(),
            claim_count,
            concept_count,
            receipt.publication.generation.generation.0.to_hex()
        );
    } else {
        println!(
            "semantic batches={batch_count} provider_requests={requests_made} metadata_requests={} cache_reused={requests_cached} claims={} concepts={} (already current)",
            provider.metadata_requests_made(),
            claim_count,
            concept_count,
        );
    }
    Ok(())
}

fn verification_policy(root: &Path, override_value: Option<bool>) -> Result<bool, CliError> {
    let canonical_root = std::fs::canonicalize(root)?;
    let config = project_config::ProjectConfig::load(&canonical_root)?;
    Ok(override_value.unwrap_or_else(|| config.verified_history()))
}
