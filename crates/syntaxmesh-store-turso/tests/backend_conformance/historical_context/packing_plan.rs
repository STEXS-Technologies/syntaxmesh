//! Distinct plan capacity reuses the temporal source fixture and backend lifecycle.

use super::{
    ContextFixtureCounter, ContextFixtureSource, ContextItemKind, ContextPack, GenerationId,
    GraphStore, NodeId, Query, QueryError, SOURCE, StoreError, request,
};

fn ids() -> Vec<NodeId> {
    (0_u64..40)
        .map(|ordinal| NodeId::derive(&[b"packing-plan-distinct", &ordinal.to_le_bytes()]))
        .collect()
}

pub(super) fn nodes(template: &syntaxmesh_core::Node) -> Vec<syntaxmesh_core::Node> {
    ids()
        .into_iter()
        .enumerate()
        .map(|(ordinal, id)| syntaxmesh_core::Node {
            id,
            name: format!("plan_only_{ordinal}"),
            kind: syntaxmesh_core::NodeKind::Function,
            ..template.clone()
        })
        .collect()
}

pub(super) fn pack(
    store: &dyn GraphStore,
    generation: GenerationId,
    historical: bool,
) -> Result<ContextPack, StoreError> {
    let mut plan = request(true);
    let ordered = ids();
    plan.seed_nodes = ordered.clone();
    if let Some(first) = ordered.first() {
        plan.seed_nodes.push(*first);
    }
    plan.max_candidates = 41;
    plan.max_hops = 0;
    plan.token_budget = 100_000;
    let query = Query::new(store, generation);
    let selection = if historical {
        query.historical_ranked_plan_context_selection(&plan)
    } else {
        query.ranked_plan_context_selection(&plan)
    }
    .map_err(|error| StoreError::Backend(error.to_string()))?;
    if selection.nodes.len() != 40
        || ordered.iter().enumerate().any(|(position, id)| {
            let priority = u32::MAX.saturating_sub(u32::try_from(position).unwrap_or(u32::MAX));
            !selection.nodes.contains(&(*id, 0, priority))
        })
    {
        return Err(StoreError::Integrity(
            "packing plan lost distinct identity or priority".to_owned(),
        ));
    }
    if !matches!(
        query.ranked_seeded_context_selection(&plan),
        Err(QueryError::InvalidContextRequest)
    ) {
        return Err(StoreError::Integrity(
            "old graph seed limit changed".to_owned(),
        ));
    }
    let source = ContextFixtureSource(SOURCE.to_vec());
    let packed = if historical {
        query.historical_ranked_plan_context(&plan, &source, &ContextFixtureCounter)
    } else {
        query.ranked_plan_context(&plan, &source, &ContextFixtureCounter)
    }
    .map_err(|error| StoreError::Backend(error.to_string()))?;
    let serialized =
        serde_json::to_string(&packed).map_err(|error| StoreError::Backend(error.to_string()))?;
    if packed.token_count != u64::try_from(serialized.len()).unwrap_or(u64::MAX)
        || packed.token_count > packed.token_budget
        || ordered.iter().any(|id| {
            !packed.items.iter().any(|item| {
                item.kind == ContextItemKind::SourceEvidence && item.node_ids.contains(id)
            })
        })
    {
        return Err(StoreError::Integrity(
            "packing plan lost source attribution or exact budget".to_owned(),
        ));
    }
    Ok(packed)
}
