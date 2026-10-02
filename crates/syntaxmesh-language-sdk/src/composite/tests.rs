use syntaxmesh_core::{FileId, FileVersion};

use super::*;

#[test]
fn send_composition_reuses_local_identity_and_moves_to_host_thread()
-> Result<(), Box<dyn std::error::Error>> {
    let mut local = CompositeExtractor::new();
    let mut transferable = SendCompositeExtractor::new();
    local.register(NamedExtractor("rust", "1"), ["rs"])?;
    transferable.register(NamedExtractor("rust", "1"), ["rs"])?;
    if local.configuration_fingerprint() != transferable.configuration_fingerprint() {
        return Err("thread capability changed extraction cache identity".into());
    }
    if transferable
        .register(NamedExtractor("python", "1"), ["py", "RS"])
        .is_ok()
        || transferable.extract(&source("app.py")) != local.extract(&source("app.py"))
    {
        return Err("Send registry partially registered conflicting extensions".into());
    }
    let expected = local.extract(&source("src/LIB.RS"));
    let moved = std::thread::spawn(move || transferable.extract(&source("src/LIB.RS")))
        .join()
        .map_err(|_panic| "extractor thread panicked")?;
    if moved != expected {
        return Err("thread transfer changed extractor dispatch".into());
    }
    Ok(())
}

#[test]
fn local_composition_preserves_non_send_extractors() -> Result<(), ExtractorRegistryError> {
    struct LocalExtractor(std::rc::Rc<String>);
    impl LanguageExtractor for LocalExtractor {
        fn language(&self) -> &'static str {
            "local"
        }
        fn producer_identity(&self) -> ExtractorIdentity {
            ExtractorIdentity::new("local", "1")
        }
        fn extract(&self, _source: &SourceFile) -> Result<Extraction, ExtractorError> {
            Err(ExtractorError::InvalidInput(self.0.as_ref().clone()))
        }
    }
    let mut local = CompositeExtractor::new();
    local.register(
        LocalExtractor(std::rc::Rc::new("local-state".to_owned())),
        ["local"],
    )?;
    if local.extract(&source("input.local"))
        != Err(ExtractorError::InvalidInput("local-state".to_owned()))
    {
        return Err(ExtractorRegistryError::EmptyExtension);
    }
    Ok(())
}

struct NamedExtractor(&'static str, &'static str);

impl LanguageExtractor for NamedExtractor {
    fn language(&self) -> &'static str {
        self.0
    }

    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new(format!("test.{}", self.0), self.1)
    }

    fn extract(&self, _source: &SourceFile) -> Result<Extraction, ExtractorError> {
        Err(ExtractorError::InvalidInput(self.0.to_owned()))
    }
}

fn source(path: &str) -> SourceFile {
    SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash: [0; 32],
            size_bytes: 0,
        },
        content: String::new(),
    }
}

#[test]
fn routes_by_case_insensitive_extension() {
    let mut registry = CompositeExtractor::new();
    assert_eq!(
        registry.register(NamedExtractor("python", "1"), [".py"]),
        Ok(())
    );
    assert_eq!(
        registry.register(NamedExtractor("rust", "1"), ["rs"]),
        Ok(())
    );

    assert_eq!(
        registry.extract(&source("src/MODULE.PY")),
        Err(ExtractorError::InvalidInput("python".to_owned()))
    );
    assert_eq!(
        registry.extract(&source("src/lib.rs")),
        Err(ExtractorError::InvalidInput("rust".to_owned()))
    );
}

#[test]
fn duplicate_registration_is_rejected_without_partial_changes() {
    let mut registry = CompositeExtractor::new();
    assert_eq!(
        registry.register(NamedExtractor("python", "1"), ["py"]),
        Ok(())
    );

    assert_eq!(
        registry.register(NamedExtractor("typescript", "1"), ["ts", "PY"]),
        Err(ExtractorRegistryError::DuplicateExtension("py".to_owned()))
    );
    assert_eq!(
        registry.extract(&source("app.ts")),
        Err(ExtractorError::UnsupportedFile("app.ts".to_owned()))
    );
    assert_eq!(
        registry.extract(&source("app.py")),
        Err(ExtractorError::InvalidInput("python".to_owned()))
    );
}

#[test]
fn empty_and_extensionless_paths_are_rejected() {
    let mut registry = CompositeExtractor::new();
    assert_eq!(
        registry.register(NamedExtractor("empty", "1"), ["."]),
        Err(ExtractorRegistryError::EmptyExtension)
    );
    assert_eq!(
        registry.extract(&source("README")),
        Err(ExtractorError::UnsupportedFile("README".to_owned()))
    );
}

#[test]
fn configuration_fingerprint_is_order_independent_and_tracks_versions() {
    let mut first = CompositeExtractor::new();
    assert_eq!(
        first.register(NamedExtractor("python", "1"), ["py"]),
        Ok(())
    );
    assert_eq!(first.register(NamedExtractor("rust", "1"), ["rs"]), Ok(()));

    let mut same_set_different_order = CompositeExtractor::new();
    assert_eq!(
        same_set_different_order.register(NamedExtractor("rust", "1"), ["rs"]),
        Ok(())
    );
    assert_eq!(
        same_set_different_order.register(NamedExtractor("python", "1"), ["py"]),
        Ok(())
    );
    assert_eq!(
        first.configuration_fingerprint(),
        same_set_different_order.configuration_fingerprint()
    );

    let mut upgraded = CompositeExtractor::new();
    assert_eq!(
        upgraded.register(NamedExtractor("python", "2"), ["py"]),
        Ok(())
    );
    assert_eq!(
        upgraded.register(NamedExtractor("rust", "1"), ["rs"]),
        Ok(())
    );
    assert_ne!(
        first.configuration_fingerprint(),
        upgraded.configuration_fingerprint()
    );
}
