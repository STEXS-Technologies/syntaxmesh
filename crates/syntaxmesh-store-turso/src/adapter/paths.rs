use std::path::{Path, PathBuf};

use super::TursoStoreError;

pub(super) fn database_path(path: &Path) -> Result<PathBuf, TursoStoreError> {
    match std::fs::canonicalize(path) {
        Ok(resolved) => Ok(resolved),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::symlink_metadata(path) {
                Ok(_) => {
                    return Err(TursoStoreError::Backend(
                        "database target is a dangling symlink".to_owned(),
                    ));
                }
                Err(metadata_error) if metadata_error.kind() == std::io::ErrorKind::NotFound => {}
                Err(metadata_error) => {
                    return Err(TursoStoreError::Backend(format!(
                        "inspect database path: {metadata_error}"
                    )));
                }
            }
            let name = path.file_name().ok_or_else(|| {
                TursoStoreError::Backend("database path needs a filename".to_owned())
            })?;
            let parent = path
                .parent()
                .filter(|value| !value.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::canonicalize(parent)
                .map(|resolved| resolved.join(name))
                .map_err(|parent_error| {
                    TursoStoreError::Backend(format!("resolve database parent: {parent_error}"))
                })
        }
        Err(error) => Err(TursoStoreError::Backend(format!(
            "resolve database path: {error}"
        ))),
    }
}
