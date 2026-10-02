use super::{NodeReadProfile, ReadStage};
use std::time::{Duration, Instant};

#[test]
fn disabled_regions_do_not_collect_work_or_start_clocks() {
    let mut profile = NodeReadProfile::new();
    profile.started = None;
    assert!(profile.timer().is_none());
    profile.record(ReadStage::SqlPage, None);
    profile.record(ReadStage::DecodeNode, None);
    profile.record(ReadStage::PageTransfer, None);
    profile.record(ReadStage::PageDecode, None);
    assert_eq!(profile.loaded_pages, 0);
    assert_eq!(profile.decoded_nodes, 0);
    assert_eq!(profile.sql, Duration::ZERO);
    assert_eq!(profile.decoding, Duration::ZERO);
    assert_eq!(profile.page_transfer, Duration::ZERO);
    assert_eq!(profile.page_decode, Duration::ZERO);
}

#[test]
fn enabled_regions_count_only_the_corresponding_successful_work() {
    let mut profile = NodeReadProfile::new();
    profile.started = Some(Instant::now());
    for stage in [
        ReadStage::Schema,
        ReadStage::Root,
        ReadStage::SqlPage,
        ReadStage::PageTransfer,
        ReadStage::PageDecode,
        ReadStage::ValidatePage,
        ReadStage::DecodeNode,
    ] {
        profile.record(stage, profile.timer());
    }
    assert_eq!(profile.loaded_pages, 1);
    assert_eq!(profile.decoded_nodes, 1);
}
