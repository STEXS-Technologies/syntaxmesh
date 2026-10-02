use super::*;

#[cfg(unix)]
#[test]
fn fingerprints_symlink_targets_and_intermediate_directory_retargeting()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    let fixture = tempfile::tempdir()?;
    let left = fixture.path().join("left");
    let right = fixture.path().join("right");
    std::fs::create_dir(&left)?;
    std::fs::create_dir(&right)?;
    std::fs::write(left.join("config.json"), "same")?;
    std::fs::write(right.join("config.json"), "same")?;
    let alias = fixture.path().join("alias");
    symlink(&left, &alias)?;
    let inputs = [alias
        .join("config.json")
        .to_str()
        .ok_or("fixture encoding")?
        .to_owned()];
    let before = module_input_fingerprint(&inputs)?;
    std::fs::remove_file(&alias)?;
    symlink(&right, &alias)?;
    if before == module_input_fingerprint(&inputs)? {
        return Err("intermediate symlink target identity was ignored".into());
    }
    let link = fixture.path().join("config-link.json");
    symlink(left.join("config.json"), &link)?;
    let direct = [link.to_str().ok_or("fixture encoding")?.to_owned()];
    let original = module_input_fingerprint(&direct)?;
    std::fs::remove_file(&link)?;
    symlink(right.join("config.json"), &link)?;
    let retargeted = module_input_fingerprint(&direct)?;
    std::fs::remove_file(right.join("config.json"))?;
    let broken = module_input_fingerprint(&direct)?;
    std::fs::remove_file(&link)?;
    let missing = module_input_fingerprint(&direct)?;
    if original == retargeted || retargeted == broken || broken == missing {
        return Err("symlink retarget/broken/missing states were conflated".into());
    }
    Ok(())
}

#[test]
fn fingerprints_contents_existence_and_canonical_input_order()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let first = fixture.path().join("base.json");
    let second = fixture.path().join("missing.json");
    let paths = [
        first.to_str().ok_or("fixture path")?.to_owned(),
        second.to_str().ok_or("fixture path")?.to_owned(),
    ];
    std::fs::write(&first, "one")?;
    let original = module_input_fingerprint(&paths)?;
    if original
        != module_input_fingerprint(&[
            paths.get(1).ok_or("second path")?.clone(),
            paths.first().ok_or("first path")?.clone(),
            paths.first().ok_or("first path")?.clone(),
        ])?
    {
        return Err("input order changed fingerprint".into());
    }
    std::fs::write(&first, "two")?;
    let changed = module_input_fingerprint(&paths)?;
    std::fs::write(&second, "new")?;
    let created = module_input_fingerprint(&paths)?;
    std::fs::remove_file(&second)?;
    if original == changed
        || changed == created
        || changed != module_input_fingerprint(&paths)?
        || module_input_fingerprint(&["relative.json".to_owned()]).is_ok()
    {
        return Err("content/existence fingerprint semantics differ".into());
    }
    Ok(())
}
