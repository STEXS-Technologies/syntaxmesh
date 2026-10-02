//! Opt-in host composition; canonical selection and packing stay in Query.

use super::{SyntaxMeshMcp, planner_cache::PlannerCache};
use std::collections::BTreeSet;
use std::sync::Mutex;
use syntaxmesh_api_model::{ContextPack, ContextRequest};
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_query::{
    ContextSourceProvider, ContextTokenCounter, DiscoveryPackingPreference, LexicalPlanRequest,
    Query, SourceRolePackingRequest, SourceRolePreference,
};
use syntaxmesh_store_turso::TursoGraphStore;

#[derive(Clone, Copy)]
pub(super) struct Policy {
    pub(super) discovery: DiscoveryPackingPreference,
    pub(super) preference: SourceRolePreference,
    pub(super) source_content: bool,
}

impl<E: LanguageExtractor> SyntaxMeshMcp<E> {
    /// Enable indexed context with explicit discovery packing order.
    /// Explicit seed requests retain their ordinary path. This changes only
    /// packing, so the generation-pinned candidate index remains reusable.
    #[must_use]
    pub const fn with_indexed_context_discovery_preference(
        mut self,
        preference: DiscoveryPackingPreference,
    ) -> Self {
        self.indexed_context = true;
        self.discovery_preference = preference;
        self
    }

    /// Enable indexed source-content selection without processing metadata seeds.
    /// Explicit seeds retain the ordinary path. Policy changes get a separate cache;
    /// clones of this configured host share only its own policy's derived index.
    #[must_use]
    pub fn with_source_content_context(mut self) -> Self {
        self.indexed_context = true;
        self.source_content_context = true;
        self.planner_cache = std::sync::Arc::new(Mutex::new(PlannerCache::new()));
        self
    }

    /// Opt in to indexed context for requests without explicit seed IDs.
    /// Cold builds are bounded but expensive; clones share one retained index.
    #[must_use]
    pub const fn with_indexed_context(mut self) -> Self {
        self.indexed_context = true;
        self.source_role_preference = SourceRolePreference::Neutral;
        self.discovery_preference = DiscoveryPackingPreference::Balanced;
        self
    }

    /// Enable indexed context with explicit role packing order, without filtering.
    /// Requests with explicit seed IDs retain their ordinary context path.
    #[must_use]
    pub const fn with_indexed_context_role_preference(
        mut self,
        preference: SourceRolePreference,
    ) -> Self {
        self.indexed_context = true;
        self.source_role_preference = preference;
        self
    }
}

pub(super) fn compile(
    query: &Query<'_, TursoGraphStore>,
    request: &ContextRequest,
    historical: bool,
    source: &impl ContextSourceProvider,
    counter: &impl ContextTokenCounter,
    cache: &Mutex<PlannerCache>,
    policy: Policy,
) -> Result<ContextPack, String> {
    let manifest = query.manifest().map_err(|error| error.to_string())?;
    let index = cache
        .lock()
        .map_err(|error| format!("planner cache lock is poisoned: {error}"))?
        .get_or_build(&manifest, || {
            if policy.source_content {
                return query
                    .build_source_content_planner_index(1_000_000, 8_000_000, 256 * 1024 * 1024)
                    .map_err(|error| error.to_string());
            }
            query
                .build_planner_identifier_index(1_000_000, 8_000_000, 256 * 1024 * 1024)
                .map_err(|error| error.to_string())
        })?;
    let lexical = LexicalPlanRequest {
        query: &request.query,
        posting_budget: 8_000_000,
    };
    let mut seeds = query
        .lexical_seed_plan(&index, &lexical)
        .map_err(|error| error.to_string())?;
    seeds.truncate(usize::from(request.max_candidates));
    if seeds.is_empty() {
        let empty = ContextRequest {
            seed_nodes: seeds,
            max_hops: 0,
            ..request.clone()
        };
        return if historical {
            query.historical_ranked_plan_context(&empty, source, counter)
        } else {
            query.ranked_plan_context(&empty, source, counter)
        }
        .map_err(|error| error.to_string());
    }
    let initial = ContextRequest {
        seed_nodes: seeds,
        ..request.clone()
    };
    let selected = if historical {
        query.historical_ranked_seeded_context_selection(&initial)
    } else {
        query.ranked_seeded_context_selection(&initial)
    }
    .map_err(|error| error.to_string())?;
    let eligible = selected
        .nodes
        .iter()
        .map(|entry| entry.0)
        .collect::<BTreeSet<_>>();
    let original = initial
        .seed_nodes
        .iter()
        .filter(|id| eligible.contains(id))
        .copied()
        .collect();
    let plan = ContextRequest {
        seed_nodes: query
            .lexical_packing_plan_with_discovery(
                &index,
                &lexical,
                &SourceRolePackingRequest {
                    selected: &eligible,
                    original: &original,
                    preference: policy.preference,
                },
                policy.discovery,
            )
            .map_err(|error| error.to_string())?,
        max_hops: 0,
        ..request.clone()
    };
    if historical {
        query.historical_ranked_plan_context(&plan, source, counter)
    } else {
        query.ranked_plan_context(&plan, source, counter)
    }
    .map_err(|error| error.to_string())
}
