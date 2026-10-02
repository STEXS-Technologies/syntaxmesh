use std::cell::{Cell, RefCell};

use syntaxmesh_core::{FileId, FileVersion};

use super::{ContextSourceProvider, MAX_CACHED_BYTES, SourceCache};

struct Provider {
    calls: Cell<usize>,
    response: RefCell<Result<Option<Vec<u8>>, String>>,
}

impl ContextSourceProvider for Provider {
    fn read_source(&self, _file: &FileVersion) -> Result<Option<Vec<u8>>, String> {
        self.calls.set(self.calls.get().saturating_add(1));
        self.response.borrow().clone()
    }
}

fn file(bytes: &[u8]) -> FileVersion {
    FileVersion {
        file_id: FileId::derive(&[b"source-cache-fixture"]),
        normalized_path: "docs/architecture.md".to_owned(),
        content_hash: *blake3::hash(bytes).as_bytes(),
        size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    }
}

#[test]
fn successful_text_is_shared_only_within_matching_query_identity() {
    let provider = Provider {
        calls: Cell::new(0),
        response: RefCell::new(Ok(Some(b"old".to_vec()))),
    };
    let old = file(b"old");
    let mut cache = SourceCache::new(&provider);
    assert_eq!(
        cache
            .read(&old)
            .map(|text| text.to_string())
            .map_err(|failure| failure.code),
        Ok("old".to_owned())
    );
    assert!(provider.response.replace(Ok(Some(b"new".to_vec()))).is_ok());
    assert!(cache.read(&old).is_ok());
    assert_eq!(provider.calls.get(), 1);
    assert!(cache.read(&file(b"new")).is_ok());
    assert_eq!(provider.calls.get(), 2);
    let mut fresh = SourceCache::new(&provider);
    assert_eq!(
        fresh.read(&old).map_err(|failure| failure.code),
        Err("stale_source")
    );
    assert_eq!(provider.calls.get(), 3);
}

#[test]
fn failures_are_reused_but_retried_in_the_next_query() {
    for (response, code) in [
        (Ok(None), "source_unavailable"),
        (Err("fixture failed".to_owned()), "source_provider_error"),
        (Ok(Some(b"wrong".to_vec())), "stale_source"),
        (Ok(Some(vec![255])), "invalid_utf8_source"),
    ] {
        let provider = Provider {
            calls: Cell::new(0),
            response: RefCell::new(response),
        };
        let accepted = if code == "invalid_utf8_source" {
            file(&[255])
        } else {
            file(b"accepted")
        };
        let mut cache = SourceCache::new(&provider);
        for _ in 0..3 {
            assert_eq!(
                cache.read(&accepted).map_err(|failure| failure.code),
                Err(code)
            );
        }
        assert_eq!(provider.calls.get(), 1);
        assert!(SourceCache::new(&provider).read(&accepted).is_err());
        assert_eq!(provider.calls.get(), 2);
    }
}

#[test]
fn oversized_success_is_verified_but_not_retained() {
    let bytes = vec![b'x'; MAX_CACHED_BYTES.saturating_add(1)];
    let accepted = file(&bytes);
    let provider = Provider {
        calls: Cell::new(0),
        response: RefCell::new(Ok(Some(bytes))),
    };
    let mut cache = SourceCache::new(&provider);
    assert!(cache.read(&accepted).is_ok());
    assert!(cache.read(&accepted).is_ok());
    assert_eq!(provider.calls.get(), 2);
    assert_eq!(cache.retained_bytes, 0);
    assert!(cache.entries.is_empty());
}

#[test]
fn retention_limit_applies_to_total_text_across_files() {
    let half = MAX_CACHED_BYTES / 2;
    let first_bytes = vec![b'a'; half];
    let second_bytes = vec![b'b'; half];
    let first = file(&first_bytes);
    let second = file(&second_bytes);
    let third = file(b"uncached");
    let provider = Provider {
        calls: Cell::new(0),
        response: RefCell::new(Ok(Some(first_bytes))),
    };
    let mut cache = SourceCache::new(&provider);
    assert!(cache.read(&first).is_ok());
    assert!(provider.response.replace(Ok(Some(second_bytes))).is_ok());
    assert!(cache.read(&second).is_ok());
    assert_eq!(cache.retained_bytes, MAX_CACHED_BYTES);
    assert!(
        provider
            .response
            .replace(Ok(Some(b"uncached".to_vec())))
            .is_ok()
    );
    assert!(cache.read(&third).is_ok());
    assert!(cache.read(&third).is_ok());
    assert_eq!(provider.calls.get(), 4);
    assert!(cache.read(&first).is_ok());
    assert!(cache.read(&second).is_ok());
    assert_eq!(provider.calls.get(), 4);
    assert_eq!(cache.retained_bytes, MAX_CACHED_BYTES);
    assert_eq!(cache.entries.len(), 2);
}
