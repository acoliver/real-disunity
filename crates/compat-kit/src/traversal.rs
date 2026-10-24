use std::os::unix::fs::MetadataExt;
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum ScanError {
    Exhausted {
        attempts: usize,
        cause: Box<ScanError>,
    },
    Io {
        path: PathBuf,
        operation: &'static str,
        error: io::Error,
        descendant: bool,
    },
    Rejected {
        path: PathBuf,
        reason: &'static str,
    },
}
impl ScanError {
    fn io(path: &Path, operation: &'static str, error: io::Error, descendant: bool) -> Self {
        Self::Io {
            path: path.into(),
            operation,
            error,
            descendant,
        }
    }
    fn transient(&self) -> bool {
        matches!(self, Self::Io { error, descendant: true, .. } if error.kind() == io::ErrorKind::NotFound)
    }
}
impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted { attempts, cause } => write!(
                f,
                "live artifact scan exhausted {attempts} attempts: {cause}"
            ),
            Self::Io {
                path,
                operation,
                error,
                ..
            } => write!(
                f,
                "artifact/source scan {operation} {}: {error}",
                path.display()
            ),
            Self::Rejected { path, reason } => write!(f, "{reason}: {}", path.display()),
        }
    }
}
impl std::error::Error for ScanError {}

type ScanResult<T> = Result<T, ScanError>;
struct Scan<'a, F> {
    root: &'a Path,
    max_bytes: u64,
    max_files: usize,
    skip_git: bool,
    before_metadata: F,
    paths: Vec<PathBuf>,
    bytes: u64,
}
impl<F: FnMut(&Path)> Scan<'_, F> {
    fn visit(&mut self, directory: &Path) -> ScanResult<()> {
        let entries = fs::read_dir(directory)
            .map_err(|e| ScanError::io(directory, "read_dir", e, directory != self.root))?;
        for entry in entries {
            let entry =
                entry.map_err(|e| ScanError::io(directory, "entry", e, directory != self.root))?;
            if self.skip_git && entry.file_name() == ".git" {
                continue;
            }
            let path = entry.path();
            (self.before_metadata)(&path);
            let meta = fs::symlink_metadata(&path)
                .map_err(|e| ScanError::io(&path, "metadata", e, true))?;
            if meta.is_dir() {
                self.visit(&path)?;
            } else if meta.is_file() {
                self.bytes = self.bytes.saturating_add(meta.len());
                self.paths.push(path.clone());
                if self.bytes > self.max_bytes || self.paths.len() > self.max_files {
                    return Err(ScanError::Rejected {
                        path,
                        reason: "artifact/source storage cap exceeded",
                    });
                }
            } else {
                return Err(ScanError::Rejected {
                    path,
                    reason: if meta.is_symlink() {
                        "symlink rejected"
                    } else {
                        "nonregular artifact/source rejected"
                    },
                });
            }
        }
        Ok(())
    }
    fn once(&mut self) -> ScanResult<Vec<PathBuf>> {
        self.paths.clear();
        self.bytes = 0;
        let identity = validate_root(self.root)?;
        self.visit(self.root)?;
        verify_root(self.root, identity)?;
        self.paths.sort();
        Ok(std::mem::take(&mut self.paths))
    }
    fn live(&mut self) -> ScanResult<Vec<PathBuf>> {
        let identity = validate_root(self.root)?;
        for attempt in 1..=3 {
            match self.once() {
                Ok(paths) => return Ok(paths),
                Err(error) if error.transient() => {
                    verify_root(self.root, identity)?;
                    if attempt == 3 {
                        return Err(ScanError::Exhausted {
                            attempts: attempt,
                            cause: Box::new(error),
                        });
                    }
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }
}
fn validate_root(root: &Path) -> ScanResult<(u64, u64)> {
    let meta =
        fs::symlink_metadata(root).map_err(|e| ScanError::io(root, "root metadata", e, false))?;
    if !meta.is_dir() {
        return Err(ScanError::Rejected {
            path: root.into(),
            reason: "scan root must be a real directory",
        });
    }
    Ok((meta.dev(), meta.ino()))
}
fn verify_root(root: &Path, expected: (u64, u64)) -> ScanResult<()> {
    if validate_root(root)? != expected {
        return Err(ScanError::Rejected {
            path: root.into(),
            reason: "scan root changed during traversal",
        });
    }
    Ok(())
}
pub(crate) fn scan(
    root: &Path,
    max_bytes: u64,
    max_files: usize,
    skip_git: bool,
) -> ScanResult<Vec<PathBuf>> {
    scanner(root, max_bytes, max_files, skip_git, |_| {}).once()
}
pub fn live_tree_files(root: &Path, max_bytes: u64, max_files: usize) -> ScanResult<Vec<PathBuf>> {
    scanner(root, max_bytes, max_files, false, |_| {}).live()
}
fn scanner<F: FnMut(&Path)>(
    root: &Path,
    max_bytes: u64,
    max_files: usize,
    skip_git: bool,
    before_metadata: F,
) -> Scan<'_, F> {
    Scan {
        root,
        max_bytes,
        max_files,
        skip_git,
        before_metadata,
        paths: Vec::new(),
        bytes: 0,
    }
}

#[cfg(test)]
mod tests;
