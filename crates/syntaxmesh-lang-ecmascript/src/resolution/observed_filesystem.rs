use oxc_resolver::{FileMetadata, FileSystem, ResolveError};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(super) type ObservedPaths = Arc<Mutex<BTreeSet<PathBuf>>>;

pub(super) struct ObservedFileSystem<Fs> {
    pub(super) inner: Fs,
    pub(super) paths: ObservedPaths,
}

impl<Fs> ObservedFileSystem<Fs> {
    fn record(&self, path: &Path) -> io::Result<()> {
        self.paths
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?
            .insert(path.to_owned());
        Ok(())
    }
}

impl<Fs: FileSystem> FileSystem for ObservedFileSystem<Fs> {
    fn new() -> Self {
        Self {
            inner: Fs::new(),
            paths: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.record(path)?;
        self.inner.read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.record(path)?;
        self.inner.read_to_string(path)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.record(path)?;
        self.inner.metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.record(path)?;
        self.inner.symlink_metadata(path)
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        self.record(path).map_err(ResolveError::from)?;
        self.inner.read_link(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.record(path)?;
        self.inner.canonicalize(path)
    }
}
