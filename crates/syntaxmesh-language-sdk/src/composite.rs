//! Extension-based composition of host-independent language extractors.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syntaxmesh_core::StableId;

use crate::extractor::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, SourceFile,
};

/// A configuration error while registering a language extractor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractorRegistryError {
    /// An extension was empty after trimming an optional leading dot.
    EmptyExtension,
    /// Another registered extractor already owns this normalized extension.
    DuplicateExtension(String),
}

impl std::fmt::Display for ExtractorRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ExtractorRegistryError {}

/// An immutable-after-setup router that composes language extractors by file
/// extension. Registration is normally completed before constructing an
/// engine; `extract` only reads the registry.
pub struct ExtractorRegistry<T: LanguageExtractor + ?Sized> {
    extractors: Vec<RegisteredExtractor<T>>,
}

/// Local composition, including extractors that cannot move across threads.
pub type CompositeExtractor = ExtractorRegistry<dyn LanguageExtractor>;
/// Composition of extractors that can move across host threads.
pub type SendCompositeExtractor = ExtractorRegistry<dyn LanguageExtractor + Send>;

impl<T: LanguageExtractor + ?Sized> Default for ExtractorRegistry<T> {
    fn default() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }
}

struct RegisteredExtractor<T: LanguageExtractor + ?Sized> {
    extensions: BTreeSet<String>,
    extractor: Box<T>,
}

impl<T: LanguageExtractor + ?Sized> ExtractorRegistry<T> {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn register_boxed<I, X>(
        &mut self,
        extractor: Box<T>,
        extensions: I,
    ) -> Result<(), ExtractorRegistryError>
    where
        I: IntoIterator<Item = X>,
        X: AsRef<str>,
    {
        let mut normalized = Vec::new();
        let mut unique = BTreeSet::new();
        for extension in extensions {
            let extension = extension
                .as_ref()
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if extension.is_empty() {
                return Err(ExtractorRegistryError::EmptyExtension);
            }
            let already_registered = self
                .extractors
                .iter()
                .any(|registration| registration.extensions.contains(&extension));
            if already_registered || !unique.insert(extension.clone()) {
                return Err(ExtractorRegistryError::DuplicateExtension(extension));
            }
            normalized.push(extension);
        }
        if normalized.is_empty() {
            return Err(ExtractorRegistryError::EmptyExtension);
        }

        self.extractors.push(RegisteredExtractor {
            extensions: normalized.into_iter().collect(),
            extractor,
        });
        Ok(())
    }

    fn extractor_for(&self, source: &SourceFile) -> Option<&T> {
        let extension = Path::new(&source.file.normalized_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)?;
        self.extractors
            .iter()
            .find(|registration| registration.extensions.contains(&extension))
            .map(|registration| registration.extractor.as_ref())
    }
}

impl<T: LanguageExtractor + ?Sized> LanguageExtractor for ExtractorRegistry<T> {
    fn language(&self) -> &'static str {
        "composite"
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new("syntaxmesh.lang.composite", "1")
    }

    fn producer_identity_for(
        &self,
        source: &SourceFile,
    ) -> Result<ExtractorIdentity, ExtractorError> {
        self.extractor_for(source)
            .map(LanguageExtractor::producer_identity)
            .ok_or_else(|| ExtractorError::UnsupportedFile(source.file.normalized_path.clone()))
    }

    fn configuration_fingerprint(&self) -> [u8; 32] {
        let mut parts = vec![b"composite-language-set-v1".to_vec()];
        let mut mappings = BTreeMap::new();
        for registration in &self.extractors {
            let identity = registration.extractor.producer_identity();
            for extension in &registration.extensions {
                mappings.insert(extension, identity.clone());
            }
        }
        for (extension, identity) in mappings {
            parts.push(extension.as_bytes().to_vec());
            parts.push(identity.namespace.into_bytes());
            parts.push(identity.version.into_bytes());
        }
        let refs = parts.iter().map(Vec::as_slice).collect::<Vec<_>>();
        StableId::derive("composite-language-set-v1", &refs).0
    }

    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        let extractor = self
            .extractor_for(source)
            .ok_or_else(|| ExtractorError::UnsupportedFile(source.file.normalized_path.clone()))?;
        extractor.extract(source)
    }
}

impl ExtractorRegistry<dyn LanguageExtractor> {
    /// Registers one extractor for one or more extensions, with or without a
    /// leading dot. Extensions are normalized to lowercase. The registry is
    /// unchanged if any extension is empty or already registered.
    ///
    /// # Errors
    /// Returns [`ExtractorRegistryError`] when registration is ambiguous or
    /// contains an invalid empty extension.
    pub fn register<E, I, X>(
        &mut self,
        extractor: E,
        extensions: I,
    ) -> Result<(), ExtractorRegistryError>
    where
        E: LanguageExtractor + 'static,
        I: IntoIterator<Item = X>,
        X: AsRef<str>,
    {
        self.register_boxed(Box::new(extractor), extensions)
    }
}

impl ExtractorRegistry<dyn LanguageExtractor + Send> {
    /// Registers one extractor for one or more extensions, with or without a
    /// leading dot. Extensions are normalized to lowercase. The registry is
    /// unchanged if any extension is empty or already registered.
    ///
    /// # Errors
    /// Returns [`ExtractorRegistryError`] when registration is ambiguous or
    /// contains an invalid empty extension.
    pub fn register<E, I, X>(
        &mut self,
        extractor: E,
        extensions: I,
    ) -> Result<(), ExtractorRegistryError>
    where
        E: LanguageExtractor + Send + 'static,
        I: IntoIterator<Item = X>,
        X: AsRef<str>,
    {
        self.register_boxed(Box::new(extractor), extensions)
    }
}

#[cfg(test)]
mod tests;
