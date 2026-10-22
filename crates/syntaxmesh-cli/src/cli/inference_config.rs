use super::CliError;
use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use syntaxmesh_core::StableId;
use syntaxmesh_inference::{
    InvalidationLimits, Limits, OntologyDefinition, OntologyRegistry, RegisteredInferenceAdmission,
    Rule,
};

const MAX_BYTES: u64 = 1_048_576;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Configuration {
    pub version: u32,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub ontology_definitions: Vec<OntologyDefinition>,
    #[serde(default)]
    pub registry_entries: usize,
    #[serde(default)]
    pub registry_bytes: usize,
    #[serde(default)]
    pub bindings: usize,
    pub limits: Limits,
    pub range_limits: InvalidationLimits,
    pub history_scan_budget: usize,
    pub rule_budget: usize,
    pub output_bytes: usize,
    pub premise_budget: usize,
    pub evidence_budget: usize,
    pub matched_claim_bytes: usize,
    pub page_budget: usize,
}

impl Configuration {
    pub const fn is_registered(&self) -> bool {
        self.version == 2
    }

    pub const fn admission(&self) -> RegisteredInferenceAdmission {
        RegisteredInferenceAdmission {
            registry_entries: self.registry_entries,
            registry_bytes: self.registry_bytes,
            bindings: self.bindings,
        }
    }

    pub fn effective_rules(&self) -> Result<Vec<Rule>, CliError> {
        if !self.is_registered() {
            return Ok(self.rules.clone());
        }
        let registry = OntologyRegistry::from_definitions(
            &self.ontology_definitions,
            self.registry_entries,
            self.registry_bytes,
        )
        .map_err(|error| invalid(&error.to_string()))?;
        let bound = registry
            .bind_rules(
                &self.rules,
                self.rule_budget,
                self.bindings,
                self.limits.input_bytes,
            )
            .map_err(|error| invalid(&error.to_string()))?;
        Ok(bound.rules().to_vec())
    }
}

pub(super) fn check(path: &Path) -> Result<StableId, CliError> {
    let configuration = load(path)?;
    syntaxmesh_inference::rule_set_identity(
        &configuration.effective_rules()?,
        configuration.rule_budget,
        configuration.limits.input_bytes,
    )
    .map_err(|error| invalid(&error.to_string()))
}

pub(super) fn load(path: &Path) -> Result<Configuration, CliError> {
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("configuration must be a regular file"));
    }
    decode(file)
}

fn decode(reader: impl Read) -> Result<Configuration, CliError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(|_error| invalid("configuration size overflow"))?
        > MAX_BYTES
    {
        return Err(invalid("configuration exceeds 1 MiB"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(CliError::Encode)?;
    for rule in value
        .get("rules")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        fields(
            rule,
            &[
                "id",
                "version",
                "ontology_versions",
                "configuration_fingerprint",
                "path_relation",
                "join_relation",
                "declaration_relation",
                "concept",
                "output_relation",
            ],
        )?;
    }
    if let Some(limits) = value.get("limits") {
        fields(
            limits,
            &["input_bytes", "facts", "edge_visits", "depth", "claims"],
        )?;
    }
    if let Some(limits) = value.get("range_limits") {
        fields(limits, &["generations", "versions", "postings", "claims"])?;
    }
    for definition in value
        .get("ontology_definitions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        fields(
            definition,
            &[
                "schema_version",
                "namespace",
                "version",
                "concepts",
                "instance_relation",
                "relations",
            ],
        )?;
        for relation in definition
            .get("relations")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            fields(relation, &["name", "subject_domain", "object_range"])?;
        }
    }
    let configuration: Configuration = serde_json::from_slice(&bytes).map_err(CliError::Encode)?;
    if !matches!(configuration.version, 1 | 2) {
        return Err(invalid("unsupported configuration version"));
    }
    if configuration.version == 1
        && (!configuration.ontology_definitions.is_empty()
            || configuration.registry_entries != 0
            || configuration.registry_bytes != 0
            || configuration.bindings != 0)
    {
        return Err(invalid(
            "version 1 cannot contain registered ontology configuration",
        ));
    }
    let limits = configuration.limits;
    let range = configuration.range_limits;
    if [
        limits.input_bytes,
        limits.facts,
        limits.edge_visits,
        limits.depth,
        limits.claims,
        range.generations,
        range.versions,
        range.postings,
        range.claims,
        configuration.history_scan_budget,
        configuration.rule_budget,
        configuration.output_bytes,
        configuration.premise_budget,
        configuration.evidence_budget,
        configuration.matched_claim_bytes,
        configuration.page_budget,
    ]
    .contains(&0)
    {
        return Err(invalid("positive inference limits required"));
    }
    if configuration.is_registered()
        && (configuration.registry_entries == 0
            || configuration.registry_bytes == 0
            || configuration.bindings == 0
            || configuration.ontology_definitions.is_empty())
    {
        return Err(invalid("positive registered ontology admission required"));
    }
    syntaxmesh_inference::rule_set_identity(
        &configuration.effective_rules()?,
        configuration.rule_budget,
        limits.input_bytes,
    )
    .map_err(|error| invalid(&error.to_string()))?;
    Ok(configuration)
}

fn fields(value: &serde_json::Value, allowed: &[&str]) -> Result<(), CliError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("configuration object required"))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("unknown inference configuration field"));
    }
    Ok(())
}

fn invalid(message: &str) -> CliError {
    CliError::Usage(format!("inference configuration: {message}"))
}

#[cfg(test)]
mod tests;
