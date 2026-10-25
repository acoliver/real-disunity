use crate::{
    files::{self, Result, io},
    lease,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const SOFT_LIMIT: u64 = 450_000_000_000;
const HARD_LIMIT: u64 = 500_000_000_000;

#[derive(Deserialize, Serialize)]
struct Owner {
    schema_version: u32,
    path: PathBuf,
    created_ms: u128,
}
pub struct OwnedDirectory {
    pub path: PathBuf,
    _lease: File,
}
fn registry(root: &Path) -> Result<PathBuf> {
    let path = files::work_path(root, Path::new(".work/compat/ownership"))?;
    fs::create_dir_all(&path).map_err(io)?;
    Ok(path)
}
fn marker(root: &Path, path: &Path) -> Result<PathBuf> {
    Ok(registry(root)?.join(format!(
        "{}.json",
        path.file_name()
            .ok_or("directory name missing")?
            .to_string_lossy()
    )))
}
pub fn allocate(root: &Path) -> Result<OwnedDirectory> {
    let base = files::runs_root(root)?;
    let path = base.join(files::unique_id());
    fs::create_dir_all(&path).map_err(io)?;
    let marker = marker(root, &path)?;
    let mut file = lease::exclusive(&marker)?.ok_or("new directory lease is busy")?;
    let owner = Owner {
        schema_version: 1,
        path: path.clone(),
        created_ms: files::now_ms(),
    };
    file.write_all(&serde_json::to_vec(&owner).map_err(io)?)
        .map_err(io)?;
    file.sync_all().map_err(io)?;
    file.lock_shared().map_err(io)?;
    Ok(OwnedDirectory { path, _lease: file })
}
fn usage(root: &Path) -> Result<u64> {
    retry_measurement(|| measure(root))
}
fn retry_measurement(mut measure: impl FnMut() -> Result<u64>) -> Result<u64> {
    let mut last = String::new();
    for _ in 0..3 {
        match measure() {
            Ok(bytes) => return Ok(bytes),
            Err(error) => last = error,
        }
    }
    Err(last)
}
fn measure(root: &Path) -> Result<u64> {
    let output = Command::new("du")
        .arg("-sb")
        .arg(root)
        .output()
        .map_err(io)?;
    if !output.status.success() {
        return Err(format!(
            "worktree storage budget could not be measured: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::str::from_utf8(&output.stdout)
        .map_err(io)?
        .split_whitespace()
        .next()
        .ok_or("missing storage size")?
        .parse()
        .map_err(io)
}
fn eligible(root: &Path, owner: &Owner) -> Result<bool> {
    if owner.schema_version != 1 {
        return Ok(false);
    }
    let path = files::work_path(root, &owner.path)?;
    let base = files::runs_root(root)?;
    if path.parent() != Some(base.as_path()) || !path.is_dir() {
        return Ok(false);
    }
    // Reject the entire tree if any symlink or special file is encountered.
    files::tree_files(&path, u64::MAX, 1_000_000)?;
    Ok(true)
}
pub fn prune(root: &Path, soft_limit: u64) -> Result<Vec<PathBuf>> {
    let registry = registry(root)?;
    let lock_path = registry.join("prune.lock");
    let Some(_guard) = lease::exclusive(&lock_path)? else {
        return Ok(Vec::new());
    };
    let mut owners = fs::read_dir(&registry)
        .map_err(io)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| {
            files::read_json::<Owner>(&e.path())
                .ok()
                .map(|o| (e.path(), o))
        })
        .collect::<Vec<_>>();
    owners.sort_by_key(|(_, owner)| owner.created_ms);
    let mut removed = Vec::new();
    for (marker, owner) in owners {
        if usage(root)? < soft_limit {
            break;
        }
        let Some(_guard) = lease::exclusive(&marker)? else {
            continue;
        };
        if !eligible(root, &owner).unwrap_or(false) {
            continue;
        }
        fs::remove_dir_all(&owner.path).map_err(io)?;
        removed.push(owner.path);
        // Marker inodes remain stable for concurrent lease holders and future inspection.
    }
    Ok(removed)
}
pub fn check_budget(root: &Path) -> Result<()> {
    if usage(root)? >= SOFT_LIMIT {
        prune(root, SOFT_LIMIT)?;
    }
    if usage(root)? > HARD_LIMIT {
        return Err("worktree 500 GB storage cap exceeded".into());
    }
    Ok(())
}

#[cfg(test)]
mod measurement_tests {
    use super::*;
    #[test]
    fn transient_measurement_failure_uses_only_a_later_successful_size() {
        let mut attempts = 0;
        let result = retry_measurement(|| {
            attempts += 1;
            if attempts < 3 {
                Err("du: file disappeared".into())
            } else {
                Ok(512)
            }
        });
        assert_eq!(result, Ok(512));
        assert_eq!(attempts, 3);
    }
    #[test]
    fn persistent_measurement_failure_is_bounded_and_fails_closed() {
        let mut attempts = 0;
        let result = retry_measurement(|| {
            attempts += 1;
            Err(format!("measurement failure {attempts}"))
        });
        assert_eq!(result, Err("measurement failure 3".into()));
        assert_eq!(attempts, 3);
    }
    #[test]
    fn successful_measurement_does_not_retry() {
        let mut attempts = 0;
        let result = retry_measurement(|| {
            attempts += 1;
            Ok(500_000_000_001)
        });
        assert_eq!(result, Ok(500_000_000_001));
        assert_eq!(attempts, 1);
    }
}
