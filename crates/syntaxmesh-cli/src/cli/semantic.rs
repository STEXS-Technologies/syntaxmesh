use std::collections::BTreeSet;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use serde_json::json;
use syntaxmesh_semantic::{
    SemanticClaim, SemanticEvidence, SemanticOutput, SemanticProvider, SemanticProviderIdentity,
    SemanticRequest,
};

use super::CliError;

mod adaptive;
mod command;
mod endpoint;
mod policy;
mod prompt;
mod retry;
mod revision;
#[cfg(test)]
mod tests;
mod usage;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434/v1";
const MAX_PARALLEL_REQUESTS: usize = 4;
const MAX_DOCUMENTS_PER_PROMPT: usize = 22;
const MAX_CHUNKS_PER_PROMPT: usize = 32;
const MAX_PROMPT_BYTES: usize = 48 * 1024;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// CLI semantic host with HTTP or explicit local-command transport.
pub(super) struct OpenAiCompatibleProvider {
    client: Client,
    endpoint: Url,
    model: String,
    api_key: Option<String>,
    identity: SemanticProviderIdentity,
    request_count: AtomicUsize,
    semantic_request_count: AtomicUsize,
    metadata_request_count: AtomicUsize,
    discovered_revision: bool,
    offline: bool,
    cross_document: bool,
    parallel_limit: usize,
    usage: usage::UsageCounters,
    command: Option<Vec<String>>,
}

impl OpenAiCompatibleProvider {
    pub(super) fn command_argv(argument: &str) -> Result<Vec<String>, CliError> {
        let json = if let Some(path) = argument.strip_prefix('@') {
            let file = std::fs::File::open(path)?;
            let mut bytes = Vec::new();
            file.take(65537).read_to_end(&mut bytes)?;
            if bytes.len() > 65536 {
                return Err(CliError::Usage(
                    "semantic command argv file exceeds 64 KiB".to_owned(),
                ));
            }
            String::from_utf8(bytes).map_err(|_error| {
                CliError::Usage("semantic command argv file must be UTF-8".to_owned())
            })?
        } else {
            argument.to_owned()
        };
        let argv: Vec<String> = serde_json::from_str(&json).map_err(|_error| {
            CliError::Usage(
                "--semantic-command requires a JSON argv array or @path to an argv JSON file"
                    .to_owned(),
            )
        })?;
        command::validate(&argv).map_err(CliError::Usage)?;
        Ok(argv)
    }

