use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(super) enum ReadStage {
    Schema,
    Root,
    SqlPage,
    PageTransfer,
    PageDecode,
    ValidatePage,
    DecodeNode,
}

pub(super) struct NodeReadProfile {
    started: Option<Instant>,
    schema: Duration,
    root: Duration,
    sql: Duration,
    page_transfer: Duration,
    page_decode: Duration,
    validation: Duration,
    decoding: Duration,
    loaded_pages: usize,
    decoded_nodes: usize,
}

impl NodeReadProfile {
    pub(super) fn new() -> Self {
        Self {
            started: std::env::var_os("SYNTAXMESH_TURSO_NODE_READ_PROFILE").map(|_| Instant::now()),
            schema: Duration::ZERO,
            root: Duration::ZERO,
            sql: Duration::ZERO,
            page_transfer: Duration::ZERO,
            page_decode: Duration::ZERO,
            validation: Duration::ZERO,
            decoding: Duration::ZERO,
            loaded_pages: 0,
            decoded_nodes: 0,
        }
    }

    pub(super) fn timer(&self) -> Option<Instant> {
        self.started.map(|_| Instant::now())
    }

    pub(super) fn record(&mut self, stage: ReadStage, started: Option<Instant>) {
        let Some(started) = started else {
            return;
        };
        let elapsed = started.elapsed();
        let region = match stage {
            ReadStage::Schema => &mut self.schema,
            ReadStage::Root => &mut self.root,
            ReadStage::SqlPage => {
                self.loaded_pages = self.loaded_pages.saturating_add(1);
                &mut self.sql
            }
            ReadStage::ValidatePage => &mut self.validation,
            ReadStage::PageTransfer => &mut self.page_transfer,
            ReadStage::PageDecode => &mut self.page_decode,
            ReadStage::DecodeNode => {
                self.decoded_nodes = self.decoded_nodes.saturating_add(1);
                &mut self.decoding
            }
        };
        *region = region.saturating_add(elapsed);
    }

    pub(super) fn finish(&self, matches: usize) {
        if let Some(started) = self.started {
            eprintln!(
                "context_node_read matches={matches} loaded_pages={} decoded_nodes={} total_us={} schema_us={} root_us={} sql_page_us={} validate_page_us={} decode_node_us={} page_transfer_us={} page_decode_us={}",
                self.loaded_pages,
                self.decoded_nodes,
                started.elapsed().as_micros(),
                self.schema.as_micros(),
                self.root.as_micros(),
                self.sql.as_micros(),
                self.validation.as_micros(),
                self.decoding.as_micros(),
                self.page_transfer.as_micros(),
                self.page_decode.as_micros(),
            );
        }
    }
}
