use super::super::*;
use syntaxmesh_core::{
    ChangeEventId, ChangeSetId, ChangeSetMembershipKey, ChangeSetMembershipRemoval,
    ConsequenceEdgeId, ConsequenceRetraction, GenerationId, Node, NodeId, NodeKind, ProvenanceId,
};

#[test]
fn borrowed_prepared_records_match_owned_wire_bytes() -> Result<(), WorkflowError> {
    let delta = super::super::tests::test_delta(
        IndexRunId::derive(&[b"borrowed-wire-run"]),
        None,
        GenerationId::derive(&[b"borrowed-wire-generation"]),
    );
    let mut record = prepare_record(
        delta,
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(123),
    )?;
    let provenance = ProvenanceId::derive(&[b"wire-provenance"]);
    record.delta.upsert_nodes.push(Node {
        id: NodeId::derive(&[b"wire-node"]),
        kind: NodeKind::Function,
        name: "borrowed Unicode λ record".to_owned(),
        owner_file: None,
        source: None,
        provenance,
        extension_payload: None,
    });
    record
        .lineage
        .unassign_events
        .push(ChangeSetMembershipRemoval {
            key: ChangeSetMembershipKey {
                change_set: ChangeSetId::derive(&[b"wire-set"]),
                event: ChangeEventId::derive(&[b"wire-event"]),
            },
            provenance,
        });
    record.consequences.retract.push(ConsequenceRetraction {
        edge: ConsequenceEdgeId::derive(&[b"wire-consequence"]),
        provenance,
    });
    for manifest in [
        None,
        Some(GenerationManifest {
            repository: record.delta.repository,
            worktree: record.delta.worktree,
            generation: record.delta.next_generation,
            parent: None,
            graph_root: [1; 32],
            configuration_hash: [2; 32],
            extractor_set_hash: [3; 32],
            schema_version: 2,
            status: GenerationStatus::Durable,
        }),
    ] {
        record.accepted_generation = manifest;
        for version in 1..=4 {
            record.schema_version = version;
            let owned = owned_bytes(&record)?;
            if encode_record(&record)? != owned || super::encode(&record)? != owned {
                return Err(WorkflowError::Verification(
                    "prepared wire bytes changed".to_owned(),
                ));
            }
            let decoded = decode_record(&owned)?;
            if decoded.delta != record.delta
                || decoded.events != record.events
                || decoded.accepted_generation != record.accepted_generation
            {
                return Err(WorkflowError::Verification(
                    "prepared round trip changed".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn owned_bytes(record: &PersistedIndexRun) -> Result<Vec<u8>, WorkflowError> {
    let bytes = match record.schema_version {
        1 => bincode::serialize(&PersistedIndexRunV1 {
            schema_version: 1,
            run_id: record.run_id,
            delta: record.delta.clone(),
            events: record.events.clone(),
            phase: record.phase,
            accepted_generation: record.accepted_generation.clone(),
        }),
        2 => bincode::serialize(&PersistedIndexRunV2 {
            schema_version: 2,
            run_id: record.run_id,
            delta: record.delta.clone(),
            accepted_at: record.accepted_at,
            events: record.events.clone(),
            phase: record.phase,
            accepted_generation: record.accepted_generation.clone(),
        }),
        3 => bincode::serialize(&PersistedIndexRunV3 {
            schema_version: 3,
            run_id: record.run_id,
            delta: record.delta.clone(),
            lineage: record.lineage.clone(),
            accepted_at: record.accepted_at,
            events: record.events.clone(),
            phase: record.phase,
            accepted_generation: record.accepted_generation.clone(),
        }),
        4 => bincode::serialize(&PersistedIndexRunV4 {
            schema_version: 4,
            run_id: record.run_id,
            delta: record.delta.clone(),
            lineage: record.lineage.clone(),
            consequences: record.consequences.clone(),
            accepted_at: record.accepted_at,
            events: record.events.clone(),
            phase: record.phase,
            accepted_generation: record.accepted_generation.clone(),
        }),
        version => {
            return Err(WorkflowError::Verification(format!(
                "unexpected fixture version {version}"
            )));
        }
    };
    bytes.map_err(|error| WorkflowError::Verification(error.to_string()))
}
