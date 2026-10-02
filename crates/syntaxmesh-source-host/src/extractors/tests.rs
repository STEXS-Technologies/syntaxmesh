use super::*;
use syntaxmesh_language_sdk::{CompositeExtractor, LanguageExtractor};

#[test]
fn source_pack_matches_previous_cli_identity_and_moves_to_host_thread()
-> Result<(), Box<dyn std::error::Error>> {
    let mut previous = CompositeExtractor::new();
    previous.register(RustExtractor, ["rs"])?;
    previous.register(PythonExtractor, ["py"])?;
    previous.register(TypeScriptExtractor, ["ts", "tsx"])?;
    previous.register(JavaScriptExtractor, ["js", "jsx", "mjs", "cjs"])?;
    previous.register(BashExtractor, ["sh", "bash"])?;
    previous.register(
        DocumentationExtractor,
        ["md", "markdown", "txt", "text", "rst", "adoc", "asciidoc"],
    )?;
    let pack = supported_source_extractors()?;
    let expected = previous.configuration_fingerprint();
    let transferred = std::thread::spawn(move || pack.configuration_fingerprint())
        .join()
        .map_err(|_panic| "source-pack thread panicked")?;
    if transferred != expected {
        return Err("source pack changed existing cache identity".into());
    }
    Ok(())
}
