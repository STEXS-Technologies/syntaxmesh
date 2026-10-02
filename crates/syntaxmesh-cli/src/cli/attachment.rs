use std::io::Read;
use std::time::Duration;

use serde::Deserialize;
use syntaxmesh_core::{GenerationId, Node, NodeId};
use syntaxmesh_ownership_host::OwnerEndpoint;

use super::{CliError, parse_generation_id};

const RESPONSE_LIMIT: u64 = 4 * 1024 * 1024;

mod diagnostics;
mod integrity;
pub(super) use integrity::backend_integrity;
mod neighbors;
mod rejections;
mod status;
pub(super) use diagnostics::resolution_diagnostics;
pub(super) use rejections::workflow_rejections;
pub(super) use status::status;
#[cfg(test)]
mod tests;
pub(super) use neighbors::historical::historical_neighbors;
pub(super) use neighbors::neighbors;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response<T> {
    schema_version: u32,
    generation: String,
    data: T,
}

pub(super) fn search_nodes(endpoint: &OwnerEndpoint, text: &str) -> Result<Vec<Node>, CliError> {
    let (_, nodes): (_, Vec<Node>) =
        Client::new(endpoint)?.get_data("/api/v1/search", &[("text", text), ("limit", "100")])?;
    if nodes.len() > 100 {
        return Err(CliError::Usage(
            "daemon attachment failed: invalid search result count".to_owned(),
        ));
    }
    Ok(nodes)
}

pub(super) fn node(
    endpoint: &OwnerEndpoint,
    id: NodeId,
    selected: Option<GenerationId>,
) -> Result<Node, CliError> {
    let generation_label = selected.map(|selected_id| selected_id.0.to_hex());
    let parameters = generation_label
        .as_deref()
        .map(|value| vec![("generation", value)])
        .unwrap_or_default();
    let (actual, node): (_, Node) = Client::new(endpoint)?
        .get_data(&format!("/api/v1/nodes/{}", id.0.to_hex()), &parameters)?;
    if node.id != id || selected.is_some_and(|generation| generation != actual) {
        return Err(CliError::Usage(
            "daemon attachment failed: node or generation mismatch".to_owned(),
        ));
    }
    Ok(node)
}

struct Client<'owner> {
    endpoint: &'owner OwnerEndpoint,
    http: reqwest::blocking::Client,
}

impl<'owner> Client<'owner> {
    fn new(endpoint: &'owner OwnerEndpoint) -> Result<Self, CliError> {
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| CliError::Usage(format!("daemon attachment failed: {error}")))?;
        Ok(Self { endpoint, http })
    }

    fn get_data<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        parameters: &[(&str, &str)],
    ) -> Result<(GenerationId, T), CliError> {
        self.get_optional_data(path, parameters)?
            .ok_or_else(|| CliError::Usage("daemon attachment failed: HTTP 404".to_owned()))
    }

    fn get_optional_data<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        parameters: &[(&str, &str)],
    ) -> Result<Option<(GenerationId, T)>, CliError> {
        let failure =
            |message: String| CliError::Usage(format!("daemon attachment failed: {message}"));
        let response = self
            .http
            .get(format!("http://{}{path}", self.endpoint.address()))
            .query(parameters)
            .header("x-syntaxmesh-owner-instance", self.endpoint.instance())
            .send()
            .map_err(|error| failure(error.to_string()))?;
        let mut instances = response
            .headers()
            .get_all("x-syntaxmesh-owner-instance")
            .iter();
        if instances.next().and_then(|value| value.to_str().ok()) != Some(self.endpoint.instance())
            || instances.next().is_some()
        {
            return Err(failure("owner instance response mismatch".to_owned()));
        }
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(failure(format!("HTTP {}", response.status())));
        }
        let mut bytes = Vec::new();
        response
            .take(RESPONSE_LIMIT.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len()).map_err(|error| failure(error.to_string()))? > RESPONSE_LIMIT
        {
            return Err(failure("response exceeds 4 MiB".to_owned()));
        }
        let result: Response<T> = serde_json::from_slice(&bytes).map_err(CliError::Encode)?;
        if result.schema_version != 1 {
            return Err(failure("invalid response schema".to_owned()));
        }
        Ok(Some((
            parse_generation_id(&result.generation)?,
            result.data,
        )))
    }
}
