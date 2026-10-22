use super::ActiveTree;
use crate::Result;
use rusqlite::{Connection, params};
use syntaxmesh_core::StableId;

fn identity(text: &str) -> Result<StableId> {
    Ok(crate::stream::checkpoint::generation(text)?.0)
}

pub(crate) fn backfill(connection: &Connection) -> Result<()> {
    let mut streams = connection.prepare("SELECT stream, head_generation, head_sequence, origin_operation FROM inference_streams ORDER BY stream")?;
    let mut stream_rows = streams.query([])?;
    while let Some(stream_row) = stream_rows.next()? {
        let stream: String = stream_row.get(0)?;
        let head: String = stream_row.get(1)?;
        let head_sequence: i64 = stream_row.get(2)?;
        let operation: String = stream_row.get(3)?;
        let (origin, origin_sequence, origin_count): (String, i64, i64) = connection.query_row(
            "SELECT generation, generation_sequence, claim_count FROM inference_operations WHERE operation = ?1",
            [&operation], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if identity(&stream)?
            != StableId::derive("inference-stream-v1", &[&identity(&operation)?.0])
        {
            return Err("root migration stream identity mismatch".into());
        }
        let mut checkpoints = connection.prepare("SELECT generation, sequence, previous_generation, added_claims, affected_claims, active_claims FROM inference_stream_checkpoints WHERE stream = ?1 ORDER BY sequence")?;
        let mut checkpoint_rows = checkpoints.query([&stream])?;
        let mut root = None;
        let mut previous: Option<(String, i64)> = None;
        let mut active = 0_usize;
        while let Some(row) = checkpoint_rows.next()? {
            let generation: String = row.get(0)?;
            let sequence: i64 = row.get(1)?;
            let parent: Option<String> = row.get(2)?;
            if let Some((prior, prior_sequence)) = &previous {
                if parent.as_ref() != Some(prior) || prior_sequence.checked_add(1) != Some(sequence)
                {
                    return Err("root migration checkpoint sequence gap".into());
                }
            } else if generation != origin || sequence != origin_sequence || parent.is_some() {
                return Err("root migration baseline coordinate mismatch".into());
            }
            let expected_added = usize::try_from(row.get::<_, i64>(3)?)?;
            let expected_closed = usize::try_from(row.get::<_, i64>(4)?)?;
            let expected_active = usize::try_from(row.get::<_, i64>(5)?)?;
            let mut closed = connection.prepare("SELECT claim, closed_generation FROM inference_stream_claims WHERE stream = ?1 AND closed_sequence = ?2 ORDER BY claim")?;
            let mut closing_rows = closed.query(params![stream, sequence])?;
            let mut closed_count = 0_usize;
            while let Some(closing) = closing_rows.next()? {
                if closing.get_ref(1)?.as_str()? != generation {
                    return Err("root migration closure coordinate mismatch".into());
                }
                root = ActiveTree::new(connection, 4096)?
                    .remove(root, identity(closing.get_ref(0)?.as_str()?)?)?;
                active = active
                    .checked_sub(1)
                    .ok_or("root migration active count underflow")?;
                closed_count = closed_count
                    .checked_add(1)
                    .ok_or("root migration closure count overflow")?;
            }
            let mut born = connection.prepare("SELECT claim, born_generation FROM inference_stream_claims WHERE stream = ?1 AND born_sequence = ?2 ORDER BY claim")?;
            let mut birth_rows = born.query(params![stream, sequence])?;
            let mut added_count = 0_usize;
            while let Some(birth) = birth_rows.next()? {
                if birth.get_ref(1)?.as_str()? != generation {
                    return Err("root migration birth coordinate mismatch".into());
                }
                root = Some(
                    ActiveTree::new(connection, 4096)?
                        .insert(root, identity(birth.get_ref(0)?.as_str()?)?)?,
                );
                active = active
                    .checked_add(1)
                    .ok_or("root migration active count overflow")?;
                added_count = added_count
                    .checked_add(1)
                    .ok_or("root migration birth count overflow")?;
            }
            if added_count != expected_added
                || closed_count != expected_closed
                || active != expected_active
                || (previous.is_none()
                    && (active != usize::try_from(origin_count)? || closed_count != 0))
            {
                return Err(
                    "root migration checkpoint counts differ from retained intervals".into(),
                );
            }
            connection.execute(
                "INSERT INTO inference_stream_roots VALUES (?1, ?2, ?3)",
                params![stream, generation, root.map(StableId::to_hex)],
            )?;
            previous = Some((generation, sequence));
        }
        if previous.as_ref() != Some(&(head, head_sequence)) {
            return Err("root migration head checkpoint missing".into());
        }
    }
    Ok(())
}
