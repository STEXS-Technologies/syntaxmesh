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
        })
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
}

#[cfg(test)]
mod tests;

#[rmcp::tool_handler(router = Self::tool_router())]
impl<E: LanguageExtractor + Send + 'static> rmcp::ServerHandler for SyntaxMeshMcp<E> {}
