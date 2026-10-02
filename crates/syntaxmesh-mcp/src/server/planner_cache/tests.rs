use super::PlannerCache;
use std::error::Error;
use std::sync::Arc;

#[test]
fn identity_changes_evict_and_failed_builds_remain_retryable() -> Result<(), Box<dyn Error>> {
    let (_fixture, server, _) = super::super::tests::indexed_server()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(server.with_query(|query| {
        let manifest = query.manifest().map_err(|error| error.to_string())?;
        let mut cache = PlannerCache::new();
        if cache
            .get_or_build(&manifest, || {
                query
                    .build_planner_identifier_index(0, 1000, 100_000)
                    .map_err(|error| error.to_string())
            })
            .is_ok()
            || cache.entry.is_some()
        {
            return Err("cache bypassed index budget or retained failed build".to_owned());
        }
        let build = || {
            query
                .build_planner_identifier_index(100, 1000, 100_000)
                .map_err(|error| error.to_string())
        };
        let first = cache.get_or_build(&manifest, build)?;
        let hit = cache.get_or_build(&manifest, || Err("hit rebuilt index".to_owned()))?;
        if !Arc::ptr_eq(&first, &hit) {
            return Err("hit did not reuse completed index".to_owned());
        }
        let mut changed = manifest.clone();
        changed.graph_root = [9; 32];
        if cache
            .get_or_build(&changed, || Err("injected build failure".to_owned()))
            .is_ok()
            || cache.entry.is_some()
        {
            return Err("failed identity replacement retained an entry".to_owned());
        }
        let retry = cache.get_or_build(&manifest, build)?;
        if Arc::ptr_eq(&first, &retry) {
            return Err("failed replacement reused old entry".to_owned());
        }
        for field in 0..3 {
            let mut other = manifest.clone();
            match field {
                0 => other.repository = syntaxmesh_core::RepositoryId::derive(&[b"other"]),
                1 => other.worktree = syntaxmesh_core::WorktreeId::derive(&[b"other"]),
                _ => other.generation = syntaxmesh_core::GenerationId::derive(&[b"other"]),
            }
            if cache
                .get_or_build(&other, || Err("foreign identity must rebuild".to_owned()))
                .is_ok()
            {
                return Err("foreign identity reused cached entry".to_owned());
            }
            let _index = cache.get_or_build(&manifest, build)?;
        }
        Ok(())
    }))?;
    Ok(())
}
