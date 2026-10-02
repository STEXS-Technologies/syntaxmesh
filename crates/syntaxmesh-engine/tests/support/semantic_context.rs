use std::cell::Cell;
use std::error::Error;

use syntaxmesh_api_model::{ContextItemKind, ContextRequest};
use syntaxmesh_core::{EvidenceClass, FileVersion, NodeId};
use syntaxmesh_query::{ContextSourceProvider, ContextTokenCounter, Query};
use syntaxmesh_store::GraphStore;

struct Sources {
    stale: bool,
    reads: Cell<usize>,
}

impl ContextSourceProvider for Sources {
    fn read_source(&self, file: &FileVersion) -> Result<Option<Vec<u8>>, String> {
        self.reads.set(self.reads.get().saturating_add(1));
        if self.stale {
            return Ok(Some(b"source changed after publication".to_vec()));
        }
        Ok(match file.normalized_path.as_str() {
            "docs/architecture.md" => Some(b"The engine is runtime agnostic.".to_vec()),
            "docs/decisions.md" => Some(b"Durable workflows use Penelope.".to_vec()),
            _ => None,
        })
    }
}

struct ByteCounter;

impl ContextTokenCounter for ByteCounter {
    fn tokenizer_id(&self) -> &str {
        "semantic-context-test-bytes-v1"
    }

    fn count_tokens(&self, payload: &str) -> Result<u64, String> {
        u64::try_from(payload.len()).map_err(|error| error.to_string())
    }
}

pub(super) fn verify_joint_sources<S: GraphStore>(
    query: &Query<'_, S>,
    claim: NodeId,
) -> Result<(), Box<dyn Error>> {
    for seeds in [vec![claim], Vec::new()] {
        let request = ContextRequest {
            query: "runtime agnostic engine".to_owned(),
            seed_nodes: seeds,
            token_budget: 50_000,
            max_hops: 2,
            max_candidates: 32,
        };
        let source = Sources {
            stale: false,
            reads: Cell::new(0),
        };
        let pack = query.context(&request, &source, &ByteCounter)?;
        let snapshot = query.graph_at(query.generation())?;
        for item in &pack.items {
            let origin = item
                .edge_ids
                .first()
                .and_then(|id| snapshot.edges.iter().find(|edge| edge.id == *id))
                .map(|edge| edge.provenance)
                .or_else(|| {
                    item.node_ids
                        .first()
                        .and_then(|id| snapshot.nodes.iter().find(|node| node.id == *id))
                        .map(|node| node.provenance)
                })
                .ok_or_else(|| std::io::Error::other("context item lost origin"))?;
            let expected = snapshot
                .provenance
                .iter()
                .find(|record| record.id == origin)
                .map(|record| record.evidence_class);
            if item.evidence_class != expected || expected.is_none() {
                return Err(
                    std::io::Error::other("context changed original fact evidence class").into(),
                );
            }
        }
        for class in [EvidenceClass::SourceFact, EvidenceClass::SemanticInference] {
            if !pack
                .items
                .iter()
                .any(|item| item.evidence_class == Some(class))
            {
                return Err(std::io::Error::other(
                    "joint context lost source/inference distinction",
                )
                .into());
            }
        }
        for (path, text) in [
            ("docs/architecture.md", "The engine is runtime agnostic."),
            ("docs/decisions.md", "Durable workflows use Penelope."),
        ] {
            if !pack.items.iter().any(|item| {
                item.kind == ContextItemKind::SourceEvidence
                    && item.source_path.as_deref() == Some(path)
                    && item.text.contains(text)
                    && item.line_start == Some(1)
                    && item.line_end == Some(1)
            }) {
                return Err(
                    std::io::Error::other("joint context lost original document evidence").into(),
                );
            }
        }
        if pack.generation != query.generation()
            || source.reads.get() != 2
            || pack.token_count != ByteCounter.count_tokens(&serde_json::to_string(&pack)?)?
            || pack.token_count > request.token_budget
            || !pack.items.iter().any(|item| {
                item.kind == ContextItemKind::GraphPath && item.node_ids.contains(&claim)
            })
        {
            return Err(std::io::Error::other(
                "joint context lost generation, path, source reuse, or budget fidelity",
            )
            .into());
        }
        let stale = Sources {
            stale: true,
            reads: Cell::new(0),
        };
        let stale_pack = query.context(&request, &stale, &ByteCounter)?;
        if stale.reads.get() != 2
            || stale_pack
                .items
                .iter()
                .any(|item| item.kind == ContextItemKind::SourceEvidence)
            || !stale_pack
                .warnings
                .iter()
                .any(|warning| warning.code == "stale_source")
        {
            return Err(std::io::Error::other("joint context accepted stale source bytes").into());
        }
    }
    Ok(())
}
