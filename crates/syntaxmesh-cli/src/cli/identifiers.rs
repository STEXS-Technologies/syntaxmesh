use syntaxmesh_core::{
    AcceptanceTime, ChangeSetId, EdgeId, EvidenceClass, FactRef, FactVersionRef, FileId,
    GenerationId, IndexRunId, LineageEndpoint, NodeId, ObservationTime, ProvenanceId, StableId,
};

use super::CliError;

pub(super) fn parse_consequence_seed(
    kind: &str,
    fact_kind: Option<&str>,
    id: &str,
    valid_from: Option<&str>,
) -> Result<LineageEndpoint, CliError> {
    match kind {
        "event" if valid_from.is_none() => {
            let id = decode_stable_id("change event", id)?;
            Ok(LineageEndpoint::ChangeEvent(
                syntaxmesh_core::ChangeEventId(id),
            ))
        }
        "fact" => {
            let fact_kind = fact_kind
                .ok_or_else(|| CliError::Usage("fact seed requires a fact kind".to_owned()))?;
            let valid_from = valid_from.ok_or_else(|| {
                CliError::Usage("fact seed requires its valid-from generation".to_owned())
            })?;
            Ok(LineageEndpoint::FactVersion(FactVersionRef {
                fact: parse_fact_ref(fact_kind, id)?,
                valid_from: parse_generation_id(valid_from)?,
            }))
        }
        _ => Err(CliError::Usage(
            "seed must be event <id> or fact <kind> <id> <valid-from>".to_owned(),
        )),
    }
}

fn decode_stable_id(label: &str, value: &str) -> Result<StableId, CliError> {
    let bytes = hex::decode(value)
        .map_err(|error| CliError::Usage(format!("invalid hexadecimal {label} ID: {error}")))?;
    let bytes = <[u8; 32]>::try_from(bytes).map_err(|invalid| {
        CliError::Usage(format!(
            "{label} ID must encode exactly 32 bytes, got {}",
            invalid.len()
        ))
    })?;
    Ok(StableId(bytes))
}

pub(super) fn parse_node_id(value: &str) -> Result<NodeId, CliError> {
    decode_stable_id("node", value).map(NodeId)
}

pub(super) fn parse_repository_id(value: &str) -> Result<syntaxmesh_core::RepositoryId, CliError> {
    decode_stable_id("repository", value).map(syntaxmesh_core::RepositoryId)
}

pub(super) fn parse_edge_id(value: &str) -> Result<EdgeId, CliError> {
    decode_stable_id("edge", value).map(EdgeId)
}

pub(super) fn parse_generation_id(value: &str) -> Result<GenerationId, CliError> {
    decode_stable_id("generation", value).map(GenerationId)
}

pub(super) fn parse_file_id(value: &str) -> Result<FileId, CliError> {
    decode_stable_id("file", value).map(FileId)
}

pub(super) fn parse_worktree_id(value: &str) -> Result<syntaxmesh_core::WorktreeId, CliError> {
    decode_stable_id("worktree", value).map(syntaxmesh_core::WorktreeId)
}

pub(super) fn parse_change_set_id(value: &str) -> Result<ChangeSetId, CliError> {
    decode_stable_id("ChangeSet", value).map(ChangeSetId)
}

pub(super) fn parse_index_run_id(value: &str) -> Result<IndexRunId, CliError> {
    decode_stable_id("index run", value).map(IndexRunId)
}

pub(super) fn parse_fact_ref(kind: &str, value: &str) -> Result<FactRef, CliError> {
    let id = decode_stable_id(kind, value)?;
    match kind {
        "file" => Ok(FactRef::File(FileId(id))),
        "provenance" => Ok(FactRef::Provenance(ProvenanceId(id))),
        "node" => Ok(FactRef::Node(NodeId(id))),
        "edge" => Ok(FactRef::Edge(EdgeId(id))),
        _ => Err(CliError::Usage(format!(
            "unknown fact kind {kind:?}; expected file, provenance, node, or edge"
        ))),
    }
}

pub(super) fn parse_limit(value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|error| CliError::Usage(format!("invalid limit: {error}")))
}

