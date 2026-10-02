use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use syntaxmesh_context_host::{ExactTiktokenCounter, RepositorySourceReader};
use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_query::{Query, QueryError};
use syntaxmesh_store::StoreError;
use syntaxmesh_store_turso::TursoGraphStore;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

#[cfg(test)]
mod tests;

/// Read-only host, startup-pinned by default or live with a shared Engine.
/// Keep a clone or shared Engine reference outside the host runtime
/// until shutdown so the Turso adapter's runtime is dropped synchronously.
pub struct SyntaxMeshHttp<E: LanguageExtractor = RustExtractor> {
    engine: Arc<Mutex<SyntaxMeshEngine<TursoGraphStore, E>>>,
    pub(crate) generation: GenerationId,
    repository: RepositoryId,
    worktree: WorktreeId,
    admission: Arc<Semaphore>,
    pub(crate) context: Option<(RepositorySourceReader, ExactTiktokenCounter)>,
    live: bool,
    pub(crate) owner_instance: Option<axum::http::HeaderValue>,
}

impl SyntaxMeshHttp {
    /// Open an existing migrated database with a published generation.
    ///
    /// # Errors
    /// Fails if the database is absent, invalid, or has no published graph.
    pub fn open(database: impl AsRef<Path>) -> Result<Self, String> {
        let store = TursoGraphStore::open(database).map_err(|error| error.to_string())?;
        let manifest = store
            .latest_generation()
            .ok_or("database has no published generation")?;
        let engine =
            SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            generation: manifest.generation,
            repository: manifest.repository,
            worktree: manifest.worktree,
            admission: Arc::new(Semaphore::new(16)),
            context: None,
            live: false,
            owner_instance: None,
        })
    }
}

