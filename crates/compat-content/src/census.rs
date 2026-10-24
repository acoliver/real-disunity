use crate::{ImportError, ImportProfile, ScriptSource, catalog};
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, fs, path::Path};

pub(crate) fn scripts(
    root: &Path,
    directory: &Path,
    profile: &ImportProfile,
) -> Result<Vec<ScriptSource>, ImportError> {
    let directory = catalog::relative(directory)?;
    let mut project = catalog::Project::index(root, profile)?;
    let selected = project.root.join(directory);
    reject_symlink_components(&project.root, &selected)?;
    let mut queue = VecDeque::from([selected]);
    let mut entries = 0;
    let mut result = Vec::new();
    while let Some(dir) = queue.pop_front() {
        if !dir
            .canonicalize()
            .map_err(|e| io_error(&e))?
            .starts_with(&project.root)
        {
            return Err(ImportError::Source {
                path: dir.display().to_string(),
                message: "script directory escapes project root".into(),
            });
        }
        for item in fs::read_dir(dir).map_err(|e| io_error(&e))? {
            entries += 1;
            if entries > profile.max_catalog_entries {
                return Err(ImportError::Limit("script census entries".into()));
            }
            let item = item.map_err(|e| io_error(&e))?;
            let kind = item.file_type().map_err(|e| io_error(&e))?;
            if kind.is_symlink() {
                return Err(ImportError::Source {
                    path: item.path().display().to_string(),
                    message: "script census rejects symlink entries".into(),
                });
            }
            if kind.is_dir() {
                queue.push_back(item.path());
            } else if kind.is_file() && item.path().extension().is_some_and(|ext| ext == "cs") {
                let path = item.path();
                let path = path.strip_prefix(&project.root).expect("indexed root");
                let path = catalog::relative(path)?;
                let bytes = project.read(&path, profile, true)?;
                result.push(ScriptSource {
                    path,
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    byte_length: bytes.len() as u64,
                });
            }
        }
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

fn reject_symlink_components(root: &Path, selected: &Path) -> Result<(), ImportError> {
    let relative = selected
        .strip_prefix(root)
        .expect("selected path is rooted");
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if fs::symlink_metadata(&current)
            .map_err(|error| io_error(&error))?
            .file_type()
            .is_symlink()
        {
            return Err(ImportError::Source {
                path: current.display().to_string(),
                message: "script census rejects symlink path components".into(),
            });
        }
    }
    Ok(())
}

fn io_error(error: &std::io::Error) -> ImportError {
    ImportError::Source {
        path: "script census".into(),
        message: error.to_string(),
    }
}