    pub(super) fn new(
        endpoint: Option<&str>,
        model: &str,
        api_key: Option<String>,
        allow_network: bool,
    ) -> Result<Self, CliError> {
        let endpoint = endpoint.unwrap_or(DEFAULT_ENDPOINT);
        let mut url = Url::parse(endpoint).map_err(|_error| {
            CliError::SemanticProvider("semantic endpoint is not a valid URL".to_owned())
        })?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(CliError::SemanticProvider(
                "semantic endpoint must be an HTTP(S) base URL without credentials, query, or fragment"
                    .to_owned(),
            ));
        }
        if model.trim().is_empty() {
            return Err(CliError::SemanticProvider(
                "semantic model must not be empty".to_owned(),
            ));
        }
        let loopback = endpoint::is_loopback(&url);
        if !loopback && !allow_network {
            return Err(CliError::SemanticProvider(
                "non-loopback semantic endpoints require --allow-network".to_owned(),
            ));
        }
        if !loopback && url.scheme() != "https" {
            return Err(CliError::SemanticProvider(
                "remote semantic endpoints must use HTTPS".to_owned(),
            ));
        }
        if url.path().ends_with('/') {
            let path = url.path().trim_end_matches('/').to_owned();
            url.set_path(&path);
        }
        let api_key = api_key.filter(|key| !key.trim().is_empty());
        if api_key
            .as_deref()
            .is_some_and(|key| key.chars().any(char::is_whitespace))
        {
            return Err(CliError::SemanticProvider(
                "semantic API key must not contain whitespace".to_owned(),
            ));
        }
        let mut config_hasher = blake3::Hasher::new();
        config_hasher.update(url.as_str().as_bytes());
        config_hasher.update(model.as_bytes());
        if let Some(key) = &api_key {
            config_hasher.update(blake3::hash(key.as_bytes()).as_bytes());
        }
        let configuration_hash = *config_hasher.finalize().as_bytes();
        let identity = SemanticProviderIdentity {
            provider: "openai-compatible".to_owned(),
            model: model.to_owned(),
            model_revision: model.to_owned(),
            prompt_version: policy::VERSION.to_owned(),
            prompt_hash: *blake3::hash(policy::SYSTEM.as_bytes()).as_bytes(),
            configuration_hash,
        };
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());
        let client = if loopback { client.no_proxy() } else { client };
        let client = client.build().map_err(|_error| {
            CliError::SemanticProvider("could not initialize the HTTP client".to_owned())
        })?;
        let parallel_limit = if loopback && url.port_or_known_default() == Some(11434) {
            1
        } else {
            MAX_PARALLEL_REQUESTS
        };
        Ok(Self {
            client,
            endpoint: url,
            model: model.to_owned(),
            api_key,
            identity,
            request_count: AtomicUsize::new(0),
            semantic_request_count: AtomicUsize::new(0),
            metadata_request_count: AtomicUsize::new(0),
            discovered_revision: false,
            offline: false,
            cross_document: false,
            parallel_limit,
            usage: usage::UsageCounters::default(),
            command: None,
        })
    }

    pub(super) fn for_command(
        argv: Vec<String>,
        model: &str,
        revision: Option<&str>,
    ) -> Result<Self, CliError> {
        let argv = argv
            .into_iter()
            .map(|argument| {
                if argument == "{model}" {
                    model.to_owned()
                } else {
                    argument
                }
            })
            .collect::<Vec<_>>();
        command::validate(&argv).map_err(CliError::Usage)?;
        let mut provider = Self::new(None, model, None, false)?;
        let encoded = serde_json::to_vec(&argv).map_err(CliError::Encode)?;
        provider.identity.provider = "local-command".to_owned();
        provider.identity.configuration_hash = *blake3::hash(&encoded).as_bytes();
        let revision = revision.unwrap_or(model);
        if revision.trim().is_empty() || revision.len() > 512 {
            return Err(CliError::Usage(
                "semantic command revision requires 1–512 bytes".to_owned(),
            ));
        }
        provider.identity.model_revision = format!("command-configured:{revision}");
        provider.command = Some(argv);
        provider.parallel_limit = 1;
        Ok(provider)
    }

    pub(super) const fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    pub(super) const fn set_cross_document(&mut self, enabled: bool) {
        self.cross_document = enabled;
    }

    pub(super) const fn cross_document_enabled(&self) -> bool {
        self.cross_document
    }

    pub(super) fn requests_made(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }

    pub(super) fn metadata_requests_made(&self) -> usize {
        self.metadata_request_count.load(Ordering::Relaxed)
    }

    pub(super) fn usage_summary(&self) -> String {
        self.usage.summary()
    }

    pub(super) fn semantic_requests_submitted(&self) -> usize {
        self.semantic_request_count.load(Ordering::Relaxed)
    }

    fn completion_url(&self) -> Url {
        let mut url = self.endpoint.clone();
        let path = format!("{}/chat/completions", url.path().trim_end_matches('/'));
        url.set_path(&path);
        url
    }

    fn send_request(&self, body: &serde_json::Value) -> Result<ChatResponse, String> {
        if self.offline {
            return Err("semantic cache miss in --semantic-offline mode; run online with the same configuration to enrich changed documents".to_owned());
        }
        let endpoint = self.completion_url();
        let mut last_error = "semantic provider request failed".to_owned();
        for attempt in 0..3_u32 {
            self.request_count.fetch_add(1, Ordering::Relaxed);
            let mut request = self.client.post(endpoint.clone()).json(body);
            if let Some(api_key) = &self.api_key {
                request = request.bearer_auth(api_key);
            }
            match request.send() {
                Ok(mut response) => {
                    let status = response.status();
                    if !status.is_success() {
                        self.usage.record(None);
                        if retryable(status) && attempt < 2 {
                            std::thread::sleep(retry::delay(response.headers(), attempt)?);
                            last_error = format!("semantic provider returned HTTP {status}");
                            continue;
                        }
                        return Err(format!("semantic provider returned HTTP {status}"));
                    }
                    let mut bytes = Vec::new();
                    response
                        .by_ref()
                        .take(
                            u64::try_from(MAX_RESPONSE_BYTES.saturating_add(1)).unwrap_or(u64::MAX),
                        )
                        .read_to_end(&mut bytes)
                        .map_err(|_error| {
                            self.usage.record(None);
                            "could not read semantic provider response".to_owned()
                        })?;
                    if bytes.len() > MAX_RESPONSE_BYTES {
                        self.usage.record(None);
                        return Err(
                            "semantic provider response exceeded the 4 MiB limit".to_owned()
                        );
                    }
                    let value: serde_json::Value =
                        serde_json::from_slice(&bytes).map_err(|_error| {
                            self.usage.record(None);
                            "semantic provider returned invalid response JSON".to_owned()
                        })?;
                    self.usage.record(value.get("usage"));
                    return serde_json::from_value(value).map_err(|_error| {
                        "semantic provider returned invalid response JSON".to_owned()
                    });
                }
                Err(_) if attempt < 2 => {
                    self.usage.record(None);
                    last_error = "semantic provider connection failed".to_owned();
                    std::thread::sleep(Duration::from_millis(250_u64 << attempt));
                }
                Err(_) => {
                    self.usage.record(None);
                    return Err("semantic provider connection failed".to_owned());
                }
            }
        }
        Err(last_error)
    }
}

