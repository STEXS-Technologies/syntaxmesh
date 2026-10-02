//! Transport selection for the existing opt-in evaluation workload.

use std::error::Error;
use std::path::Path;

pub(super) struct EvaluationConfig {
    pub(super) model: String,
    pub(super) transport: &'static str,
    pub(super) cross_document: bool,
    pub(super) arguments: Vec<String>,
}

impl EvaluationConfig {
    pub(super) fn from_environment() -> Result<Self, Box<dyn Error>> {
        let endpoint = optional_env("SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT")?;
        let mut command = optional_env("SYNTAXMESH_SEMANTIC_EVAL_COMMAND")?;
        let model = optional_env("SYNTAXMESH_SEMANTIC_EVAL_MODEL")?;
        let revision = optional_env("SYNTAXMESH_SEMANTIC_EVAL_REVISION")?;
        let cross_document = boolean_env(
            "SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT",
            "cross-document mode",
        )?;
        let allow_network =
            boolean_env("SYNTAXMESH_SEMANTIC_EVAL_ALLOW_NETWORK", "network opt-in")?;
        if endpoint.is_none()
            && !allow_network
            && let (Some(argument), Some(directory)) = (
                command.as_mut(),
                optional_env("SYNTAXMESH_SEMANTIC_EVAL_WORKING_DIRECTORY")?,
            )
            && let Some(path) = argument.strip_prefix('@')
        {
            let resolved = Path::new(&directory).join(path);
            let encoded = resolved
                .to_str()
                .ok_or_else(|| std::io::Error::other("command file path is not UTF-8"))?;
            *argument = format!("@{encoded}");
        }
        Self::new(
            endpoint,
            command,
            model,
            revision,
            cross_document,
            allow_network,
        )
    }

    pub(super) fn new(
        endpoint: Option<String>,
        command: Option<String>,
        model: Option<String>,
        revision: Option<String>,
        cross_document: bool,
        allow_network: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let (transport, option, value, default_model) = match (endpoint, command) {
            (Some(endpoint), None) => ("http", "--semantic-endpoint", endpoint, "qwen3:latest"),
            (None, Some(command)) if !allow_network => {
                let resolved = if let Some(path) = command.strip_prefix('@') {
                    format!("@{}", Path::new(path).canonicalize()?.display())
                } else {
                    command
                };
                ("local-command", "--semantic-command", resolved, "gpt-6-luna")
            }
            (None, Some(_)) => return Err(std::io::Error::other("command evaluation uses harness-owned network policy, not evaluator network opt-in").into()),
            _ => return Err(std::io::Error::other("set exactly one of SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT or SYNTAXMESH_SEMANTIC_EVAL_COMMAND explicitly; no provider is started automatically").into()),
        };
        let model = model.unwrap_or_else(|| default_model.to_owned());
        if value.trim().is_empty()
            || model.trim().is_empty()
            || revision
                .as_ref()
                .is_some_and(|asserted_revision| asserted_revision.trim().is_empty())
        {
            return Err(std::io::Error::other(
                "evaluator transport, model, and explicit revision must be non-empty",
            )
            .into());
        }
        let mut arguments = vec![
            "index".to_owned(),
            "--semantic".to_owned(),
            model.clone(),
            option.to_owned(),
            value,
        ];
        if cross_document {
            arguments.push("--semantic-cross-document".to_owned());
        }
        if let Some(revision) = revision {
            arguments.extend(["--semantic-model-revision".to_owned(), revision]);
        }
        if allow_network {
            arguments.push("--allow-network".to_owned());
        }
        Ok(Self {
            model,
            transport,
            cross_document,
            arguments,
        })
    }
}

fn optional_env(name: &str) -> Result<Option<String>, Box<dyn Error>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn boolean_env(name: &str, label: &str) -> Result<bool, Box<dyn Error>> {
    match optional_env(name)?.as_deref() {
        Some("true") => Ok(true),
        Some("false") | None => Ok(false),
        _ => Err(std::io::Error::other(format!("evaluator {label} must be true or false")).into()),
    }
}
