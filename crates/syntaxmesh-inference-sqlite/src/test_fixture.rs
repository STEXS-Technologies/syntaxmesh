use syntaxmesh_core::GenerationId;
use syntaxmesh_inference::{Limits, Rule};
use syntaxmesh_query::Query;
use syntaxmesh_store::GraphStore;

#[path = "../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
mod shared;

pub(crate) fn fixture<S: GraphStore>(
    store: &mut S,
) -> crate::Result<(Rule, GenerationId, GenerationId)> {
    shared::fixture(store)
}

pub(crate) const fn limits() -> Limits {
    shared::limits()
}

pub(crate) fn verify<S: GraphStore>(
    store: &S,
    rule: &Rule,
    retained: GenerationId,
    current: GenerationId,
) -> crate::Result<()> {
    shared::verify(store, rule, retained, current)
}

pub(crate) fn verify_restoration<S: GraphStore>(
    store: &mut S,
    rule: &Rule,
    retained: GenerationId,
    current: GenerationId,
) -> crate::Result<()> {
    shared::verify_restoration(store, rule, retained, current)
}
