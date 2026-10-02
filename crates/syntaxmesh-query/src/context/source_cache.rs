use std::collections::BTreeMap;
use std::sync::Arc;

use syntaxmesh_core::{FileId, FileVersion};

use super::ContextSourceProvider;

const MAX_CACHED_BYTES: usize = 8 * 1024 * 1024;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct SourceFailure {
    pub(super) code: &'static str,
    pub(super) message: String,
}

pub(super) struct SourceCache<'provider, P> {
    provider: &'provider P,
    entries: BTreeMap<(FileId, [u8; 32]), Result<Arc<str>, SourceFailure>>,
    retained_bytes: usize,
}

impl<'provider, P: ContextSourceProvider> SourceCache<'provider, P> {
    pub(super) const fn new(provider: &'provider P) -> Self {
        Self {
            provider,
            entries: BTreeMap::new(),
            retained_bytes: 0,
        }
    }

    pub(super) fn read(&mut self, file: &FileVersion) -> Result<Arc<str>, SourceFailure> {
        let key = (file.file_id, file.content_hash);
        if let Some(cached) = self.entries.get(&key) {
            return cached.clone();
        }
        let result = self.load(file);
        let bytes = result.as_ref().map_or(0, |text| text.len());
        if bytes <= MAX_CACHED_BYTES.saturating_sub(self.retained_bytes) {
            self.retained_bytes = self.retained_bytes.saturating_add(bytes);
            self.entries.insert(key, result.clone());
        }
        result
    }

    fn load(&self, file: &FileVersion) -> Result<Arc<str>, SourceFailure> {
        let failure = |code, message: &str| SourceFailure {
            code,
            message: message.to_owned(),
        };
        let bytes = self
            .provider
            .read_source(file)
            .map_err(|message| SourceFailure {
                code: "source_provider_error",
                message,
            })?
            .ok_or_else(|| {
                failure(
                    "source_unavailable",
                    "source provider returned no file content",
                )
            })?;
        if blake3::hash(&bytes).as_bytes() != &file.content_hash {
            return Err(failure(
                "stale_source",
                "current source hash differs from the pinned graph generation",
            ));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_error| failure("invalid_utf8_source", "source bytes are not UTF-8"))?;
        Ok(Arc::from(text))
    }
}
