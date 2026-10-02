use syntaxmesh_engine::EngineError;
use syntaxmesh_lang_ecmascript::ModuleResolutionError;
use syntaxmesh_query::QueryError;
use syntaxmesh_scanner::ScanError;
use syntaxmesh_store::StoreError;
use syntaxmesh_store_turso::TursoStoreError;

use super::project_config::ProjectConfigError;

#[derive(Debug)]
pub(super) enum CliError {
    Usage(String),
    Io(std::io::Error),
    Engine(EngineError),
    Query(QueryError),
    Store(StoreError),
    Turso(TursoStoreError),
    Encode(serde_json::Error),
    Scan(ScanError),
    Integrity(String),
    IndexLag(String),
    ProjectConfig(ProjectConfigError),
    ModuleResolution(ModuleResolutionError),
    PythonModuleResolution(String),
    SemanticProvider(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => write!(formatter, "usage: {message}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Engine(error) => write!(formatter, "engine error: {error}"),
            Self::Query(error) => write!(formatter, "query error: {error}"),
            Self::Store(error) => write!(formatter, "store error: {error}"),
            Self::Turso(error) => write!(formatter, "Turso error: {error}"),
            Self::Encode(error) => write!(formatter, "encoding error: {error}"),
            Self::Scan(error) => write!(formatter, "scan error: {error}"),
            Self::Integrity(message) => write!(formatter, "integrity check failed: {message}"),
            Self::IndexLag(message) => write!(formatter, "indexed source is stale: {message}"),
            Self::ProjectConfig(error) => write!(formatter, "project configuration error: {error}"),
            Self::ModuleResolution(error) => {
                write!(formatter, "module resolver setup failed: {error}")
            }
            Self::PythonModuleResolution(error) => {
                write!(formatter, "Python module resolver setup failed: {error}")
            }
            Self::SemanticProvider(error) => write!(formatter, "semantic provider failed: {error}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<syntaxmesh_ownership_host::OwnershipError> for CliError {
    fn from(error: syntaxmesh_ownership_host::OwnershipError) -> Self {
        match error {
            syntaxmesh_ownership_host::OwnershipError::Io(cause) => Self::Io(cause),
            other @ (syntaxmesh_ownership_host::OwnershipError::InvalidTarget(_)
            | syntaxmesh_ownership_host::OwnershipError::AlreadyOwned) => {
                Self::Usage(other.to_string())
            }
        }
    }
}

impl From<ProjectConfigError> for CliError {
    fn from(error: ProjectConfigError) -> Self {
        Self::ProjectConfig(error)
    }
}

impl From<EngineError> for CliError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}

impl From<QueryError> for CliError {
    fn from(error: QueryError) -> Self {
        Self::Query(error)
    }
}

impl From<StoreError> for CliError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<TursoStoreError> for CliError {
    fn from(error: TursoStoreError) -> Self {
        Self::Turso(error)
    }
}

impl From<ScanError> for CliError {
    fn from(error: ScanError) -> Self {
        Self::Scan(error)
    }
}

impl From<ModuleResolutionError> for CliError {
    fn from(error: ModuleResolutionError) -> Self {
        Self::ModuleResolution(error)
    }
}
