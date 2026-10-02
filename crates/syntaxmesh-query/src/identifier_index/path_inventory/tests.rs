use super::validate_page;
use syntaxmesh_core::{FileId, FileVersion};
use syntaxmesh_store::HistoricalFilePage;

fn file(id: FileId) -> FileVersion {
    FileVersion {
        file_id: id,
        normalized_path: "scripts/load.py".to_owned(),
        content_hash: [1; 32],
        size_bytes: 1,
    }
}

#[test]
fn inventory_pages_reject_duplicate_descending_and_nonadvancing_ids() {
    let first = FileId::derive(&[b"inventory-first"]);
    let second = FileId::derive(&[b"inventory-second"]);
    let (low, high) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    for ids in [[low, low], [high, low]] {
        let page = HistoricalFilePage {
            items: ids.into_iter().map(file).collect(),
            has_more: false,
        };
        assert!(validate_page(&page, None, 2).is_err());
    }
    let page = HistoricalFilePage {
        items: vec![file(low), file(high)],
        has_more: false,
    };
    assert!(validate_page(&page, Some(low), 2).is_err());
    assert!(validate_page(&page, None, 2).is_ok());
    assert!(validate_page(&page, None, 1).is_err());
}

#[test]
fn empty_inventory_is_valid_only_when_terminal() {
    let mut page = HistoricalFilePage {
        items: Vec::new(),
        has_more: false,
    };
    assert!(validate_page(&page, None, 1).is_ok());
    page.has_more = true;
    assert!(validate_page(&page, None, 1).is_err());
}
