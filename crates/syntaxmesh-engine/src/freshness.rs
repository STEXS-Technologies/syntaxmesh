use std::collections::BTreeMap;

use syntaxmesh_core::{FileId, FileVersion};

use crate::EngineIndexFreshness;

#[cfg(test)]
mod tests;

impl EngineIndexFreshness {
    /// Compare indexed files with a caller-supplied inventory from the same scope.
    /// This does not scan sources or validate scope; duplicate IDs retain the
    /// last entry in each inventory, matching the Engine's existing behavior.
    #[must_use]
    pub fn compare(indexed: &[FileVersion], source: &[FileVersion]) -> Self {
        let indexed_by_id: BTreeMap<FileId, &FileVersion> =
            indexed.iter().map(|file| (file.file_id, file)).collect();
        let source_by_id: BTreeMap<FileId, &FileVersion> =
            source.iter().map(|file| (file.file_id, file)).collect();
        let mut unindexed_files = 0_usize;
        let mut changed_files = 0_usize;
        for (id, source_file) in &source_by_id {
            match indexed_by_id.get(id) {
                None => unindexed_files = unindexed_files.saturating_add(1),
                Some(indexed_file) if *indexed_file != *source_file => {
                    changed_files = changed_files.saturating_add(1);
                }
                Some(_) => {}
            }
        }
        let removed_files = indexed_by_id
            .keys()
            .filter(|id| !source_by_id.contains_key(*id))
            .count();
        Self {
            unindexed_files,
            changed_files,
            removed_files,
        }
    }
}
