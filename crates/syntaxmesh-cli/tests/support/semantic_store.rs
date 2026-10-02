use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use syntaxmesh_core::{GenerationManifest, GenerationStatus};
use syntaxmesh_store::{FileGraphStore, GraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

#[derive(Clone, Copy)]
pub(super) enum StoreHost {
    File,
    TursoVerified,
}

impl StoreHost {
    pub(super) fn path(self, root: &Path) -> PathBuf {
        root.join(match self {
            Self::File => ".syntaxmesh/index.snapshot",
            Self::TursoVerified => ".syntaxmesh/index.db",
        })
    }

    pub(super) fn prepare(self, root: &Path) -> Result<(), Box<dyn Error>> {
        if matches!(self, Self::TursoVerified) {
            std::fs::create_dir_all(root.join(".syntaxmesh"))?;
            TursoGraphStore::migrate(self.path(root))?;
        }
        Ok(())
    }

    pub(super) fn open(self, root: &Path) -> Result<Box<dyn GraphStore>, Box<dyn Error>> {
        match self {
            Self::File => Ok(Box::new(FileGraphStore::open(self.path(root))?)),
            Self::TursoVerified => Ok(Box::new(TursoGraphStore::open(self.path(root))?)),
        }
    }

    pub(super) fn run(
        self,
        root: &Path,
        endpoint: Option<&str>,
        options: &[&str],
    ) -> std::io::Result<Output> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
        command.current_dir(root);
        match self {
            Self::File => {
                command.arg("index");
            }
            Self::TursoVerified => {
                command
                    .arg("index-turso")
                    .arg(root)
                    .arg(self.path(root))
                    .arg("--verify");
            }
        }
        if let Some(endpoint) = endpoint {
            command.args(["--semantic", "--semantic-endpoint", endpoint]);
        }
        command.args(options).output()
    }

    pub(super) fn latest(
        self,
        store: &dyn GraphStore,
    ) -> Result<GenerationManifest, Box<dyn Error>> {
        let history = store.generation_history()?;
        let latest = history
            .last()
            .ok_or_else(|| std::io::Error::other("missing semantic generation"))?;
        let manifest = store.manifest(latest.manifest.generation)?;
        let expected = match self {
            Self::File => GenerationStatus::Durable,
            Self::TursoVerified => GenerationStatus::Verified,
        };
        if manifest.status != expected {
            return Err(
                std::io::Error::other("semantic generation verification status differs").into(),
            );
        }
        Ok(manifest)
    }
}