impl OpenAiCompatibleProvider {
    fn extract_requests(&self, requests: &[SemanticRequest]) -> Result<SemanticOutput, String> {
        adaptive::extract(requests, &|group| self.extract_prompt(group))
    }

    fn extract_prompt(
        &self,
        requests: &[SemanticRequest],
    ) -> Result<SemanticOutput, adaptive::PromptError> {
        SemanticRequest::combine(requests.to_vec())
            .map_err(|error| format!("semantic request grouping: {error}"))?;
        let input = prompt::encode(requests)?;
        if input.len() > MAX_PROMPT_BYTES {
            return Err(adaptive::PromptError::Failed(
                "semantic request exceeds serialized prompt bounds".to_owned(),
            ));
        }
        let content = if let Some(argv) = &self.command {
            if self.offline {
                return Err(adaptive::PromptError::Failed(
                    "semantic command cache miss in --semantic-offline mode".to_owned(),
                ));
            }
            self.request_count.fetch_add(1, Ordering::Relaxed);
            self.usage.record(None);
            command::run(argv, &format!("{}\n\nInput:\n{input}", policy::SYSTEM))?
        } else {
            let response = self.send_request(&json!({
                "model": self.model,
                "temperature": 0,
                "stream": false,
                "response_format": { "type": "json_object" },
                "messages": [
                    { "role": "system", "content": policy::SYSTEM },
                    { "role": "user", "content": input },
                ],
            }))?;
            self.verify_model_revision()?;
            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| "semantic provider returned no completion choices".to_owned())?;
            if choice.finish_reason.as_deref() == Some("length") {
                return Err(adaptive::PromptError::Truncated);
            }
            choice.message.content.ok_or_else(|| {
                "semantic provider returned no claim content without a truncation signal".to_owned()
            })?
        };
        let wire: WireSemanticOutput = serde_json::from_str(&content).map_err(|_error| {
            "semantic provider content was not a valid claim document".to_owned()
        })?;
        wire.try_into().map_err(adaptive::PromptError::from)
    }
}

impl SemanticProvider for OpenAiCompatibleProvider {
    fn identity(&self) -> SemanticProviderIdentity {
        self.identity.clone()
    }

    fn extract(&self, request: &SemanticRequest) -> Result<SemanticOutput, String> {
        self.extract_requests(std::slice::from_ref(request))
    }

    fn extract_many(&self, requests: &[SemanticRequest]) -> Vec<Result<SemanticOutput, String>> {
        if requests.is_empty() {
            return Vec::new();
        }
        self.semantic_request_count
            .fetch_add(requests.len(), Ordering::Relaxed);
        let mut results = (0..requests.len())
            .map(|_| None)
            .collect::<Vec<Option<Result<SemanticOutput, String>>>>();
        let mut groups = Vec::<Vec<usize>>::new();
        let mut current_group = Vec::new();
        let mut current_chunks = 0_usize;
        let mut current_bytes = prompt::ENVELOPE_BYTES;
        for (position, request) in requests.iter().enumerate() {
            let (encoded, chunk_count) = match prompt::encode_request(request) {
                Ok(encoded) => encoded,
                Err(error) => {
                    if let Some(slot) = results.get_mut(position) {
                        *slot = Some(Err(error));
                    }
                    continue;
                }
            };
            let byte_count = encoded.len();
            if chunk_count > MAX_CHUNKS_PER_PROMPT
                || prompt::ENVELOPE_BYTES.saturating_add(byte_count) > MAX_PROMPT_BYTES
            {
                if let Some(slot) = results.get_mut(position) {
                    *slot = Some(Err("semantic request exceeds prompt bounds".to_owned()));
                }
                continue;
            }
            if !current_group.is_empty()
                && (current_group.len() == MAX_DOCUMENTS_PER_PROMPT
                    || current_chunks.saturating_add(chunk_count) > MAX_CHUNKS_PER_PROMPT
                    || current_bytes
                        .saturating_add(byte_count)
                        .saturating_add(usize::from(!current_group.is_empty()))
                        > MAX_PROMPT_BYTES)
            {
                groups.push(std::mem::take(&mut current_group));
                current_chunks = 0;
                current_bytes = prompt::ENVELOPE_BYTES;
            }
            current_bytes = current_bytes
                .saturating_add(byte_count)
                .saturating_add(usize::from(!current_group.is_empty()));
            current_group.push(position);
            current_chunks = current_chunks.saturating_add(chunk_count);
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }

        eprintln!(
            "semantic_progress pending_requests={} prompt_groups={} parallel_limit={}",
            requests.len(),
            groups.len(),
            self.parallel_limit
        );

        for parallel_group in groups.chunks(self.parallel_limit) {
            let completed = std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(parallel_group.len());
                for positions in parallel_group {
                    handles.push(scope.spawn(move || {
                        let grouped_requests = positions
                            .iter()
                            .filter_map(|position| requests.get(*position).cloned())
                            .collect::<Vec<_>>();
                        let group_result = adaptive::extract_many(&grouped_requests, &|group| {
                            let output = self.extract_prompt(group)?;
                            let local_positions = (0..group.len()).collect::<Vec<_>>();
                            split_output_by_request(output, &local_positions, group)
                                .map_err(adaptive::PromptError::from)
                        });
                        let validated = group_result.iter().filter(|result| result.is_ok()).count();
                        let failed = group_result.iter().filter(|result| result.is_err()).count();
                        eprintln!(
                            "semantic_progress group_requests={} validated_requests={validated} failed_requests={failed}",
                            grouped_requests.len()
                        );
                        (positions.clone(), group_result)
                    }));
                }
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap_or_else(|_| (Vec::new(), Vec::new())))
                    .collect::<Vec<_>>()
            });
            for (positions, group_result) in completed {
                for (position, document_output) in positions.into_iter().zip(group_result) {
                    if let Some(slot) = results.get_mut(position) {
                        *slot = Some(document_output);
                    }
                }
            }
        }
        results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| {
                    Err("semantic provider omitted a document result".to_owned())
                })
            })
            .collect()
    }
}

