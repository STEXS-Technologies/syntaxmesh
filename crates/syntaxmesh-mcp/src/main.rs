use std::error::Error;

mod options;

use rmcp::ServiceExt;
use rmcp::transport::stdio;
use syntaxmesh_mcp::SyntaxMeshMcp;

fn main() -> Result<(), Box<dyn Error>> {
    let selected = options::parse(std::env::args_os().skip(1))?;
    let mut server =
        SyntaxMeshMcp::open(selected.database, selected.source_root, &selected.tokenizer)?;
    if selected.source_content {
        server = server.with_source_content_context();
    }
    if selected.lexical_first {
        server = server.with_indexed_context_discovery_preference(
            syntaxmesh_query::DiscoveryPackingPreference::LexicalFirst,
        );
    }
    // Keep the Turso adapter alive until after the host runtime shuts down. The
    // adapter owns its own runtime, which must not be dropped inside Tokio.
    let shutdown_guard = server.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let service = server.serve(stdio()).await?;
        service.waiting().await?;
        Ok::<(), Box<dyn Error>>(())
    });
    drop(runtime);
    drop(shutdown_guard);
    result
}
