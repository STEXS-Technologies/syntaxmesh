use super::CliError;
use crate::cli::parse_node_id;
use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use syntaxmesh_core::RelationKind;
use syntaxmesh_inference::{Contract, ContractLimits, ContractOrigin, RelationExpectation};

const MAX_BYTES: u64 = 1_048_576;
const MAX_CONTRACTS: usize = 1_000;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Configuration {
    version: u32,
    contracts: Vec<ContractInput>,
    limits: LimitsInput,
    history_scan_budget: usize,
    output_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractInput {
    id: String,
    version: String,
    origin: OriginInput,
    source: String,
    target: String,
    relation: RelationInput,
    expectation: ExpectationInput,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OriginInput {
    SourceDerived,
    SchemaDerived,
    ConfigDerived,
    Declared,
    Documented,
    RuntimeInferred,
    ModelSuggested,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExpectationInput {
    Required,
    Forbidden,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RelationInput {
    Defines,
    Imports,
    References,
    Calls,
    Implements,
    ResolvesTo,
    Exports,
    HasResolutionDiagnostic,
    Contains,
    External { namespace: String, relation: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitsInput {
    contracts: usize,
    input_bytes: usize,
    edge_visits: usize,
    results: usize,
}

impl Configuration {
    pub fn contracts(&self) -> Result<Vec<Contract>, CliError> {
        self.contracts
            .iter()
            .map(|contract| {
                Ok(Contract {
                    id: contract.id.clone(),
                    version: contract.version.clone(),
                    origin: match contract.origin {
                        OriginInput::SourceDerived => ContractOrigin::SourceDerived,
                        OriginInput::SchemaDerived => ContractOrigin::SchemaDerived,
                        OriginInput::ConfigDerived => ContractOrigin::ConfigDerived,
                        OriginInput::Declared => ContractOrigin::Declared,
                        OriginInput::Documented => ContractOrigin::Documented,
                        OriginInput::RuntimeInferred => ContractOrigin::RuntimeInferred,
                        OriginInput::ModelSuggested => ContractOrigin::ModelSuggested,
                    },
                    source: parse_node_id(&contract.source)?,
                    target: parse_node_id(&contract.target)?,
                    relation: match &contract.relation {
                        RelationInput::Defines => RelationKind::Defines,
                        RelationInput::Imports => RelationKind::Imports,
                        RelationInput::References => RelationKind::References,
                        RelationInput::Calls => RelationKind::Calls,
                        RelationInput::Implements => RelationKind::Implements,
                        RelationInput::ResolvesTo => RelationKind::ResolvesTo,
                        RelationInput::Exports => RelationKind::Exports,
                        RelationInput::HasResolutionDiagnostic => {
                            RelationKind::HasResolutionDiagnostic
                        }
                        RelationInput::Contains => RelationKind::Contains,
                        RelationInput::External {
                            namespace,
                            relation,
                        } => RelationKind::External {
                            namespace: namespace.clone(),
                            relation: relation.clone(),
                        },
                    },
                    expectation: match contract.expectation {
                        ExpectationInput::Required => RelationExpectation::Required,
                        ExpectationInput::Forbidden => RelationExpectation::Forbidden,
                    },
                })
            })
            .collect()
    }

    pub const fn limits(&self) -> ContractLimits {
        ContractLimits {
            contracts: self.limits.contracts,
            input_bytes: self.limits.input_bytes,
            edge_visits: self.limits.edge_visits,
            results: self.limits.results,
        }
    }

    pub const fn history_scan_budget(&self) -> usize {
        self.history_scan_budget
    }

    pub const fn output_bytes(&self) -> usize {
        self.output_bytes
    }
}

pub(super) fn load(path: &Path) -> Result<Configuration, CliError> {
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("contract configuration must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(|_| invalid("configuration size overflow"))? > MAX_BYTES {
        return Err(invalid("contract configuration exceeds 1 MiB"));
    }
    let config: Configuration = serde_json::from_slice(&bytes).map_err(CliError::Encode)?;
    if config.version != 1 {
        return Err(invalid("unsupported contract configuration version"));
    }
    if config.contracts.is_empty()
        || config.contracts.len() > MAX_CONTRACTS
        || config.limits.contracts == 0
        || config.limits.contracts > MAX_CONTRACTS
        || config.contracts.len() > config.limits.contracts
        || config.contracts.len() > config.limits.results
        || config.limits.input_bytes == 0
        || config.limits.input_bytes > usize::try_from(MAX_BYTES).unwrap_or(usize::MAX)
        || config.limits.edge_visits == 0
        || config.history_scan_budget == 0
        || config.output_bytes == 0
        || config.output_bytes > usize::try_from(MAX_BYTES).unwrap_or(usize::MAX)
    {
        return Err(invalid("invalid or exhausted contract admission bounds"));
    }
    Ok(config)
}

fn invalid(message: &str) -> CliError {
    CliError::Usage(message.to_owned())
}
