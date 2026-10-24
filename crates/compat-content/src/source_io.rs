use crate::ImportError;
use std::{fs, io::Read, path::Path};

/// Bounded content reads supplied by the composition root. Implementations must be read-only
/// and return at most `limit` bytes. The importer rechecks reads before publishing a receipt.
pub trait ContentSource {
    fn read(&self, root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>, ImportError>;
}

pub struct FileContentSource;
impl ContentSource for FileContentSource {
    fn read(&self, root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>, ImportError> {
        read(root, relative, limit)
    }
}

pub(crate) fn error(path: &str, message: impl Into<String>) -> ImportError {
    ImportError::Source {
        path: path.into(),
        message: message.into(),
    }
}

pub(crate) fn read(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>, ImportError> {
    crate::catalog::relative(Path::new(relative))?;
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        path.push(part);
        let metadata = fs::symlink_metadata(&path).map_err(|e| error(relative, e.to_string()))?;
        if metadata.is_symlink() {
            return Err(error(relative, "source symlink rejected"));
        }
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| error(relative, e.to_string()))?;
    if !canonical.starts_with(root) {
        return Err(error(relative, "source escapes project root"));
    }
    let metadata = fs::metadata(&canonical).map_err(|e| error(relative, e.to_string()))?;
    if !metadata.is_file() {
        return Err(error(relative, "source must be a regular file"));
    }
    if metadata.len() > limit as u64 {
        return Err(ImportError::Limit(format!("file bytes: {relative}")));
    }
    let mut bytes = Vec::new();
    fs::File::open(canonical)
        .map_err(|e| error(relative, e.to_string()))?
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| error(relative, e.to_string()))?;
    if bytes.len() > limit {
        return Err(ImportError::Limit(format!("file bytes: {relative}")));
    }
    Ok(bytes)
}
