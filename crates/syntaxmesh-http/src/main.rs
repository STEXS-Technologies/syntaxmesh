use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;

use syntaxmesh_http::SyntaxMeshHttp;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let database = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: syntaxmesh-http <migrated-indexed-database> <loopback-address:port> [--context <source-root> <cl100k_base|o200k_base>]")?;
    let address = args.next().ok_or("missing loopback address")?;
    let address: SocketAddr = address.to_str().ok_or("address must be UTF-8")?.parse()?;
    let context = match args.next() {
        None => None,
        Some(flag) if flag == "--context" => {
            let root = args
                .next()
                .map(PathBuf::from)
                .ok_or("missing context source root")?;
            let tokenizer = args.next().ok_or("missing context tokenizer")?;
            Some((
                root,
                tokenizer
                    .into_string()
                    .map_err(|_value| "tokenizer must be UTF-8")?,
            ))
        }
        Some(_) => return Err("expected --context <source-root> <cl100k_base|o200k_base>".into()),
    };
    if args.next().is_some() || !address.ip().is_loopback() {
        return Err("expected a database and literal loopback address".into());
    }
    let host = SyntaxMeshHttp::open(database)?;
    let host = if let Some((root, tokenizer)) = context {
        host.with_context(root, &tokenizer)?
    } else {
        host
    };
    // Keep the adapter's runtime outside the host runtime, as in the MCP host.
    let guard = host.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address).await?;
        eprintln!("SyntaxMesh HTTP listening on {}", listener.local_addr()?);
        host.serve_until(listener, async {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("HTTP shutdown signal failed: {error}");
            }
        })
        .await
    });
    drop(runtime);
    drop(guard);
    result.map_err(Into::into)
}
