#[cfg(test)]
mod context_profile;
mod indexed_context;
mod planner_cache;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData as McpError};
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use syntaxmesh_api_model::ContextRequest;
use syntaxmesh_context_host::RepositorySourceReader;
use syntaxmesh_core::{GenerationId, NodeId, RepositoryId, StableId, WorktreeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_inference::{
    Claim, Contract, ContractLimits, ContractOrigin, ContractOutcome, DerivationClass,
    RelationExpectation, Rule,
};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_query::{ContextTokenCounter, Query};
use syntaxmesh_store_turso::TursoGraphStore;

#[derive(Debug)]
pub enum McpHostError {
    Store(String),
    NoGeneration,
}

impl std::fmt::Display for McpHostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(message) => formatter.write_str(message),
            Self::NoGeneration => formatter.write_str("database has no published generation"),
        }
    }
}

impl std::error::Error for McpHostError {}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchArgs {
    /// Case-insensitive substring in a node name.
    text: String,
    /// Maximum results to return (1–100, default 20).
    #[schemars(range(min = 1, max = 100))]
    limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NodeArgs {
    /// Stable node ID as 64 hexadecimal characters.
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NeighborsArgs {
    /// Stable node ID as 64 hexadecimal characters.
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PathArgs {
    /// Stable start node ID as 64 hexadecimal characters.
    start: String,
    /// Stable target node ID as 64 hexadecimal characters.
    target: String,
    /// Maximum directed path length in edges (1–32).
    #[schemars(range(min = 1, max = 32))]
    max_hops: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ContextArgs {
    /// Optional retained generation ID (64 hex characters); omit for the host's default graph.
    generation: Option<String>,
    /// Natural-language retrieval request.
    query: String,
    /// Optional stable graph-node IDs to use as explicit retrieval seeds.
    seed_nodes: Option<Vec<String>>,
    /// Maximum token count for the complete serialized context pack.
    #[schemars(range(min = 1))]
    token_budget: u64,
    /// Maximum graph-neighborhood depth (0–8, default 1).
    #[schemars(range(min = 0, max = 8))]
    max_hops: Option<u8>,
    /// Maximum graph candidates considered (1–256, default 64).
    #[schemars(range(min = 1, max = 256))]
    max_candidates: Option<u16>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ContractRelationArgs {
    /// Core relation name in snake_case, or `external`.
    kind: String,
    /// Required only when kind is `external`.
    #[schemars(length(max = 128))]
    namespace: Option<String>,
    /// Required only when kind is `external`.
    #[schemars(length(max = 128))]
    relation: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ContractExpectationArgs {
    Required,
    Forbidden,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ContractItemArgs {
    #[schemars(length(min = 1, max = 128))]
    id: String,
    #[schemars(length(min = 1, max = 64))]
    version: String,
    #[schemars(length(min = 64, max = 64))]
    source: String,
    #[schemars(length(min = 64, max = 64))]
    target: String,
    relation: ContractRelationArgs,
    expectation: ContractExpectationArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EvaluateContractsArgs {
    schema_version: u32,
    /// Optional retained generation; omitted means host-selected default.
    #[schemars(length(max = 64))]
    generation: Option<String>,
    #[schemars(length(max = 32))]
    contracts: Vec<ContractItemArgs>,
    /// Maximum retained fact-history entries scanned (1–100000, default 10000).
    #[schemars(range(min = 1, max = 100000))]
    history_scan_budget: Option<u64>,
    /// Maximum aggregate serialized evaluation input (1–4194304, default 1048576).
    #[schemars(range(min = 1, max = 4194304))]
    input_byte_budget: Option<u64>,
    /// Maximum contract-to-edge comparisons (1–1000000, default 100000).
    #[schemars(range(min = 1, max = 1000000))]
    edge_visit_budget: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InferenceCheckpointArgs {
    schema_version: u32,
    #[schemars(length(max = 128))]
    binding: String,
    #[schemars(length(max = 64))]
    generation: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ContractHistoryArgs {
    schema_version: u32,
    #[schemars(length(max = 128))]
    binding: String,
    #[schemars(length(min = 1, max = 128))]
    contract_id: String,
    #[schemars(length(min = 1, max = 64))]
    contract_version: String,
    /// Both cursor fields must be supplied together from the previous page.
    #[schemars(length(max = 64))]
    after_generation: Option<String>,
    after_sequence: Option<u64>,
    /// Maximum retained results (1–100, default 25).
    #[schemars(range(min = 1, max = 100))]
    limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InferenceCursorArgs {
    #[schemars(length(max = 128))]
    binding: String,
    #[schemars(length(max = 64))]
    stream: String,
    #[schemars(length(max = 64))]
    repository: String,
    #[schemars(length(max = 64))]
    worktree: String,
    #[schemars(length(max = 64))]
    generation: String,
    #[schemars(length(max = 64))]
    root: Option<String>,
    #[schemars(length(max = 64))]
    after: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InferencePageArgs {
    schema_version: u32,
    #[schemars(length(max = 128))]
    binding: String,
    #[schemars(length(max = 64))]
    generation: String,
    #[schemars(range(min = 1))]
    claims: u64,
    #[schemars(range(min = 1))]
    payload_bytes: u64,
    #[schemars(range(min = 1))]
    premises: u64,
    #[schemars(range(min = 1))]
    tree_pages: u64,
    cursor: Option<InferenceCursorArgs>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExplainInferenceClaimArgs {
    /// Serialized `InferenceClaim` copied from `inference_claims_page`.
    #[schemars(length(max = 131072))]
    claim_json: String,
    /// Serialized deterministic `Rule` from the operator's inference configuration.
    #[schemars(length(max = 131072))]
    rule_json: String,
}

const MAX_EXPLANATION_INPUT_BYTES: usize = 256 * 1024;
const MAX_EXPLANATION_PREMISES: usize = 256;
const MAX_EXPLANATION_DEPTH: usize = 64;
const MAX_EXPLANATION_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Clone)]
struct ExactTiktokenCounter {
    inner: syntaxmesh_context_host::ExactTiktokenCounter,
    #[cfg(test)]
    profile: Option<Arc<context_profile::Profile>>,
}

impl ExactTiktokenCounter {
    fn open(name: &str) -> Result<Self, McpHostError> {
        Ok(Self {
            inner: syntaxmesh_context_host::ExactTiktokenCounter::open(name)
                .map_err(McpHostError::Store)?,
            #[cfg(test)]
            profile: None,
        })
    }
}

impl ContextTokenCounter for ExactTiktokenCounter {
    fn tokenizer_id(&self) -> &str {
        self.inner.tokenizer_id()
    }

    fn count_tokens(&self, serialized_pack: &str) -> Result<u64, String> {
        #[cfg(test)]
        let started = self.profile.as_ref().map(|_| std::time::Instant::now());
        let result = self.inner.count_tokens(serialized_pack);
        #[cfg(test)]
        if let (Some(profile), Some(started)) = (&self.profile, started) {
            profile.record(started);
        }
        result
    }
}

/// Read-only MCP host, startup-pinned by default or live with a shared Engine.
pub struct SyntaxMeshMcp<E: LanguageExtractor = RustExtractor> {
    engine: Arc<Mutex<SyntaxMeshEngine<TursoGraphStore, E>>>,
    generation: GenerationId,
    repository: RepositoryId,
    worktree: WorktreeId,
    source_reader: RepositorySourceReader,
    token_counter: ExactTiktokenCounter,
    live: bool,
    indexed_context: bool,
    source_content_context: bool,
    source_role_preference: syntaxmesh_query::SourceRolePreference,
    discovery_preference: syntaxmesh_query::DiscoveryPackingPreference,
    planner_cache: Arc<Mutex<planner_cache::PlannerCache>>,
    #[cfg(feature = "inference-sqlite")]
    inference: Option<Arc<syntaxmesh_inference_host::InferenceReadBindings>>,
    #[cfg(feature = "inference-sqlite")]
    inference_admission: Arc<tokio::sync::Semaphore>,
}

impl SyntaxMeshMcp {
    /// Open an already-migrated Turso database without mutating it.
    ///
    /// # Errors
    /// Returns an error when the database is absent, invalid, or has no graph.
    pub fn open(
        database: impl AsRef<Path>,
        source_root: impl AsRef<Path>,
        tokenizer: &str,
    ) -> Result<Self, McpHostError> {
        let source_reader =
            RepositorySourceReader::open(source_root).map_err(McpHostError::Store)?;
        let token_counter = ExactTiktokenCounter::open(tokenizer)?;
        let store = TursoGraphStore::open(database)
            .map_err(|error| McpHostError::Store(error.to_string()))?;
        let manifest = store
            .latest_generation()
            .ok_or(McpHostError::NoGeneration)?;
        let generation = manifest.generation;
        let engine =
            SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            generation,
            repository: manifest.repository,
            worktree: manifest.worktree,
            source_reader,
            token_counter,
            live: false,
            indexed_context: false,
            source_content_context: false,
            source_role_preference: syntaxmesh_query::SourceRolePreference::Neutral,
            discovery_preference: syntaxmesh_query::DiscoveryPackingPreference::Balanced,
            planner_cache: Arc::new(Mutex::new(planner_cache::PlannerCache::new())),
            #[cfg(feature = "inference-sqlite")]
            inference: None,
            #[cfg(feature = "inference-sqlite")]
            inference_admission: Arc::new(tokio::sync::Semaphore::new(16)),
        })
    }
}

#[cfg(feature = "inference-sqlite")]
impl<E: LanguageExtractor + Send + 'static> SyntaxMeshMcp<E> {
    /// Install operator-configured retained inference reads after host creation.
    ///
    /// # Errors
    /// Rejects a registry outside this MCP host's repository/worktree scope.
    pub fn with_inference(
        mut self,
        bindings: syntaxmesh_inference_host::InferenceReadBindings,
    ) -> Result<Self, String> {
        if bindings.scope() != (self.repository, self.worktree) {
            return Err("inference registry outside MCP host scope".to_owned());
        }
        self.inference = Some(Arc::new(bindings));
        Ok(self)
    }
}

impl<E: LanguageExtractor> Clone for SyntaxMeshMcp<E> {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            generation: self.generation,
            repository: self.repository,
            worktree: self.worktree,
            source_reader: self.source_reader.clone(),
            token_counter: self.token_counter.clone(),
            live: self.live,
            indexed_context: self.indexed_context,
            source_content_context: self.source_content_context,
            source_role_preference: self.source_role_preference,
            discovery_preference: self.discovery_preference,
            planner_cache: Arc::clone(&self.planner_cache),
            #[cfg(feature = "inference-sqlite")]
            inference: self.inference.clone(),
            #[cfg(feature = "inference-sqlite")]
            inference_admission: Arc::clone(&self.inference_admission),
        }
    }
}

#[tool_router]
impl<E: LanguageExtractor + Send + 'static> SyntaxMeshMcp<E> {
    /// Query a trusted owner's existing scoped Engine without reopening its store.
    /// Keep the Engine alive outside the async runtime until shutdown. The caller
    /// owns writer coordination; reads serialize with publication on this mutex.
    ///
    /// # Errors
    /// Returns poisoned-lock, missing-scope, source-root, or tokenizer errors.
    pub fn from_shared_engine(
        engine: Arc<Mutex<SyntaxMeshEngine<TursoGraphStore, E>>>,
        repository: RepositoryId,
        worktree: WorktreeId,
        source_root: impl AsRef<Path>,
        tokenizer: &str,
    ) -> Result<Self, McpHostError> {
        let generation = engine
            .lock()
            .map_err(|error| McpHostError::Store(error.to_string()))?
            .current_generation(repository, worktree)
            .map_err(|error| McpHostError::Store(error.to_string()))?
            .ok_or(McpHostError::NoGeneration)?
            .generation;
        Ok(Self {
            engine,
            generation,
            repository,
            worktree,
            source_reader: RepositorySourceReader::open(source_root)
                .map_err(McpHostError::Store)?,
            token_counter: ExactTiktokenCounter::open(tokenizer)?,
            live: true,
            indexed_context: false,
            source_content_context: false,
            source_role_preference: syntaxmesh_query::SourceRolePreference::Neutral,
            discovery_preference: syntaxmesh_query::DiscoveryPackingPreference::Balanced,
            planner_cache: Arc::new(Mutex::new(planner_cache::PlannerCache::new())),
            #[cfg(feature = "inference-sqlite")]
            inference: None,
            #[cfg(feature = "inference-sqlite")]
            inference_admission: Arc::new(tokio::sync::Semaphore::new(16)),
        })
    }

    fn tool_error(message: impl Into<String>) -> McpError {
        McpError::new(rmcp::model::ErrorCode::INTERNAL_ERROR, message.into(), None)
    }

    fn input_error(message: impl Into<String>) -> McpError {
        McpError::new(rmcp::model::ErrorCode::INVALID_PARAMS, message.into(), None)
    }

    fn ensure_pinned_generation(
        engine: &SyntaxMeshEngine<TursoGraphStore, E>,
        repository: RepositoryId,
        worktree: WorktreeId,
        generation: GenerationId,
        live: bool,
    ) -> Result<GenerationId, String> {
        let current = engine
            .current_generation(repository, worktree)
            .map_err(|error| error.to_string())?;
        let current = current
            .ok_or_else(|| "scoped graph generation is no longer available".to_owned())?
            .generation;
        if !live && current != generation {
            return Err(format!(
                "pinned generation {} is no longer current; restart syntaxmesh-mcp to select the latest graph",
                generation.0.to_hex()
            ));
        }
        Ok(current)
    }

    fn result(value: &Value) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(&value)
            .map_err(|error| Self::tool_error(format!("encode query result: {error}")))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    fn parse_node_id(value: &str) -> Result<NodeId, McpError> {
        let bytes = hex::decode(value)
            .map_err(|error| Self::input_error(format!("invalid node ID: {error}")))?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|invalid_bytes: Vec<u8>| {
            Self::input_error(format!(
                "node ID must contain 32 bytes, got {}",
                invalid_bytes.len()
            ))
        })?;
        Ok(NodeId(StableId(bytes)))
    }

    fn parse_generation_id(value: &str) -> Result<GenerationId, McpError> {
        let bytes = hex::decode(value)
            .map_err(|error| Self::input_error(format!("invalid generation ID: {error}")))?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
            Self::input_error(format!(
                "generation ID must contain 32 bytes, got {}",
                bytes.len()
            ))
        })?;
        Ok(GenerationId(StableId(bytes)))
    }

    fn parse_contract_relation(
        args: ContractRelationArgs,
    ) -> Result<syntaxmesh_core::RelationKind, McpError> {
        use syntaxmesh_core::RelationKind;
        let no_external_fields = args.namespace.is_none() && args.relation.is_none();
        match args.kind.as_str() {
            "defines" if no_external_fields => Ok(RelationKind::Defines),
            "imports" if no_external_fields => Ok(RelationKind::Imports),
            "references" if no_external_fields => Ok(RelationKind::References),
            "calls" if no_external_fields => Ok(RelationKind::Calls),
            "implements" if no_external_fields => Ok(RelationKind::Implements),
            "resolves_to" if no_external_fields => Ok(RelationKind::ResolvesTo),
            "exports" if no_external_fields => Ok(RelationKind::Exports),
            "has_resolution_diagnostic" if no_external_fields => {
                Ok(RelationKind::HasResolutionDiagnostic)
            }
            "contains" if no_external_fields => Ok(RelationKind::Contains),
            "external" => {
                let namespace = args
                    .namespace
                    .ok_or_else(|| Self::input_error("external relation requires a namespace"))?;
                let relation = args.relation.ok_or_else(|| {
                    Self::input_error("external relation requires a relation name")
                })?;
                if namespace.trim().is_empty()
                    || relation.trim().is_empty()
                    || namespace.len() > 128
                    || relation.len() > 128
                    || namespace == "core"
                {
                    return Err(Self::input_error("invalid external relation identity"));
                }
                Ok(RelationKind::External {
                    namespace,
                    relation,
                })
            }
            _ => Err(Self::input_error(
                "unknown relation kind or unexpected relation fields",
            )),
        }
    }

    fn ensure_selected_scope(
        manifest: &syntaxmesh_core::GenerationManifest,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<(), String> {
        if manifest.repository != repository || manifest.worktree != worktree {
            return Err(
                "requested generation belongs to another repository or worktree".to_owned(),
            );
        }
        Ok(())
    }

    async fn with_query<T, F>(&self, operation: F) -> Result<T, McpError>
    where
        T: Send + 'static,
        F: for<'store> FnOnce(&Query<'store, TursoGraphStore>) -> Result<T, String>
            + Send
            + 'static,
    {
        self.with_selected_query(None, operation).await
    }

    async fn with_selected_query<T, F>(
        &self,
        selected: Option<GenerationId>,
        operation: F,
    ) -> Result<T, McpError>
    where
        T: Send + 'static,
        F: for<'store> FnOnce(&Query<'store, TursoGraphStore>) -> Result<T, String>
            + Send
            + 'static,
    {
        let engine = Arc::clone(&self.engine);
        let generation = self.generation;
        let repository = self.repository;
        let worktree = self.worktree;
        let live = self.live;
        tokio::task::spawn_blocking(move || {
            let engine = engine
                .lock()
                .map_err(|error| format!("query store lock is poisoned: {error}"))?;
            let current =
                Self::ensure_pinned_generation(&engine, repository, worktree, generation, live)?;
            let target = selected.unwrap_or(if live { current } else { generation });
            let query = engine.query(target);
            if selected.is_some() {
                let manifest = query.manifest().map_err(|error| error.to_string())?;
                Self::ensure_selected_scope(&manifest, repository, worktree)?;
            }
            let result = operation(&query)?;
            Self::ensure_pinned_generation(&engine, repository, worktree, generation, live)?;
            Ok::<T, String>(result)
        })
        .await
        .map_err(|error| Self::tool_error(format!("query worker failed: {error}")))?
        .map_err(Self::tool_error)
    }

    async fn with_engine<T, F>(&self, operation: F) -> Result<T, McpError>
    where
        T: Send + 'static,
        F: FnOnce(&SyntaxMeshEngine<TursoGraphStore, E>) -> Result<T, String> + Send + 'static,
    {
        let engine = Arc::clone(&self.engine);
        let generation = self.generation;
        let repository = self.repository;
        let worktree = self.worktree;
        let live = self.live;
        tokio::task::spawn_blocking(move || {
            let engine = engine
                .lock()
                .map_err(|error| format!("query store lock is poisoned: {error}"))?;
            Self::ensure_pinned_generation(&engine, repository, worktree, generation, live)?;
            let result = operation(&engine)?;
            Self::ensure_pinned_generation(&engine, repository, worktree, generation, live)?;
            Ok::<T, String>(result)
        })
        .await
        .map_err(|error| Self::tool_error(format!("query worker failed: {error}")))?
        .map_err(Self::tool_error)
    }

    #[tool(
        description = "Return the host's selected current generation and repository/worktree scope. Standalone hosts are startup-pinned; shared hosts refresh after publication."
    )]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        self.with_engine(move |engine| {
            let status = engine
                .status()
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "pinned graph generation is no longer available".to_owned())?;
            let manifest = status.manifest;
            Ok(json!({
                "generation": manifest.generation.0.to_hex(),
                "parent": manifest.parent.map(|parent_generation| parent_generation.0.to_hex()),
            "repository": manifest.repository.0.to_hex(),
            "worktree": manifest.worktree.0.to_hex(),
            "schema_version": manifest.schema_version,
                "status": format!("{:?}", manifest.status),
                "graph_root": hex::encode(manifest.graph_root),
                "files": status.files,
                "nodes": status.nodes,
                "edges": status.edges,
                "provenance": status.provenance,
                "integrity": status.integrity.is_valid(),
            }))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(description = "Search node names in the host's selected default graph generation.")]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let limit = args.limit.unwrap_or(20);
        let limit = usize::try_from(limit)
            .map_err(|error| Self::input_error(format!("invalid result limit: {error}")))?;
        if limit == 0 || limit > 100 {
            return Err(Self::input_error("result limit must be between 1 and 100"));
        }
        self.with_query(move |query| {
            let generation = query.generation();
            let nodes = query
                .search(&args.text, limit)
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "generation": generation.0.to_hex(),
                "nodes": nodes.into_iter().map(|node| json!({
                    "id": node.id.0.to_hex(),
                    "kind": format!("{:?}", node.kind),
                    "name": node.name,
                    "source": node.source,
                })).collect::<Vec<_>>(),
            }))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(
        description = "Read one node and source evidence by stable node ID from the selected default generation."
    )]
    async fn node(
        &self,
        Parameters(args): Parameters<NodeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = Self::parse_node_id(&args.id)?;
        self.with_query(move |query| {
            let generation = query.generation();
            let node = query.node(id).map_err(|error| error.to_string())?;
            Ok(json!({
                "generation": generation.0.to_hex(),
                "node": node.map(|node| json!({
                    "id": node.id.0.to_hex(),
                    "kind": format!("{:?}", node.kind),
                    "name": node.name,
                    "owner_file": node.owner_file.map(|file| file.0.to_hex()),
                    "source": node.source,
                    "provenance": node.provenance.0.to_hex(),
                })),
            }))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(description = "Read outgoing neighbors of a node in the selected default generation.")]
    async fn neighbors(
        &self,
        Parameters(args): Parameters<NeighborsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = Self::parse_node_id(&args.id)?;
        self.with_query(move |query| {
            let generation = query.generation();
            let neighbors = query.neighbors(id).map_err(|error| error.to_string())?;
            Ok(json!({
                "generation": generation.0.to_hex(),
                "neighbors": neighbors.into_iter().map(|(edge, node)| json!({
                    "edge_id": edge.id.0.to_hex(),
                    "relation": format!("{:?}", edge.relation),
                    "node_id": node.id.0.to_hex(),
                    "name": node.name,
                    "kind": format!("{:?}", node.kind),
                })).collect::<Vec<_>>(),
            }))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(
        description = "Find a bounded directed path between two node IDs in the selected default generation."
    )]
    async fn path(
        &self,
        Parameters(args): Parameters<PathArgs>,
    ) -> Result<CallToolResult, McpError> {
        let start = Self::parse_node_id(&args.start)?;
        let target = Self::parse_node_id(&args.target)?;
        let max_hops = usize::try_from(args.max_hops)
            .map_err(|error| Self::input_error(format!("invalid hop limit: {error}")))?;
        if max_hops == 0 || max_hops > 32 {
            return Err(Self::input_error("max_hops must be between 1 and 32"));
        }
        self.with_query(move |query| {
            let generation = query.generation();
            let path = query
                .path(start, target, max_hops)
                .map_err(|error| error.to_string())?;
            let nodes = path
                .map(|path| {
                    path.into_iter()
                        .map(|id| {
                            let node = query.node(id).map_err(|error| error.to_string())?;
                            node.ok_or_else(|| "path referenced a missing node".to_owned())
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()?;
            Ok(json!({
                "generation": generation.0.to_hex(),
                "path": nodes.map(|nodes| nodes.into_iter().map(|node| json!({
                    "id": node.id.0.to_hex(),
                    "name": node.name,
                    "kind": format!("{:?}", node.kind),
                })).collect::<Vec<_>>()),
            }))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(
        description = "Compile source and graph evidence under an exact tokenizer token budget; optionally select a retained generation. Changed historical source bytes are omitted with warnings."
    )]
    async fn context(
        &self,
        Parameters(args): Parameters<ContextArgs>,
    ) -> Result<CallToolResult, McpError> {
        let generation = args
            .generation
            .as_deref()
            .map(|value| {
                let bytes = hex::decode(value).map_err(|error| {
                    Self::input_error(format!("invalid generation ID: {error}"))
                })?;
                let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    Self::input_error(format!(
                        "generation ID must contain 32 bytes, got {}",
                        bytes.len()
                    ))
                })?;
                Ok::<_, McpError>(GenerationId(StableId(bytes)))
            })
            .transpose()?;
        let seed_nodes = args
            .seed_nodes
            .unwrap_or_default()
            .iter()
            .map(|id| Self::parse_node_id(id))
            .collect::<Result<Vec<_>, _>>()?;
        let max_hops = args.max_hops.unwrap_or(1);
        let max_candidates = args.max_candidates.unwrap_or(64);
        if args.token_budget == 0 {
            return Err(Self::input_error("token_budget must be positive"));
        }
        if max_hops > 8 {
            return Err(Self::input_error("max_hops must be between 0 and 8"));
        }
        if max_candidates == 0 || max_candidates > 256 {
            return Err(Self::input_error(
                "max_candidates must be between 1 and 256",
            ));
        }
        let request = ContextRequest {
            query: args.query,
            seed_nodes,
            token_budget: args.token_budget,
            max_hops,
            max_candidates,
        };
        let reader = self.source_reader.clone();
        let mut counter = self.token_counter.clone();
        counter.inner = counter.inner.for_request();
        let indexed = self.indexed_context && request.seed_nodes.is_empty();
        let cache = Arc::clone(&self.planner_cache);
        let preference = self.source_role_preference;
        let discovery = self.discovery_preference;
        let source_content = self.source_content_context;
        self.with_selected_query(generation, move |query| {
            let pack = if indexed {
                indexed_context::compile(
                    query,
                    &request,
                    generation.is_some(),
                    &reader,
                    &counter,
                    &cache,
                    indexed_context::Policy {
                        discovery,
                        preference,
                        source_content,
                    },
                )
            } else if generation.is_some() {
                query
                    .historical_context(&request, &reader, &counter)
                    .map_err(|error| error.to_string())
            } else {
                query
                    .context(&request, &reader, &counter)
                    .map_err(|error| error.to_string())
            }?;
            serde_json::to_value(pack).map_err(|error| format!("encode context pack: {error}"))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(
        description = "Evaluate up to 32 explicit required/forbidden relation contracts against a pinned current or retained graph generation. Missing relation evidence is reported as insufficient, never as proof."
    )]
    async fn evaluate_contracts(
        &self,
        Parameters(args): Parameters<EvaluateContractsArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.schema_version != 1 {
            return Err(Self::input_error(
                "unsupported contract evaluation schema_version",
            ));
        }
        if args.contracts.is_empty() || args.contracts.len() > 32 {
            return Err(Self::input_error(
                "contracts must contain between 1 and 32 items",
            ));
        }
        let selected = args
            .generation
            .as_deref()
            .map(Self::parse_generation_id)
            .transpose()?;
        let history_scan_budget = usize::try_from(args.history_scan_budget.unwrap_or(10_000))
            .map_err(|error| Self::input_error(format!("invalid history scan budget: {error}")))?;
        let input_byte_budget = usize::try_from(args.input_byte_budget.unwrap_or(1_048_576))
            .map_err(|error| Self::input_error(format!("invalid input byte budget: {error}")))?;
        let edge_visit_budget = usize::try_from(args.edge_visit_budget.unwrap_or(100_000))
            .map_err(|error| Self::input_error(format!("invalid edge visit budget: {error}")))?;
        if !(1..=100_000).contains(&history_scan_budget)
            || !(1..=4_194_304).contains(&input_byte_budget)
            || !(1..=1_000_000).contains(&edge_visit_budget)
        {
            return Err(Self::input_error(
                "contract evaluation budget exceeds host bounds",
            ));
        }
        let contracts = args
            .contracts
            .into_iter()
            .map(|item| {
                if item.id.trim().is_empty()
                    || item.id.len() > 128
                    || item.version.trim().is_empty()
                    || item.version.len() > 64
                    || item.source.len() != 64
                    || item.target.len() != 64
                    || item.relation.kind.len() > 32
                {
                    return Err(Self::input_error(
                        "invalid contract identity or node ID length",
                    ));
                }
                Ok(Contract {
                    id: item.id,
                    version: item.version,
                    origin: ContractOrigin::Declared,
                    source: Self::parse_node_id(&item.source)?,
                    target: Self::parse_node_id(&item.target)?,
                    relation: Self::parse_contract_relation(item.relation)?,
                    expectation: match item.expectation {
                        ContractExpectationArgs::Required => RelationExpectation::Required,
                        ContractExpectationArgs::Forbidden => RelationExpectation::Forbidden,
                    },
                })
            })
            .collect::<Result<Vec<_>, McpError>>()?;
        let limits = ContractLimits {
            contracts: 32,
            input_bytes: input_byte_budget,
            edge_visits: edge_visit_budget,
            results: 32,
        };
        self.with_selected_query(selected, move |query| {
            let generation = query.generation();
            let evaluations = query
                .evaluate_contracts(&contracts, &limits, history_scan_budget)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|evaluation| {
                    let evidence = evaluation.evidence.map(|version| {
                        let (fact_kind, fact_id) = match version.fact {
                            syntaxmesh_core::FactRef::Edge(id) => ("edge", id.0.to_hex()),
                            syntaxmesh_core::FactRef::File(id) => ("file", id.0.to_hex()),
                            syntaxmesh_core::FactRef::Node(id) => ("node", id.0.to_hex()),
                            syntaxmesh_core::FactRef::Provenance(id) => {
                                ("provenance", id.0.to_hex())
                            }
                        };
                        json!({
                            "fact_kind": fact_kind,
                            "fact_id": fact_id,
                            "valid_from": version.valid_from.0.to_hex(),
                        })
                    });
                    let outcome = match evaluation.outcome {
                        ContractOutcome::Supported => "supported",
                        ContractOutcome::Contradicted => "contradicted",
                        ContractOutcome::InsufficientEvidence => "insufficient_evidence",
                    };
                    json!({
                        "id": evaluation.id.to_hex(),
                        "contract_id": evaluation.contract_id,
                        "contract_version": evaluation.contract_version,
                        "origin": "declared",
                        "generation": evaluation.generation.0.to_hex(),
                        "outcome": outcome,
                        "evidence": evidence,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "schema_version": 1,
                "generation": generation.0.to_hex(),
                "evaluations": evaluations,
            }))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(
        description = "Read one explicitly selected retained inference checkpoint from an operator-configured binding."
    )]
    async fn inference_checkpoint(
        &self,
        Parameters(args): Parameters<InferenceCheckpointArgs>,
    ) -> Result<CallToolResult, McpError> {
        #[cfg(not(feature = "inference-sqlite"))]
        {
            let _ = (args.schema_version, args.binding, args.generation);
            return Err(Self::tool_error(
                "inference reads are not enabled in this build",
            ));
        }
        #[cfg(feature = "inference-sqlite")]
        {
            let bindings = self
                .inference
                .as_ref()
                .ok_or_else(|| Self::tool_error("inference reads are not configured"))?;
            let request = syntaxmesh_api_model::InferenceCheckpointRequest {
                schema_version: args.schema_version,
                binding: args.binding,
                generation: Self::parse_generation_id(&args.generation)?,
            };
            bindings.checkpoint(&request).map_err(Self::input_error)?;
            let permit = Arc::clone(&self.inference_admission)
                .try_acquire_owned()
                .map_err(|_error| Self::tool_error("inference read capacity is busy"))?;
            let bindings = Arc::clone(bindings);
            self.with_engine(move |engine| {
                let _permit = permit;
                let (database, origin, generation) = bindings.checkpoint(&request)?;
                let checkpoint = engine
                    .inference_checkpoint(database, origin, generation)
                    .map_err(|error| error.to_string())?
                    .map(syntaxmesh_inference_host::inference_checkpoint_record)
                    .transpose()?;
                serde_json::to_value(syntaxmesh_api_model::InferenceCheckpointResponse {
                    schema_version: syntaxmesh_api_model::INFERENCE_READ_SCHEMA_VERSION,
                    binding: request.binding,
                    generation,
                    checkpoint,
                })
                .map_err(|error| format!("encode inference checkpoint: {error}"))
            })
            .await
            .and_then(|value| Self::result(&value))
        }
    }

    #[tool(
        description = "Read a bounded page of exact retained inference claims from an operator-configured binding."
    )]
    async fn inference_claims_page(
        &self,
        Parameters(args): Parameters<InferencePageArgs>,
    ) -> Result<CallToolResult, McpError> {
        #[cfg(not(feature = "inference-sqlite"))]
        {
            let InferencePageArgs {
                schema_version,
                binding,
                generation,
                claims,
                payload_bytes,
                premises,
                tree_pages,
                cursor,
            } = args;
            let _ = (
                schema_version,
                binding,
                generation,
                claims,
                payload_bytes,
                premises,
                tree_pages,
            );
            if let Some(cursor) = cursor {
                let InferenceCursorArgs {
                    binding: cursor_binding,
                    stream,
                    repository,
                    worktree,
                    generation: cursor_generation,
                    root,
                    after,
                } = cursor;
                let _ = (
                    cursor_binding,
                    stream,
                    repository,
                    worktree,
                    cursor_generation,
                    root,
                    after,
                );
            }
            return Err(Self::tool_error(
                "inference reads are not enabled in this build",
            ));
        }
        #[cfg(feature = "inference-sqlite")]
        {
            let bindings = self
                .inference
                .as_ref()
                .ok_or_else(|| Self::tool_error("inference reads are not configured"))?;
            let cursor = args
                .cursor
                .map(|cursor| {
                    Ok(syntaxmesh_api_model::InferencePageCursor {
                        binding: cursor.binding,
                        stream: Self::parse_node_id(&cursor.stream)?.0,
                        selection: syntaxmesh_api_model::InferenceSelection {
                            repository: RepositoryId(Self::parse_node_id(&cursor.repository)?.0),
                            worktree: WorktreeId(Self::parse_node_id(&cursor.worktree)?.0),
                            generation: Self::parse_generation_id(&cursor.generation)?,
                        },
                        root: cursor
                            .root
                            .map(|root| Self::parse_node_id(&root).map(|node| node.0))
                            .transpose()?,
                        after: Self::parse_node_id(&cursor.after)?.0,
                    })
                })
                .transpose()?;
            let request = syntaxmesh_api_model::InferencePageRequest {
                schema_version: args.schema_version,
                binding: args.binding,
                generation: Self::parse_generation_id(&args.generation)?,
                limits: syntaxmesh_api_model::InferencePageLimits {
                    claims: args.claims,
                    payload_bytes: args.payload_bytes,
                    premises: args.premises,
                    tree_pages: args.tree_pages,
                },
                cursor,
            };
            bindings.page(&request).map_err(Self::input_error)?;
            let permit = Arc::clone(&self.inference_admission)
                .try_acquire_owned()
                .map_err(|_error| Self::tool_error("inference read capacity is busy"))?;
            let bindings = Arc::clone(bindings);
            self.with_engine(move |engine| {
                let _permit = permit;
                let admitted = bindings.page(&request)?;
                if engine
                    .inference_checkpoint(
                        admitted.database,
                        admitted.request.origin,
                        request.generation,
                    )
                    .map_err(|error| error.to_string())?
                    .is_none()
                {
                    return Err("inference checkpoint is not retained".to_owned());
                }
                let page = engine
                    .inference_claims_page(admitted.database, &admitted.request)
                    .map_err(|error| error.to_string())?;
                let response =
                    syntaxmesh_inference_host::inference_page_response(request.binding, page)?;
                serde_json::to_value(response)
                    .map_err(|error| format!("encode inference page: {error}"))
            })
            .await
            .and_then(|value| Self::result(&value))
        }
    }

    #[tool(
        description = "Verify one retained deterministic inference claim and return its bounded, exact premise and provenance evidence from the claim's graph generation. Supply the claim from inference_claims_page and its matching Rule JSON."
    )]
    async fn explain_inference_claim(
        &self,
        Parameters(args): Parameters<ExplainInferenceClaimArgs>,
    ) -> Result<CallToolResult, McpError> {
        let input_bytes = args
            .claim_json
            .len()
            .checked_add(args.rule_json.len())
            .ok_or_else(|| Self::tool_error("explanation input byte count overflow"))?;
        if input_bytes > MAX_EXPLANATION_INPUT_BYTES {
            return Err(Self::tool_error(
                "claim and rule JSON exceed the explanation input byte budget",
            ));
        }
        let claim: syntaxmesh_api_model::InferenceClaim = serde_json::from_str(&args.claim_json)
            .map_err(|error| Self::tool_error(format!("invalid inference claim JSON: {error}")))?;
        let rule: Rule = serde_json::from_str(&args.rule_json)
            .map_err(|error| Self::tool_error(format!("invalid inference rule JSON: {error}")))?;
        let claim = domain_claim(claim);
        self.with_engine(move |engine| {
            let explanation = engine
                .query(claim.generation)
                .explain_inference_claim(
                    &claim,
                    &rule,
                    MAX_EXPLANATION_PREMISES,
                    MAX_EXPLANATION_DEPTH,
                    MAX_EXPLANATION_OUTPUT_BYTES,
                )
                .map_err(|error| error.to_string())?;
            serde_json::to_value(explanation)
                .map_err(|error| format!("encode verified inference explanation: {error}"))
        })
        .await
        .and_then(|value| Self::result(&value))
    }

    #[tool(
        description = "Read a bounded page of already-retained contract evaluations from an operator-configured inference binding. This tool never evaluates or materializes contracts."
    )]
    async fn contract_history(
        &self,
        Parameters(args): Parameters<ContractHistoryArgs>,
    ) -> Result<CallToolResult, McpError> {
        #[cfg(not(feature = "inference-sqlite"))]
        {
            let ContractHistoryArgs {
                schema_version,
                binding,
                contract_id,
                contract_version,
                after_generation,
                after_sequence,
                limit,
            } = args;
            let _ = (
                schema_version,
                binding,
                contract_id,
                contract_version,
                after_generation,
                after_sequence,
                limit,
            );
            return Err(Self::tool_error(
                "retained contract history is not enabled in this build",
            ));
        }
        #[cfg(feature = "inference-sqlite")]
        {
            if args.schema_version != syntaxmesh_api_model::INFERENCE_READ_SCHEMA_VERSION {
                return Err(Self::input_error(
                    "unsupported contract history schema_version",
                ));
            }
            let after = match (args.after_generation, args.after_sequence) {
                (None, None) => None,
                (Some(generation), Some(generation_sequence)) => {
                    let (repository, worktree) = self
                        .inference
                        .as_ref()
                        .ok_or_else(|| Self::tool_error("inference reads are not configured"))?
                        .scope();
                    Some(syntaxmesh_inference_host::ContractHistoryCursor {
                        repository,
                        worktree,
                        contract_id: args.contract_id.clone(),
                        contract_version: args.contract_version.clone(),
                        generation_sequence,
                        generation: Self::parse_generation_id(&generation)?,
                    })
                }
                _ => {
                    return Err(Self::input_error(
                        "after_generation and after_sequence must be supplied together",
                    ));
                }
            };
            let bindings = self
                .inference
                .as_ref()
                .ok_or_else(|| Self::tool_error("inference reads are not configured"))?;
            let admitted = bindings
                .contract_history(
                    args.schema_version,
                    &args.binding,
                    args.contract_id,
                    args.contract_version,
                    after,
                    usize::try_from(args.limit.unwrap_or(25)).map_err(|error| {
                        Self::input_error(format!("invalid page limit: {error}"))
                    })?,
                )
                .map_err(Self::input_error)?;
            let permit = Arc::clone(&self.inference_admission)
                .try_acquire_owned()
                .map_err(|_error| Self::tool_error("inference read capacity is busy"))?;
            let database = admitted.database.to_path_buf();
            let query = admitted.query;
            let binding = args.binding;
            self.with_engine(move |engine| {
                let _permit = permit;
                let page = engine
                    .contract_history(database, &query)
                    .map_err(|error| error.to_string())?;
                let items = page
                    .items
                    .into_iter()
                    .map(|item| {
                        let contract = item.contract;
                        let evaluation = item.evaluation;
                        let origin = match contract.origin {
                            ContractOrigin::SourceDerived => "source_derived",
                            ContractOrigin::SchemaDerived => "schema_derived",
                            ContractOrigin::ConfigDerived => "config_derived",
                            ContractOrigin::Declared => "declared",
                            ContractOrigin::Documented => "documented",
                            ContractOrigin::RuntimeInferred => "runtime_inferred",
                            ContractOrigin::ModelSuggested => "model_suggested",
                        };
                        let expectation = match contract.expectation {
                            RelationExpectation::Required => "required",
                            RelationExpectation::Forbidden => "forbidden",
                        };
                        let outcome = match evaluation.outcome {
                            ContractOutcome::Supported => "supported",
                            ContractOutcome::Contradicted => "contradicted",
                            ContractOutcome::InsufficientEvidence => "insufficient_evidence",
                        };
                        let evidence = evaluation.evidence.map(|version| {
                            let (fact_kind, fact_id) = match version.fact {
                                syntaxmesh_core::FactRef::Edge(id) => ("edge", id.0.to_hex()),
                                syntaxmesh_core::FactRef::File(id) => ("file", id.0.to_hex()),
                                syntaxmesh_core::FactRef::Node(id) => ("node", id.0.to_hex()),
                                syntaxmesh_core::FactRef::Provenance(id) => {
                                    ("provenance", id.0.to_hex())
                                }
                            };
                            json!({
                                "fact_kind": fact_kind,
                                "fact_id": fact_id,
                                "valid_from": version.valid_from.0.to_hex(),
                            })
                        });
                        json!({
                            "contract": {
                                "id": contract.id,
                                "version": contract.version,
                                "origin": origin,
                                "source": contract.source.0.to_hex(),
                                "target": contract.target.0.to_hex(),
                                "relation": contract.relation,
                                "expectation": expectation,
                            },
                            "evaluation": {
                                "id": evaluation.id.to_hex(),
                                "contract_id": evaluation.contract_id,
                                "contract_version": evaluation.contract_version,
                                "origin": origin,
                                "generation": evaluation.generation.0.to_hex(),
                                "outcome": outcome,
                                "evidence": evidence,
                            },
                            "generation_sequence": item.generation_sequence,
                        })
                    })
                    .collect::<Vec<_>>();
                let next_cursor = page.next_cursor.map(|cursor| {
                    json!({
                        "generation_sequence": cursor.generation_sequence,
                        "generation": cursor.generation.0.to_hex(),
                    })
                });
                Ok(json!({
                    "schema_version": 1,
                    "binding": binding,
                    "items": items,
                    "next_cursor": next_cursor,
                }))
            })
            .await
            .and_then(|value| Self::result(&value))
        }
    }
}

fn domain_claim(claim: syntaxmesh_api_model::InferenceClaim) -> Claim {
    Claim {
        id: claim.id,
        derivation_class: match claim.derivation_class {
            syntaxmesh_api_model::InferenceDerivationClass::DeterministicRule => {
                DerivationClass::DeterministicRule
            }
            syntaxmesh_api_model::InferenceDerivationClass::HeuristicRule => {
                DerivationClass::HeuristicRule
            }
            syntaxmesh_api_model::InferenceDerivationClass::ModelInference => {
                DerivationClass::ModelInference
            }
            syntaxmesh_api_model::InferenceDerivationClass::UserDeclared => {
                DerivationClass::UserDeclared
            }
            syntaxmesh_api_model::InferenceDerivationClass::RuntimeCorrelation => {
                DerivationClass::RuntimeCorrelation
            }
        },
        repository: claim.repository,
        worktree: claim.worktree,
        generation: claim.generation,
        source: claim.source,
        target: claim.target,
        relation: claim.relation,
        rule_id: claim.rule_id,
        rule_version: claim.rule_version,
        ontology_versions: claim.ontology_versions,
        configuration_fingerprint: claim.configuration_fingerprint,
        premises: claim.premises,
    }
}

#[cfg(test)]
mod tests;

#[rmcp::tool_handler(router = Self::tool_router())]
impl<E: LanguageExtractor + Send + 'static> rmcp::ServerHandler for SyntaxMeshMcp<E> {}
