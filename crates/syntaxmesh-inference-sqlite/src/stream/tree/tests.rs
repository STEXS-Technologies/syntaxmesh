use super::*;

fn connection() -> Result<Connection> {
    let connection = Connection::open_in_memory()?;
    connection.execute_batch(
        "CREATE TABLE inference_active_pages(page TEXT PRIMARY KEY, payload BLOB NOT NULL)",
    )?;
    Ok(connection)
}

#[test]
fn compressed_path_copy_is_canonical_and_keeps_every_old_root() -> Result<()> {
    let connection = connection()?;
    let keys = (0..64)
        .map(|index| StableId::derive("active-test-key", &[index.to_string().as_bytes()]))
        .collect::<Vec<_>>();
    let mut root = None;
    let mut expected = BTreeSet::new();
    let mut history = Vec::new();
    for key in &keys {
        root = Some(ActiveTree::new(&connection, 4096)?.insert(root, *key)?);
        expected.insert(*key);
        let selected = ActiveTree::new(&connection, 4096)?.keys(root, 64)?;
        if selected != expected.iter().copied().collect::<Vec<_>>() {
            return Err("active-tree insertion differs from set oracle".into());
        }
        history.push((root, selected));
        for candidate in &keys {
            if ActiveTree::new(&connection, 513)?.membership(root, *candidate, expected.len())?
                != expected.contains(candidate)
            {
                return Err("point membership differs from set oracle".into());
            }
        }
    }
    let mut reversed = None;
    for key in keys.iter().rev() {
        reversed = Some(ActiveTree::new(&connection, 4096)?.insert(reversed, *key)?);
    }
    if root != reversed {
        return Err("active-tree root depends on insertion order".into());
    }
    for key in &keys {
        let before: i64 =
            connection.query_row("SELECT COUNT(*) FROM inference_active_pages", [], |row| {
                row.get(0)
            })?;
        root = ActiveTree::new(&connection, 4096)?.remove(root, *key)?;
        expected.remove(key);
        let after: i64 =
            connection.query_row("SELECT COUNT(*) FROM inference_active_pages", [], |row| {
                row.get(0)
            })?;
        if after.checked_sub(before).ok_or("page count underflow")? > 256
            || ActiveTree::new(&connection, 4096)?.keys(root, 64)?
                != expected.iter().copied().collect::<Vec<_>>()
        {
            return Err("active-tree removal copied unrelated pages or lost keys".into());
        }
    }
    if root.is_some() {
        return Err("removing all keys retained nonempty root".into());
    }
    for (old_root, old_keys) in history {
        if ActiveTree::new(&connection, 4096)?.keys(old_root, 64)? != old_keys {
            return Err("path-copy changed historical root".into());
        }
        for after in keys.iter().copied().map(Some).chain(std::iter::once(None)) {
            for limit in [1, 3, 65] {
                let expected_page = old_keys
                    .iter()
                    .copied()
                    .filter(|key| after.is_none_or(|bound| *key > bound))
                    .take(limit)
                    .collect::<Vec<_>>();
                if ActiveTree::new(&connection, 513)?.page(
                    old_root,
                    after,
                    limit,
                    old_keys.len(),
                )? != expected_page
                {
                    return Err("tree page seek differs from sorted-set oracle".into());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn page_and_output_exhaustion_rolls_back_and_corruption_rejects() -> Result<()> {
    let connection = connection()?;
    let left = StableId([0; 32]);
    let right = StableId([255; 32]);
    let root = Some(ActiveTree::new(&connection, 100)?.insert(None, left)?);
    let before: i64 =
        connection.query_row("SELECT COUNT(*) FROM inference_active_pages", [], |row| {
            row.get(0)
        })?;
    {
        let transaction = connection.unchecked_transaction()?;
        if ActiveTree::new(&transaction, 3)?
            .insert(root, right)
            .is_ok()
        {
            return Err("active-tree exhausted write budget succeeded".into());
        }
    }
    let after: i64 =
        connection.query_row("SELECT COUNT(*) FROM inference_active_pages", [], |row| {
            row.get(0)
        })?;
    if before != after {
        return Err("exhausted tree update survived transaction rollback".into());
    }
    let both = Some(ActiveTree::new(&connection, 100)?.insert(root, right)?);
    if !ActiveTree::new(&connection, 1)?.membership(root, left, 1)?
        || ActiveTree::new(&connection, 1)?.membership(root, right, 1)?
        || ActiveTree::new(&connection, 2)?
            .membership(both, left, 2)
            .is_ok()
        || ActiveTree::new(&connection, 100)?
            .membership(both, left, 1)
            .is_ok()
        || ActiveTree::new(&connection, 1)?
            .membership(None, left, 1)
            .is_ok()
        || ActiveTree::new(&connection, 1)?.membership(None, left, 0)?
    {
        return Err("point membership admitted invalid bounds or counts".into());
    }
    if ActiveTree::new(&connection, 1)?.keys(both, 2).is_ok()
        || ActiveTree::new(&connection, 100)?.keys(both, 1).is_ok()
        || ActiveTree::new(&connection, 100)?
            .insert(both, left)
            .is_ok()
    {
        return Err("tree admitted exhausted or duplicate output".into());
    }
    let identity = both.ok_or("missing branch")?;
    connection.execute(
        "UPDATE inference_active_pages SET payload = X'00' WHERE page = ?1",
        [identity.to_hex()],
    )?;
    if ActiveTree::new(&connection, 100)?.keys(both, 2).is_ok() {
        return Err("corrupt page digest accepted".into());
    }
    if ActiveTree::new(&connection, 100)?
        .membership(both, left, 2)
        .is_ok()
    {
        return Err("point membership accepted corrupt page".into());
    }
    Ok(())
}

#[test]
fn validly_encoded_invalid_pivots_and_missing_children_reject() -> Result<()> {
    let connection = connection()?;
    let mut tree = ActiveTree::new(&connection, 100)?;
    let left = tree.insert(None, StableId([0; 32]))?;
    let right = tree.insert(None, StableId([255; 32]))?;
    for (bit, child, count) in [
        (300, right, 2),
        (1, right, 2),
        (0, StableId::derive("missing-page", &[b"x"]), 2),
        (0, right, 3),
    ] {
        let root = Some(tree.put(Page {
            version: 1,
            key: StableId([0; 32]),
            count,
            kind: Kind::Branch {
                bit,
                left,
                right: child,
            },
        })?);
        if ActiveTree::new(&connection, 100)?.keys(root, 2).is_ok()
            || ActiveTree::new(&connection, 100)?
                .page(root, None, 1, usize::try_from(count)?)
                .is_ok()
        {
            return Err("malformed branch or missing child accepted".into());
        }
    }
    Ok(())
}

#[test]
fn late_page_skips_prior_subtrees_with_bounded_reads() -> Result<()> {
    let connection = connection()?;
    let mut root = None;
    let mut keys = Vec::new();
    for index in 0_u16..1024 {
        let mut bytes = [0_u8; 32];
        bytes
            .get_mut(30..)
            .ok_or("missing key suffix")?
            .copy_from_slice(&index.to_be_bytes());
        let key = StableId(bytes);
        root = Some(ActiveTree::new(&connection, 4096)?.insert(root, key)?);
        keys.push(key);
    }
    let before_last = *keys.get(1022).ok_or("missing penultimate key")?;
    let last = *keys.last().ok_or("missing last key")?;
    // Ten compressed branches load the root plus twenty child headers. No
    // earlier leaf walk is needed, despite 1022 preceding results.
    if ActiveTree::new(&connection, 21)?.page(root, Some(before_last), 2, 1024)? != vec![last]
        || ActiveTree::new(&connection, 20)?
            .page(root, Some(before_last), 2, 1024)
            .is_ok()
    {
        return Err("late page exceeded its compressed-path read bound".into());
    }
    let first_key = *keys.first().ok_or("missing first key")?;
    let first_leaf = ActiveTree::new(&connection, 1)?.insert(None, first_key)?;
    connection.execute(
        "UPDATE inference_active_pages SET payload = X'00' WHERE page = ?1",
        [first_leaf.to_hex()],
    )?;
    if ActiveTree::new(&connection, 21)?.page(root, Some(before_last), 2, 1024)? != vec![last]
        || ActiveTree::new(&connection, 4096)?.keys(root, 1024).is_ok()
    {
        return Err("late page visited an unrelated prior subtree".into());
    }
    Ok(())
}
