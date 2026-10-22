use super::*;

#[test]
fn oversized_receipt_rejects_before_json_decoding() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let receipt = directory.path().join("origin.json");
    std::fs::write(&receipt, vec![b' '; 16_385])?;
    let error = read_origin(&receipt)
        .err()
        .ok_or("oversized origin admitted")?;
    if !error.to_string().contains("exceeds 16 KiB") {
        return Err("origin JSON was decoded before size admission".into());
    }
    Ok(())
}

#[test]
fn malformed_and_non_file_receipts_reject() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    if read_origin(directory.path()).is_ok() {
        return Err("origin directory admitted".into());
    }
    let receipt = directory.path().join("origin.json");
    for body in ["{}", "null", "{", "[]"] {
        std::fs::write(&receipt, body)?;
        if read_origin(&receipt).is_ok() {
            return Err("malformed origin receipt admitted".into());
        }
    }
    Ok(())
}
