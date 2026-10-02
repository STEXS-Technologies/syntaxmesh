use crate::EngineIndexFreshness;
use syntaxmesh_core::{FileId, FileVersion};

fn file(path: &str, hash: u8) -> FileVersion {
    FileVersion {
        file_id: FileId::derive(&[path.as_bytes()]),
        normalized_path: path.to_owned(),
        content_hash: [hash; 32],
        size_bytes: 10,
    }
}

#[test]
fn inventory_comparison_reports_changes_independent_of_order() {
    let unchanged = file("unchanged.rs", 1);
    let before = file("changed.rs", 1);
    let after = file("changed.rs", 2);
    let removed = file("removed.rs", 1);
    let added = file("added.rs", 1);
    let actual = EngineIndexFreshness::compare(
        &[unchanged.clone(), before, removed],
        &[added, after, unchanged],
    );
    assert_eq!(
        actual,
        EngineIndexFreshness {
            unindexed_files: 1,
            changed_files: 1,
            removed_files: 1
        }
    );
    assert!(!actual.is_current());
    assert!(EngineIndexFreshness::compare(&[], &[]).is_current());
}

#[test]
fn full_file_equality_and_last_duplicate_entry_are_preserved() {
    let original = file("a.rs", 1);
    let mut resized = original.clone();
    resized.size_bytes = 11;
    let mut renamed = original.clone();
    renamed.normalized_path = "alias.rs".to_owned();
    for changed in [&resized, &renamed] {
        assert_eq!(
            EngineIndexFreshness::compare(
                std::slice::from_ref(&original),
                std::slice::from_ref(changed)
            )
            .changed_files,
            1
        );
    }
    assert!(
        EngineIndexFreshness::compare(
            &[resized.clone(), original.clone()],
            &[renamed, original.clone()]
        )
        .is_current()
    );
    assert_eq!(
        EngineIndexFreshness::compare(
            std::slice::from_ref(&original),
            &[original.clone(), resized]
        )
        .changed_files,
        1
    );
}