impl<E: LanguageExtractor + Send + 'static> SyntaxMeshHttp<E> {
    /// Serve live reads from a trusted host's shared Engine and scoped graph.
    /// The owner must coordinate writers and retain the Engine outside the async
    /// runtime until shutdown. Reads and publications serialize on the same lock.
    ///
    /// # Errors
    /// Fails for a poisoned lock, invalid scope, or absent published generation.
    pub fn from_shared_engine(
        engine: Arc<Mutex<SyntaxMeshEngine<TursoGraphStore, E>>>,
        repository: RepositoryId,
        worktree: WorktreeId,
    ) -> Result<Self, String> {
        let generation = engine
            .lock()
            .map_err(|error| error.to_string())?
            .current_generation(repository, worktree)
            .map_err(|error| error.to_string())?
            .ok_or("shared engine has no published generation for the requested scope")?
            .generation;
        Ok(Self {
            engine,
            generation,
            repository,
            worktree,
            admission: Arc::new(Semaphore::new(16)),
            context: None,
            live: true,
            owner_instance: None,
        })
    }

    /// Enable source-verified context retrieval with an explicit tokenizer.
    ///
    /// Configure an owner instance separately with `with_owner_instance` when
    /// this host is served through daemon discovery.
    ///
    /// # Errors
    /// Rejects invalid repository roots or unsupported/unavailable tokenizers.
    pub fn with_context(mut self, root: impl AsRef<Path>, tokenizer: &str) -> Result<Self, String> {
        self.context = Some((
            RepositorySourceReader::open(root)?,
            ExactTiktokenCounter::open(tokenizer)?,
        ));
        Ok(self)
    }

    /// Bind guarded HTTP requests to a host instance (not an authentication secret).
    /// Unguarded requests remain compatible with ordinary HTTP/MCP clients.
    ///
    /// # Errors
    /// Rejects identifiers other than 64 lowercase hexadecimal characters.
    pub fn with_owner_instance(mut self, instance: &str) -> Result<Self, String> {
        if instance.len() != 64
            || !instance
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("invalid owner instance identifier".to_owned());
        }
        self.owner_instance =
            Some(axum::http::HeaderValue::from_str(instance).map_err(|error| error.to_string())?);
        Ok(self)
    }

    fn ensure_pin(&self, engine: &SyntaxMeshEngine<TursoGraphStore, E>) -> Result<(), StatusCode> {
        let current = engine
            .current_generation(self.repository, self.worktree)
            .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
        if current.is_none()
            || (!self.live && current.map(|manifest| manifest.generation) != Some(self.generation))
        {
            return Err(StatusCode::CONFLICT);
        }
        Ok(())
    }

    pub(crate) async fn query<T, F>(
        &self,
        selected: Option<GenerationId>,
        operation: F,
    ) -> Result<T, StatusCode>
    where
        T: Send + 'static,
        F: for<'store> FnOnce(&Query<'store, TursoGraphStore>) -> Result<T, StatusCode>
            + Send
            + 'static,
    {
        self.read_engine(move |host, engine| {
            let default = if host.live {
                engine
                    .current_generation(host.repository, host.worktree)
                    .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
                    .ok_or(StatusCode::CONFLICT)?
                    .generation
            } else {
                host.generation
            };
            let query = engine.query(selected.unwrap_or(default));
            if selected.is_some() {
                let manifest = query.manifest().map_err(|error| {
                    if matches!(error, QueryError::Store(StoreError::StaleBase { .. })) {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                })?;
                if manifest.repository != host.repository || manifest.worktree != host.worktree {
                    return Err(StatusCode::NOT_FOUND);
                }
            }
            operation(&query)
        })
        .await
    }

    pub(crate) async fn labeled_query<T, F>(
        &self,
        selected: Option<GenerationId>,
        operation: F,
    ) -> Result<(GenerationId, T), StatusCode>
    where
        T: Send + 'static,
        F: for<'store> FnOnce(&Query<'store, TursoGraphStore>) -> Result<T, StatusCode>
            + Send
            + 'static,
    {
        self.query(selected, move |query| {
            operation(query).map(|data| (query.generation(), data))
        })
        .await
    }

    pub(crate) async fn readiness(
        &self,
    ) -> Result<(GenerationId, syntaxmesh_engine::EngineWorkflowDiagnostics), StatusCode> {
        self.read_engine(|host, engine| {
            let generation = engine
                .current_generation(host.repository, host.worktree)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::CONFLICT)?
                .generation;
            engine
                .workflow_diagnostics()
                .map(|diagnostics| (generation, diagnostics))
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await
    }

    pub(crate) async fn backend_integrity(
        &self,
    ) -> Result<(GenerationId, syntaxmesh_store::BackendIntegrityReport), StatusCode> {
        self.read_engine(|host, engine| {
            let generation = engine
                .current_generation(host.repository, host.worktree)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::CONFLICT)?
                .generation;
            let report = engine
                .backend_integrity_check()
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok((generation, report))
        })
        .await
    }

    pub(crate) async fn workflow_rejections(
        &self,
        after: Option<syntaxmesh_core::IndexRunId>,
        limit: usize,
    ) -> Result<(GenerationId, syntaxmesh_engine::WorkflowRejectionPage), StatusCode> {
        self.read_engine(move |host, engine| {
            let generation = engine
                .current_generation(host.repository, host.worktree)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::CONFLICT)?
                .generation;
            let page = engine
                .workflow_rejections(after, limit)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok((generation, page))
        })
        .await
    }

    pub(crate) async fn read_engine<T, F>(&self, operation: F) -> Result<T, StatusCode>
    where
        T: Send + 'static,
        F: FnOnce(&Self, &SyntaxMeshEngine<TursoGraphStore, E>) -> Result<T, StatusCode>
            + Send
            + 'static,
    {
        let permit = Arc::clone(&self.admission)
            .try_acquire_owned()
            .map_err(|_error| StatusCode::SERVICE_UNAVAILABLE)?;
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let engine = host
                .engine
                .lock()
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            host.ensure_pin(&engine)?;
            let result = operation(&host, &engine)?;
            host.ensure_pin(&engine)?;
            Ok(result)
        })
        .await
        .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
    }

    /// Serve a loopback listener until the injected shutdown signal resolves.
    /// Shutdown closes query admission for every clone of this host instance;
    /// accepted blocking workers are not cancelled. Reopen to serve again.
    ///
    /// # Errors
    /// Rejects non-loopback listeners and propagates listener/server IO failures.
    pub async fn serve_until<F>(
        &self,
        listener: TcpListener,
        shutdown: F,
    ) -> Result<(), std::io::Error>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.serve_with_routes(listener, shutdown, axum::Router::new())
            .await
    }

    /// Compose host-owned routes under the same loopback request boundary.
    /// Additional routes must not conflict with built-in route paths. Shutdown
    /// closes query admission; accepted workers are not cancelled.
    ///
    /// # Errors
    /// Rejects non-loopback listeners and propagates listener/server IO failures.
    ///
    /// # Panics
    /// Axum rejects route conflicts when additional routes overlap built-in paths.
    pub async fn serve_with_routes<F>(
        &self,
        listener: TcpListener,
        shutdown: F,
        additional: axum::Router,
    ) -> Result<(), std::io::Error>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let address: SocketAddr = listener.local_addr()?;
        if !address.ip().is_loopback() {
            return Err(std::io::Error::other(
                "HTTP host requires a loopback listener",
            ));
        }
        let router = crate::routes::router(self.clone(), address.to_string(), additional);
        let admission = Arc::clone(&self.admission);
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                shutdown.await;
                admission.close();
            })
            .await
    }
}
impl<E: LanguageExtractor> Clone for SyntaxMeshHttp<E> {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            generation: self.generation,
            repository: self.repository,
            worktree: self.worktree,
            admission: Arc::clone(&self.admission),
            context: self.context.clone(),
            live: self.live,
            owner_instance: self.owner_instance.clone(),
        }
    }
}
