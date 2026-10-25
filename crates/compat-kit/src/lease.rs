use crate::files::{Result, io};
use std::{
    fs::{self, File, OpenOptions},
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
