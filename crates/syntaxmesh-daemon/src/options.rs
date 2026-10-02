use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use syntaxmesh_watch_host::WatchLoopOptions;

pub(super) const USAGE: &str = "usage: syntaxmeshd <source-root> <migrated-turso-database> <loopback-address:port> [--verify] [--poll-only] [--reconcile-ms <positive-ms>] [--watch-duration-ms <positive-ms>] [--context-tokenizer <cl100k_base|o200k_base>] [--mcp]";

pub(super) struct Options {
    pub root: PathBuf,
    pub database: PathBuf,
    pub address: SocketAddr,
    pub verify: bool,
    pub watch: WatchLoopOptions,
    pub tokenizer: Option<String>,
    pub mcp: bool,
}

pub(super) fn parse(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<Option<Options>, String> {
    let Some(root) = arguments.next() else {
        return Ok(None);
    };
    if root == "--help" || root == "-h" {
        return Ok(None);
    }
    let database = arguments.next().ok_or_else(|| USAGE.to_owned())?;
    let address = arguments.next().ok_or_else(|| USAGE.to_owned())?;
    let address: SocketAddr = address
        .to_str()
        .ok_or("address must be UTF-8")?
        .parse()
        .map_err(|error| format!("invalid address: {error}"))?;
    if !address.ip().is_loopback() {
        return Err("daemon requires a literal loopback address".to_owned());
    }
    let mut options = Options {
        root: root.into(),
        database: database.into(),
        address,
        verify: false,
        watch: WatchLoopOptions::default(),
        tokenizer: None,
        mcp: false,
    };
    while let Some(argument) = arguments.next() {
        if argument == "--verify" && !options.verify {
            options.verify = true;
        } else if argument == "--mcp" && !options.mcp {
            options.mcp = true;
        } else if argument == "--poll-only" && !options.watch.poll_only {
            options.watch.poll_only = true;
        } else if argument == "--reconcile-ms" || argument == "--watch-duration-ms" {
            let value = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .ok_or("watch timing requires positive milliseconds")?;
            if argument == "--reconcile-ms" {
                options.watch.reconcile = Duration::from_millis(value);
            } else {
                options.watch.duration = Some(Duration::from_millis(value));
            }
        } else if argument == "--context-tokenizer" && options.tokenizer.is_none() {
            let value = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or("missing context tokenizer")?;
            if value != "cl100k_base" && value != "o200k_base" {
                return Err("unsupported context tokenizer".to_owned());
            }
            options.tokenizer = Some(value);
        } else {
            return Err(format!(
                "unknown or duplicate daemon option: {}",
                argument.to_string_lossy()
            ));
        }
    }
    if options.mcp && options.tokenizer.is_none() {
        return Err("--mcp requires --context-tokenizer".to_owned());
    }
    Ok(Some(options))
}
