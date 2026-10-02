use std::io::Read;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Deserialize;

use super::{MAX_RESPONSE_BYTES, OpenAiCompatibleProvider};
use crate::cli::CliError;

#[cfg(test)]
mod tests;

impl OpenAiCompatibleProvider {
    pub(in crate::cli) fn resolve_model_revision(
        &mut self,
        explicit: Option<&str>,
    ) -> Result<(), CliError> {
        let revision = if let Some(revision) = explicit {
            if revision.trim().is_empty() || revision.len() > 512 {
                return Err(CliError::Usage(
                    "semantic model revision must contain 1–512 bytes".to_owned(),
                ));
            }
            format!("asserted:{revision}")
        } else {
            self.ollama_model_revision()
                .map_err(CliError::SemanticProvider)?
        };
        self.identity.model_revision = revision;
        self.discovered_revision = explicit.is_none();
        Ok(())
    }

    pub(in crate::cli) fn verify_model_revision(&self) -> Result<(), String> {
        if self.discovered_revision && self.ollama_model_revision()? != self.identity.model_revision
        {
            return Err("semantic model revision changed during indexing; retry with the current model revision".to_owned());
        }
        Ok(())
    }

    fn ollama_model_revision(&self) -> Result<String, String> {
        if self.offline {
            return Err(
                "offline semantic execution requires an explicit model revision".to_owned(),
            );
        }
        let loopback = super::endpoint::is_loopback(&self.endpoint);
        let prefix = self.endpoint.path().strip_suffix("/v1");
        if !loopback || prefix.is_none() {
            return Err(
                "this provider requires --semantic-model-revision <immutable-revision>".to_owned(),
            );
        }
        let mut endpoint = self.endpoint.clone();
        endpoint.set_path(&format!("{}/api/tags", prefix.unwrap_or_default()));
        let mut request = self.client.get(endpoint).timeout(Duration::from_secs(5));
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        self.metadata_request_count.fetch_add(1, Ordering::Relaxed);
        let mut response = request.send().map_err(|_error| {
            "could not discover semantic model revision; run the local Ollama service or supply --semantic-model-revision".to_owned()
        })?;
        if !response.status().is_success() {
            return Err(format!(
                "model revision discovery returned HTTP {}; compatible providers without Ollama metadata require --semantic-model-revision",
                response.status()
            ));
        }
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(u64::try_from(MAX_RESPONSE_BYTES.saturating_add(1)).unwrap_or(u64::MAX))
            .read_to_end(&mut bytes)
            .map_err(|_error| "could not read semantic model metadata".to_owned())?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err("semantic model metadata exceeded the 4 MiB limit".to_owned());
        }
        let metadata: ModelList = serde_json::from_slice(&bytes)
            .map_err(|_error| "semantic model metadata is malformed".to_owned())?;
        let model = tagged_model(&self.model);
        let mut matches = metadata
            .models
            .iter()
            .filter(|entry| tagged_model(&entry.name) == model);
        let entry = matches.next().ok_or_else(|| {
            "selected semantic model is not installed; install it in Ollama or select an installed model".to_owned()
        })?;
        if matches.next().is_some() {
            return Err("semantic model metadata contains duplicate model names".to_owned());
        }
        let digest = entry
            .digest
            .strip_prefix("sha256:")
            .unwrap_or(&entry.digest);
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("semantic model digest must be a lowercase SHA-256 value".to_owned());
        }
        Ok(format!("ollama:sha256:{digest}"))
    }
}

fn tagged_model(model: &str) -> String {
    if model.rsplit('/').next().unwrap_or(model).contains(':') {
        model.to_owned()
    } else {
        format!("{model}:latest")
    }
}

#[derive(Deserialize)]
struct ModelList {
    models: Vec<ModelMetadata>,
}

#[derive(Deserialize)]
struct ModelMetadata {
    name: String,
    digest: String,
}
