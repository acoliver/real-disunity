use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, String>;
pub fn runs_root(root: &Path) -> Result<PathBuf> {
    let requested = std::env::var_os("COMPAT_RUNS")
        .map_or_else(|| root.join(".work/compat/runs"), PathBuf::from);
    work_path(root, &requested)
}

pub fn expand_command(
    command: &crate::model::CommandSpec,
    root: &Path,
    source: &Path,
    output: &Path,
) -> crate::model::CommandSpec {
    let expand = |value: &str| {
        value
            .replace("{root}", &root.to_string_lossy())
            .replace("{source}", &source.to_string_lossy())
            .replace("{output}", &output.to_string_lossy())
    };
    crate::model::CommandSpec {
        program: expand(&command.program),
        args: command.args.iter().map(|arg| expand(arg)).collect(),
    }
}
pub fn io(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
pub fn unique_id() -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        now_ms(),
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_file() || meta.len() > 16_777_216 {
        return Err(format!("invalid or oversized JSON: {}", path.display()));
    }
    serde_json::from_slice(&fs::read(path).map_err(io)?).map_err(io)
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(value).map_err(io)?)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("path needs parent")?;
    fs::create_dir_all(parent).map_err(io)?;
    let tmp = parent.join(format!(".{}.tmp", unique_id()));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)
            .map_err(io)?;
        file.write_all(bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        fs::rename(&tmp, path).map_err(io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
pub fn relative_path(value: &Path) -> Result<()> {
    if value.as_os_str().is_empty()
        || value
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("unsafe relative path: {}", value.display()));
    }
    Ok(())
}
pub fn safe_file(base: &Path, relative: &Path) -> Result<PathBuf> {
    relative_path(relative)?;
    let mut full = base.to_path_buf();
    for part in relative.components() {
        full.push(part);
        if fs::symlink_metadata(&full)
            .map_err(io)?
            .file_type()
            .is_symlink()
        {
            return Err(format!("symlink rejected: {}", full.display()));
        }
    }
    if !full.is_file() {
        return Err(format!("missing file: {}", full.display()));
    }
    Ok(full)
}
pub fn work_path(root: &Path, requested: &Path) -> Result<PathBuf> {
    let work = root.join(".work");
    if root.canonicalize().map_err(io)? != root {
        return Err("COMPAT_ROOT must be canonical".into());
    }
    match fs::symlink_metadata(&work) {
        Ok(meta) => {
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(".work must be a real directory inside COMPAT_ROOT".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let relative = path
        .strip_prefix(&work)
        .map_err(|_| "output must be inside .work")?;
    relative_path(relative)?;
    let mut current = work;
    for part in relative.components() {
        current.push(part);
        if fs::symlink_metadata(&current).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(format!(
                "symlink output path rejected: {}",
                current.display()
            ));
        }
    }
    Ok(path)
}
pub fn hash(path: &Path) -> Result<String> {
    let mut input = fs::File::open(path).map_err(io)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let count = input.read(&mut buffer).map_err(io)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub fn tree_files(root: &Path, max_bytes: u64, max_files: usize) -> Result<Vec<PathBuf>> {
    crate::traversal::scan(root, max_bytes, max_files).map_err(io)
}
pub use crate::traversal::{ScanError, live_tree_files};

pub fn discard_worker_outputs(run_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(run_dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        if ["request.json", "stdout.log", "stderr.log", "process.json"]
            .iter()
            .any(|name| entry.file_name() == *name)
        {
            continue;
        }
        if entry.file_type().map_err(io)?.is_dir() {
            fs::remove_dir_all(entry.path()).map_err(io)?;
        } else {
            fs::remove_file(entry.path()).map_err(io)?;
        }
    }
    Ok(())
}

pub fn check_budget(root: &Path) -> Result<()> {
    crate::storage::check_budget(root)
}
