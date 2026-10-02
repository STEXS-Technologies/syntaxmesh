use std::ffi::OsString;
use std::path::PathBuf;

pub(super) const USAGE: &str = "usage: syntaxmesh-mcp <migrated-turso-database> <source-root> <cl100k_base|o200k_base> [--source-content-context] [--lexical-first-context]";

pub(super) struct Options {
    pub(super) database: PathBuf,
    pub(super) source_root: PathBuf,
    pub(super) tokenizer: String,
    pub(super) source_content: bool,
    pub(super) lexical_first: bool,
}

pub(super) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Options, String> {
    let mut arguments = arguments.into_iter();
    let database = arguments.next().map(PathBuf::from).ok_or(USAGE)?;
    let source_root = arguments.next().map(PathBuf::from).ok_or(USAGE)?;
    let tokenizer = arguments
        .next()
        .ok_or(USAGE)?
        .into_string()
        .map_err(|_invalid_utf8| "tokenizer name must be valid UTF-8".to_owned())?;
    if !matches!(tokenizer.as_str(), "cl100k_base" | "o200k_base") {
        return Err("tokenizer must be cl100k_base or o200k_base".to_owned());
    }
    let mut source_content = false;
    let mut lexical_first = false;
    for argument in arguments {
        if argument == "--source-content-context" && !source_content {
            source_content = true;
        } else if argument == "--lexical-first-context" && !lexical_first {
            lexical_first = true;
        } else {
            return Err(USAGE.to_owned());
        }
    }
    Ok(Options {
        database,
        source_root,
        tokenizer,
        source_content,
        lexical_first,
    })
}

#[cfg(test)]
mod tests;
