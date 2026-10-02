use syntaxmesh_lang_bash::BashExtractor;
use syntaxmesh_lang_docs::DocumentationExtractor;
use syntaxmesh_lang_ecmascript::{JavaScriptExtractor, TypeScriptExtractor};
use syntaxmesh_lang_python::PythonExtractor;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::{ExtractorRegistryError, SendCompositeExtractor};

#[cfg(test)]
mod tests;

/// Build the existing supported Rust-native source pack for CLI or shared hosts.
/// Hosts may extend the returned registry without changing Engine dependencies.
///
/// # Errors
/// Returns an error if the bundled extension registration is inconsistent.
pub fn supported_source_extractors() -> Result<SendCompositeExtractor, ExtractorRegistryError> {
    let mut extractors = SendCompositeExtractor::new();
    extractors.register(RustExtractor, ["rs"])?;
    extractors.register(PythonExtractor, ["py"])?;
    extractors.register(TypeScriptExtractor, ["ts", "tsx"])?;
    extractors.register(JavaScriptExtractor, ["js", "jsx", "mjs", "cjs"])?;
    extractors.register(BashExtractor, ["sh", "bash"])?;
    extractors.register(
        DocumentationExtractor,
        ["md", "markdown", "txt", "text", "rst", "adoc", "asciidoc"],
    )?;
    Ok(extractors)
}
