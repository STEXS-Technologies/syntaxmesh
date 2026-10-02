use std::io::{self, Read};
use std::path::Path;

/// Fingerprint opaque absolute resolver inputs, including missing paths.
/// Contents are streamed; timestamps are not used. This is not a filesystem snapshot.
///
/// # Errors
/// Returns invalid path/encoding and filesystem errors except unavailable lookup
/// paths (NotFound or NotADirectory).
pub fn module_input_fingerprint(inputs: &[String]) -> io::Result<[u8; 32]> {
    let mut ordered = inputs.iter().collect::<Vec<_>>();
    ordered.sort();
    ordered.dedup();
    let mut hash = blake3::Hasher::new();
    hash.update(b"syntaxmesh-module-inputs-v2");
    for input in ordered {
        let path = Path::new(input);
        if !path.is_absolute() {
            return Err(io::Error::other("module input path must be absolute"));
        }
        field(&mut hash, input.as_bytes())?;
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                hash.update(b"missing");
                continue;
            }
            Err(error) => return Err(error),
        };
        if metadata.is_symlink() {
            hash.update(b"symlink");
            let target = std::fs::read_link(path)?;
            field(
                &mut hash,
                target
                    .to_str()
                    .ok_or_else(|| io::Error::other("module input link target is not UTF-8"))?
                    .as_bytes(),
            )?;
        } else {
            hash.update(b"direct");
        }
        let followed = match std::fs::metadata(path) {
            Ok(target_metadata) => target_metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                hash.update(b"missing-target");
                continue;
            }
            Err(error) => return Err(error),
        };
        let canonical = std::fs::canonicalize(path)?;
        field(
            &mut hash,
            canonical
                .to_str()
                .ok_or_else(|| io::Error::other("canonical module input path is not UTF-8"))?
                .as_bytes(),
        )?;
        if followed.is_file() {
            hash.update(b"file");
            let mut content = blake3::Hasher::new();
            let mut file = std::fs::File::open(path)?;
            let mut buffer = [0_u8; 8192];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                content.update(
                    buffer
                        .get(..count)
                        .ok_or_else(|| io::Error::other("invalid read length"))?,
                );
            }
            hash.update(content.finalize().as_bytes());
        } else if followed.is_dir() {
            hash.update(b"directory");
        } else {
            hash.update(b"other");
        }
    }
    Ok(*hash.finalize().as_bytes())
}

fn field(hash: &mut blake3::Hasher, bytes: &[u8]) -> io::Result<()> {
    let length = u64::try_from(bytes.len()).map_err(io::Error::other)?;
    hash.update(&length.to_le_bytes());
    hash.update(bytes);
    Ok(())
}

#[cfg(test)]
mod tests;
