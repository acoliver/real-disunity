use std::path::{Component, Path, PathBuf};

/// Require a portable relative census key without ambiguous separators.
pub fn validate_document_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("unsafe source document path: {path}"));
    }
    Ok(())
}

pub(super) fn anchored(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err("noncanonical inspection input".into());
        }
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("symlink inspection input".into());
        }
    }
    Ok(current)
}

pub(super) fn regular_under(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let current = anchored(root, path)?;
    let metadata = std::fs::metadata(&current).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
        return Err("inspection input is empty, oversized, or nonregular".into());
    }
    Ok(current)
}
