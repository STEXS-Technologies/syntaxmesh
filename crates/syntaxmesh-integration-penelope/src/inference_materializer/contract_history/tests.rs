use super::*;
use std::path::Path;
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, GenerationId, GraphDelta, IndexRunId, Node, NodeId, NodeKind,
    Provenance, ProvenanceId, RelationKind, RepositoryId, WorktreeId,
};
use syntaxmesh_inference::{ContractOrigin, ContractOutcome, RelationExpectation};
use syntaxmesh_inference_sqlite::InferenceMaterializer;
use syntaxmesh_query::Query;
use syntaxmesh_store::{DurableRecordStore, GraphStore};

fn graph<S: GraphStore>(store: &mut S) -> Result<GenerationId, Box<dyn std::error::Error>> {
    let repository = RepositoryId::derive(&[b"contract-history-repository"]);
    let worktree = WorktreeId::derive(&[b"contract-history-worktree"]);
    let generation = GenerationId::derive(&[b"contract-history-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"contract-history-provenance"]),
        producer_namespace: "test.contract-history".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let source = NodeId::derive(&[b"entry"]);
    let target = NodeId::derive(&[b"writer"]);
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"contract-history-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance.clone()],
        upsert_nodes: [source, target]
            .into_iter()
            .map(|id| Node {
                id,
                kind: NodeKind::Function,
                name: id.0.to_hex(),
                owner_file: None,
                source: None,
                provenance: provenance.id,
                extension_payload: None,
            })
            .collect(),
        upsert_edges: vec![Edge {
            id: EdgeId::derive(&[b"contract-history-edge"]),
            source,
            target,
            relation: RelationKind::Calls,
            provenance: provenance.id,
            extension_payload: None,
        }],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    Ok(generation)
}

fn exercise<S: GraphStore + DurableRecordStore>(
    mut open_store: impl FnMut() -> Result<S, Box<dyn std::error::Error>>,
    database: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = open_store()?;
    let generation = graph(&mut store)?;
    let contract = Contract {
        id: "entry-calls-writer".to_owned(),
        version: "1".to_owned(),
        origin: ContractOrigin::Declared,
        source: NodeId::derive(&[b"entry"]),
        target: NodeId::derive(&[b"writer"]),
        relation: RelationKind::Calls,
        expectation: RelationExpectation::Required,
    };
    let contracts = [contract];
    let limits = ContractLimits {
        contracts: 2,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 2,
    };
    let request = ContractHistoryRequest {
        generation,
        contracts: &contracts,
        limits: &limits,
        history_scan_budget: 100,
        output_bytes: 16_384,
    };
    let expected = Query::new(&store, generation).evaluate_contracts(&contracts, &limits, 100)?;
    let mut coordinator = PenelopeInferenceMaterializer::new(store);
    if coordinator
        .retain_contract_history(database, &request)
        .is_ok()
    {
        return Err("unmigrated downstream contract database was accepted".into());
    }
    store = coordinator.into_store();
    let page = store.records_with_prefix_page(PREFIX, None, 32)?;
    let (key, bytes) = page
        .records
        .iter()
        .find(|(key, _)| key.starts_with(PREFIX))
        .ok_or("missing prepared Penelope contract-history request")?;
    let prepared = journal::decode::<Request>(bytes)?;
    if prepared.receipt.is_some() {
        return Err("failed downstream effect was not left prepared".into());
    }
    let key = key.clone();
    let prepared_request = prepared.request;
    drop(store);

    InferenceMaterializer::migrate(database)?;
    let mut recovered = PenelopeInferenceMaterializer::new(open_store()?);
    if recovered.recover_pending()? != 1 || recovered.recover_pending()? != 0 {
        return Err("Penelope did not recover exactly one contract-history request".into());
    }
    let receipt = recovered.retain_contract_history(database, &request)?;
    let row = InferenceMaterializer::open(database)?
        .contract_result_at(
            receipt.repository,
            receipt.worktree,
            generation,
            "entry-calls-writer",
            "1",
        )?
        .ok_or("recovered contract result missing")?;
    if row.evaluation.outcome != ContractOutcome::Supported
        || expected.first().map(|result| result.id) != Some(row.evaluation.id)
    {
        return Err("recovered contract result differs from pinned Query evaluation".into());
    }

    // Simulate the SQL commit succeeding before the journal completion write.
    store = recovered.into_store();
    let completed = store
        .read_record(&key)?
        .ok_or("missing completed Penelope record")?;
    let prepared_again = journal::encode(&journal::prepare(prepared_request)?)?;
    store.compare_exchange_record(&key, Some(&completed), &prepared_again)?;
    drop(store);
    let mut interrupted = PenelopeInferenceMaterializer::new(open_store()?);
    if interrupted.recover_pending()? != 1
        || interrupted.retain_contract_history(database, &request)? != receipt
    {
        return Err("SQL-before-journal recovery changed the contract result receipt".into());
    }
    Ok(())
}

#[test]
fn file_workflow_recovers_contract_results_before_and_after_sql_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let store_path = directory.path().join("graph.snapshot");
    exercise(
        || Ok(syntaxmesh_store::FileGraphStore::open(&store_path)?),
        &directory.path().join("inference.sqlite"),
    )
}

#[test]
fn turso_workflow_recovers_contract_results_before_and_after_sql_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let store_path = directory.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&store_path)?;
    exercise(
        || Ok(syntaxmesh_store_turso::TursoGraphStore::open(&store_path)?),
        &directory.path().join("inference.sqlite"),
    )
}