fn split_output_by_request(
    output: SemanticOutput,
    positions: &[usize],
    requests: &[SemanticRequest],
) -> Result<Vec<SemanticOutput>, String> {
    let grouped_requests = positions
        .iter()
        .filter_map(|position| requests.get(*position).cloned())
        .collect::<Vec<_>>();
    let combined = SemanticRequest::combine(grouped_requests)
        .map_err(|error| format!("semantic request grouping: {error}"))?;
    combined
        .into_fact_batch(output.clone())
        .map_err(|error| format!("semantic evidence validation: {error}"))?;
    let allowed_hashes = positions
        .iter()
        .filter_map(|position| requests.get(*position))
        .map(|request| {
            request
                .prompt_chunks()
                .into_iter()
                .map(|chunk| chunk.content_hash)
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut outputs = (0..positions.len())
        .map(|_| SemanticOutput::default())
        .collect::<Vec<_>>();
    for claim in output.claims {
        let claim_owners = allowed_hashes
            .iter()
            .enumerate()
            .filter(|(_, hashes)| {
                claim
                    .evidence
                    .iter()
                    .any(|item| hashes.contains(&item.chunk_content_hash))
            })
            .filter_map(|(index, _)| {
                positions
                    .get(index)
                    .and_then(|position| requests.get(*position))
                    .map(SemanticRequest::cache_key)
            })
            .collect::<BTreeSet<_>>();
        if claim_owners.len() > 1 {
            return Err(
                "semantic claim combines evidence from independently cached requests".to_owned(),
            );
        }
        for (index, hashes) in allowed_hashes.iter().enumerate() {
            let evidence = claim
                .evidence
                .iter()
                .filter(|item| hashes.contains(&item.chunk_content_hash))
                .cloned()
                .collect::<Vec<_>>();
            if !evidence.is_empty()
                && let Some(result) = outputs.get_mut(index)
            {
                result.claims.push(SemanticClaim {
                    evidence,
                    ..claim.clone()
                });
            }
        }
    }
    Ok(outputs)
}

fn retryable(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    finish_reason: Option<String>,
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSemanticOutput {
    claims: Vec<WireSemanticClaim>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSemanticClaim {
    subject: String,
    relation: String,
    object: String,
    evidence: Vec<WireSemanticEvidence>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSemanticEvidence {
    chunk_content_hash: String,
    quote: String,
}

impl TryFrom<WireSemanticOutput> for SemanticOutput {
    type Error = String;

    fn try_from(value: WireSemanticOutput) -> Result<Self, Self::Error> {
        let claims = value
            .claims
            .into_iter()
            .map(|claim| {
                let evidence = claim
                    .evidence
                    .into_iter()
                    .map(|item| {
                        let bytes = hex::decode(item.chunk_content_hash).map_err(|_error| {
                            "semantic evidence hash is not lowercase hex".to_owned()
                        })?;
                        let chunk_content_hash: [u8; 32] = bytes.try_into().map_err(|_error| {
                            "semantic evidence hash must encode exactly 32 bytes".to_owned()
                        })?;
                        Ok(SemanticEvidence {
                            chunk_content_hash,
                            quote: item.quote,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(SemanticClaim {
                    subject: claim.subject,
                    relation: claim.relation,
                    object: claim.object,
                    evidence,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(SemanticOutput { claims })
    }
}
