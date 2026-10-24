use crate::files::{self, Result, io};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

pub fn exclusive(path: &Path) -> Result<Option<File>> {
    if fs::symlink_metadata(path).is_ok_and(|m| !m.is_file()) {
        return Err(format!("nonregular lease rejected: {}", path.display()));
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(io)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(error) => Err(io(error)),
    }
}

pub fn refresh(path: &Path) -> Result<Option<File>> {
    let Some(mut file) = exclusive(path)? else {
        return Ok(None);
    };
    let owner = serde_json::json!({"pid":std::process::id(),"generation":files::unique_id()});
    file.set_len(0).map_err(io)?;
    file.write_all(&serde_json::to_vec(&owner).map_err(io)?)
        .map_err(io)?;
    file.sync_all().map_err(io)?;
    // Keep the inode present: unlinking allows waiters to lock different generations.
    Ok(Some(file))
}
