//! A temporary workspace addressed by its canonical path. Kit APIs take the canonical root the
//! CLI resolves, and temporary directories may sit behind a symlink (macOS `/var`).
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct Root {
    _temp: TempDir,
    path: PathBuf,
}
impl Root {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn temporary() -> Root {
    let temp = tempfile::tempdir().unwrap();
    Root {
        path: temp.path().canonicalize().unwrap(),
        _temp: temp,
    }
}