pub(super) fn parse_evidence_classes(value: &str) -> Result<Vec<EvidenceClass>, CliError> {
    if value.is_empty() {
        return Err(CliError::Usage(
            "evidence-class set must not be empty; omit the option to include all classes"
                .to_owned(),
        ));
    }
    let mut classes = Vec::new();
    for name in value.split(',') {
        let class = match name {
            "source_fact" => EvidenceClass::SourceFact,
            "statically_resolved" => EvidenceClass::StaticallyResolved,
            "heuristic" => EvidenceClass::Heuristic,
            "semantic_inference" => EvidenceClass::SemanticInference,
            "user_asserted" => EvidenceClass::UserAsserted,
            "runtime_observed" => EvidenceClass::RuntimeObserved,
            "committed_transition" => EvidenceClass::CommittedTransition,
            "resolution_diagnostic" => EvidenceClass::ResolutionDiagnostic,
            _ => {
                return Err(CliError::Usage(format!(
                    "unknown evidence class {name:?}; expected source_fact, statically_resolved, heuristic, semantic_inference, user_asserted, runtime_observed, committed_transition, or resolution_diagnostic"
                )));
            }
        };
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    Ok(classes)
}

pub(super) fn parse_acceptance_time(value: &str) -> Result<AcceptanceTime, CliError> {
    value
        .parse::<u64>()
        .map(AcceptanceTime)
        .map_err(|error| CliError::Usage(format!("invalid Unix timestamp in nanoseconds: {error}")))
}

pub(super) fn parse_observation_time(value: &str) -> Result<ObservationTime, CliError> {
    value.parse::<u64>().map(ObservationTime).map_err(|error| {
        CliError::Usage(format!(
            "invalid observed Unix timestamp in nanoseconds: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        parse_acceptance_time, parse_change_set_id, parse_consequence_seed, parse_evidence_classes,
        parse_fact_ref, parse_generation_id, parse_limit, parse_node_id, parse_observation_time,
    };
    use syntaxmesh_core::{
        AcceptanceTime, ChangeSetId, EdgeId, EvidenceClass, FactRef, FactVersionRef, FileId,
        GenerationId, LineageEndpoint, NodeId, ObservationTime, ProvenanceId, StableId,
    };

    #[test]
    fn parses_a_stable_node_id_from_hex() -> Result<(), String> {
        let text = "ab".repeat(32);
        let id = parse_node_id(&text).map_err(|error| error.to_string())?;
        if id.0.0 != [0xab; 32] {
            return Err("parsed node ID has unexpected bytes".to_owned());
        }
        Ok(())
    }

    #[test]
    fn rejects_invalid_node_ids_and_hop_limits() -> Result<(), String> {
        if parse_node_id("abc").is_ok()
            || parse_node_id(&"00".repeat(31)).is_ok()
            || parse_limit("zero").is_ok()
        {
            return Err("invalid node ID or hop limit was accepted".to_owned());
        }
        Ok(())
    }

    #[test]
    fn parses_a_stable_generation_and_change_set_id_from_hex() -> Result<(), String> {
        let generation = GenerationId::derive(&[b"cli-generation"]);
        let parsed_generation =
            parse_generation_id(&generation.0.to_hex()).map_err(|error| error.to_string())?;
        let change_set = ChangeSetId::derive(&[b"cli-change-set"]);
        let parsed_change_set =
            parse_change_set_id(&change_set.0.to_hex()).map_err(|error| error.to_string())?;
        if parsed_generation != generation || parsed_change_set != change_set {
            return Err("parsed temporal ID differs from expected ID".to_owned());
        }
        Ok(())
    }

    #[test]
    fn parses_unix_nanosecond_acceptance_time() -> Result<(), String> {
        if parse_acceptance_time("18446744073709551615").map_err(|error| error.to_string())?
            != AcceptanceTime(u64::MAX)
            || parse_acceptance_time("-1").is_ok()
        {
            return Err("acceptance timestamp parsing did not preserve u64 semantics".to_owned());
        }
        Ok(())
    }

    #[test]
    fn parses_unix_nanosecond_observation_time() -> Result<(), String> {
        let actual = parse_observation_time("123").map_err(|error| error.to_string())?;
        if actual != ObservationTime(123) || parse_observation_time("nope").is_ok() {
            return Err(
                "observation-time parser accepted or returned an incorrect value".to_owned(),
            );
        }
        Ok(())
    }

    #[test]
    fn parses_exact_deduplicated_evidence_class_sets() -> Result<(), String> {
        let parsed = parse_evidence_classes("user_asserted,source_fact,user_asserted")
            .map_err(|error| error.to_string())?;
        if parsed != vec![EvidenceClass::UserAsserted, EvidenceClass::SourceFact]
            || parse_evidence_classes("unknown").is_ok()
            || parse_evidence_classes("").is_ok()
        {
            return Err("evidence-class set parsing was not exact or deterministic".to_owned());
        }
        Ok(())
    }

    #[test]
    fn parses_fact_family_ids_without_losing_their_type() -> Result<(), String> {
        let id = StableId::derive("cli-fact", &[b"fixture"]);
        let encoded = id.to_hex();
        let parsed = ["file", "provenance", "node", "edge"]
            .into_iter()
            .map(|kind| parse_fact_ref(kind, &encoded).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        if parsed
            != vec![
                FactRef::File(FileId(id)),
                FactRef::Provenance(ProvenanceId(id)),
                FactRef::Node(NodeId(id)),
                FactRef::Edge(EdgeId(id)),
            ]
        {
            return Err("fact family parser changed a typed identity".to_owned());
        }
        Ok(())
    }

    #[test]
    fn parses_event_and_exact_fact_version_consequence_seeds() -> Result<(), String> {
        let id = StableId::derive("cli-seed", &[b"fixture"]);
        let event = parse_consequence_seed("event", None, &id.to_hex(), None)
            .map_err(|error| error.to_string())?;
        let generation = GenerationId::derive(&[b"cli-seed-generation"]);
        let fact = parse_consequence_seed(
            "fact",
            Some("node"),
            &id.to_hex(),
            Some(&generation.0.to_hex()),
        )
        .map_err(|error| error.to_string())?;
        if event != LineageEndpoint::ChangeEvent(syntaxmesh_core::ChangeEventId(id))
            || fact
                != LineageEndpoint::FactVersion(FactVersionRef {
                    fact: FactRef::Node(NodeId(id)),
                    valid_from: generation,
                })
            || parse_consequence_seed("fact", None, &id.to_hex(), None).is_ok()
        {
            return Err(
                "consequence seed parser changed or accepted an invalid endpoint".to_owned(),
            );
        }
        Ok(())
    }
}
