use syntaxmesh_api_model::{
    CONTEXT_PACK_SCHEMA_VERSION, ContextItem, ContextItemKind, ContextPack, OmittedContextSummary,
};
use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};

use super::{MAX_MEMO_BYTES, Memo};

#[test]
fn memo_matches_full_encoding_for_both_supported_encodings()
-> Result<(), Box<dyn std::error::Error>> {
    for encoding in [tiktoken_rs::cl100k_base()?, tiktoken_rs::o200k_base()?] {
        let mut memo = Memo::default();
        for query in [
            "architecture",
            "λ 中文 🦀",
            "escaped ,\"token_count\":999 and \\ paths",
            "<|endoftext|> code fn f() {}\n",
        ] {
            let mut pack = ContextPack {
                schema_version: CONTEXT_PACK_SCHEMA_VERSION,
                query: query.to_owned(),
                repository: RepositoryId::derive(&[b"token-memo"]),
                worktree: WorktreeId::derive(&[b"token-memo"]),
                generation: GenerationId::derive(&[b"token-memo"]),
                tokenizer: "fixture".to_owned(),
                token_budget: 8192,
                token_count: 0,
                items: vec![ContextItem {
                    rank: 1,
                    evidence_class: Some(syntaxmesh_core::EvidenceClass::SemanticInference),
                    kind: ContextItemKind::SourceEvidence,
                    text: query.repeat(32),
                    node_ids: Vec::new(),
                    edge_ids: Vec::new(),
                    source_path: Some("docs/design.md".to_owned()),
                    line_start: Some(1),
                    line_end: Some(2),
                }],
                warnings: Vec::new(),
                omitted: OmittedContextSummary::default(),
            };
            for count in [0, 1, 9, 99, 999, 1000, 999_999, 1_000_000, u64::MAX] {
                pack.token_count = count;
                let payload = serde_json::to_string(&pack)?;
                if memo.count(&encoding, &payload) != encoding.encode_ordinary(&payload).len()
                    || memo.entries.is_empty()
                {
                    return Err(std::io::Error::other(
                        "exact token memo differs from full encoding",
                    )
                    .into());
                }
            }
            let pretty = serde_json::to_string_pretty(&pack)?;
            if memo.count(&encoding, &pretty) != encoding.encode_ordinary(&pretty).len() {
                return Err(std::io::Error::other("noncanonical fallback count differs").into());
            }
            let template = pack
                .items
                .first()
                .cloned()
                .ok_or_else(|| std::io::Error::other("missing item fixture"))?;
            for width in [0_usize, 1, 4, 16, 3, 0] {
                pack.items = (0..width)
                    .map(|position| {
                        let mut item = template.clone();
                        item.rank = u32::try_from(position.saturating_add(999)).unwrap_or(u32::MAX);
                        item.line_start =
                            Some(u32::try_from(position.saturating_add(999)).unwrap_or(u32::MAX));
                        item.line_end = if position % 2 == 0 {
                            None
                        } else {
                            Some(u32::MAX)
                        };
                        item.text.push_str("\n spaces   ١٢٣ １２３ ,\"rank\":77 ");
                        item
                    })
                    .collect();
                let payload = serde_json::to_string(&pack)?;
                if memo.count(&encoding, &payload) != encoding.encode_ordinary(&payload).len() {
                    return Err(std::io::Error::other(
                        "segment memo differs on changing item list",
                    )
                    .into());
                }
                if memo.retained_bytes > MAX_MEMO_BYTES || memo.entries.len() > 1024 {
                    return Err(
                        std::io::Error::other("segment memo exceeded retention bounds").into(),
                    );
                }
            }
        }
        for payload in [
            "not JSON".to_owned(),
            "{\"token_count\":123}".to_owned(),
            "x".repeat(MAX_MEMO_BYTES.saturating_add(1)),
        ] {
            if memo.count(&encoding, &payload) != encoding.encode_ordinary(&payload).len() {
                return Err(std::io::Error::other(
                    "fallback token count differs from full encoding",
                )
                .into());
            }
        }
    }
    Ok(())
}
