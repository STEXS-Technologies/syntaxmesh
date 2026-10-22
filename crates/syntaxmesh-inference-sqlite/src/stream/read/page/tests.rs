use super::*;
use crate::{RebuildRequest, StreamSyncRequest, test_fixture};
use syntaxmesh_inference::InvalidationLimits;
use syntaxmesh_store::InMemoryGraphStore;

#[test]
fn pages_match_complete_witnesses_and_keep_historical_cursors_after_reopen() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("pages.sqlite");
    InferenceMaterializer::migrate(&path)?;
    let mut materializer = InferenceMaterializer::open(&path)?;
    let mut store = InMemoryGraphStore::new();
    let (rule, declared, retracted) = test_fixture::fixture(&mut store)?;
    let rules = (0..16)
        .map(|index| {
            let mut distinct = rule.clone();
            distinct.id = format!("page-rule-{index}");
            distinct
        })
        .collect::<Vec<_>>();
    let mut limits = test_fixture::limits();
    limits.claims = 100;
    limits.edge_visits = 10_000;
    let baseline = materializer.rebuild(
        &store,
        &RebuildRequest {
            generation: declared,
            rules: &rules,
            limits: &limits,
            history_scan_budget: 100,
            rule_budget: 16,
            output_bytes: 100_000,
            premise_budget: 100,
        },
    )?;
    let origin = materializer.begin_stream(baseline, 100, 100_000, 100, 4096)?;
    let complete = materializer.stream_claims(origin, declared, 100, 100_000, 100, 4096)?;
    if complete.claims.len() != 16 {
        return Err("page fixture must contain sixteen distinct witnesses".into());
    }
    let initial = StreamPageRequest {
        origin,
        generation: declared,
        cursor: None,
        claim_budget: 3,
        byte_budget: 100_000,
        premise_budget: 100,
        page_budget: 4096,
    };
    let first = materializer.stream_claims_page(&initial)?;
    let cursor = first.next.ok_or("first page omitted continuation")?;
    let exact_bytes = first
        .claims
        .iter()
        .try_fold(0_usize, |total, claim| -> Result<usize> {
            total
                .checked_add(usize::try_from(bincode::serialized_size(claim)?)?)
                .ok_or_else(|| "page byte total overflow".into())
        })?;
    let exact_premises = first
        .claims
        .iter()
        .map(|claim| claim.premises.len())
        .sum::<usize>();
    if materializer.stream_claims_page(&StreamPageRequest {
        byte_budget: exact_bytes,
        premise_budget: exact_premises,
        ..initial
    })? != first
        || materializer
            .stream_claims_page(&StreamPageRequest {
                byte_budget: exact_bytes.checked_sub(1).ok_or("empty page")?,
                ..initial
            })
            .is_ok()
        || materializer
            .stream_claims_page(&StreamPageRequest {
                premise_budget: exact_premises.checked_sub(1).ok_or("empty postings")?,
                ..initial
            })
            .is_ok()
    {
        return Err("page failed exact byte/posting admission".into());
    }
    for size in [1, 3, 4, 16, 17] {
        let mut request = StreamPageRequest {
            claim_budget: size,
            ..initial
        };
        let mut collected = Vec::new();
        loop {
            let page = materializer.stream_claims_page(&request)?;
            if page.checkpoint != complete.checkpoint || page.claims.len() > size {
                return Err("page selection or output bound inconsistent".into());
            }
            collected.extend(page.claims);
            if collected.len() > complete.claims.len() {
                return Err("pagination repeated witnesses".into());
            }
            request.cursor = page.next;
            if request.cursor.is_none() {
                break;
            }
        }
        if collected != complete.claims {
            return Err("paged witnesses differ from complete selection".into());
        }
    }
    for invalid in [
        StreamPageRequest {
            claim_budget: 0,
            ..initial
        },
        StreamPageRequest {
            byte_budget: 1,
            ..initial
        },
        StreamPageRequest {
            premise_budget: 1,
            ..initial
        },
        StreamPageRequest {
            page_budget: 1,
            ..initial
        },
        StreamPageRequest {
            cursor: Some(StreamCursor {
                stream: StableId([0; 32]),
                ..cursor
            }),
            ..initial
        },
        StreamPageRequest {
            cursor: Some(StreamCursor {
                root: None,
                ..cursor
            }),
            ..initial
        },
        StreamPageRequest {
            cursor: Some(StreamCursor {
                after: StableId([0; 32]),
                ..cursor
            }),
            ..initial
        },
        StreamPageRequest {
            cursor: Some(StreamCursor {
                selection: ClaimSelection {
                    generation: retracted,
                    ..cursor.selection
                },
                ..cursor
            }),
            ..initial
        },
    ] {
        if materializer.stream_claims_page(&invalid).is_ok() {
            return Err("page admitted exhausted bounds or substituted cursor".into());
        }
    }
    let continued = StreamPageRequest {
        cursor: Some(cursor),
        ..initial
    };
    let second = materializer.stream_claims_page(&continued)?;
    materializer.sync_stream(
        &store,
        &StreamSyncRequest {
            origin,
            through: retracted,
            rules: &rules,
            limits: &limits,
            range_limits: &InvalidationLimits {
                generations: 1,
                versions: 100,
                postings: 100,
                claims: 100,
            },
            history_scan_budget: 100,
            rule_budget: 16,
            output_bytes: 100_000,
            premise_budget: 100,
            evidence_budget: 100,
            matched_claim_bytes: 100_000,
            page_budget: 4096,
        },
    )?;
    if materializer.stream_claims_page(&continued)? != second
        || materializer
            .stream_claims_page(&StreamPageRequest {
                generation: retracted,
                cursor: Some(cursor),
                ..initial
            })
            .is_ok()
    {
        return Err(
            "later head changed historical cursor or accepted cross-generation cursor".into(),
        );
    }
    let empty = materializer.stream_claims_page(&StreamPageRequest {
        generation: retracted,
        ..initial
    })?;
    if !empty.claims.is_empty() || empty.next.is_some() {
        return Err("empty root produced claims or continuation".into());
    }
    for index in [0, 6] {
        let identity = complete
            .claims
            .get(index)
            .ok_or("missing nonselected payload")?
            .id;
        materializer.connection.execute(
            "UPDATE inference_stream_claims SET payload = X'00' WHERE stream = ?1 AND claim = ?2",
            params![origin.stream.to_hex(), identity.to_hex()],
        )?;
    }
    if materializer.stream_claims_page(&continued)? != second
        || materializer.stream_claims_page(&initial).is_ok()
    {
        return Err("page read prior/lookahead payloads or ignored selected corruption".into());
    }
    drop(store);
    drop(materializer);
    let reopened = InferenceMaterializer::open(&path)?;
    if reopened.stream_claims_page(&continued)? != second {
        return Err("historical page required canonical replay after reopen".into());
    }
    Ok(())
}
