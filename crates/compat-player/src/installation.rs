//! Locations the player depends on: the workspace (facade sources and scratch output) and the
//! native runtime library it loads.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Installation {
    root: PathBuf,
    native_library: PathBuf,
}

impl Installation {
    /// Uses an explicitly selected native runtime library.
    pub fn new(root: impl Into<PathBuf>, native_library: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            native_library: native_library.into(),
        }
    }

    /// Uses the native runtime library that Cargo built next to the running executable.
    pub fn beside_executable(root: impl Into<PathBuf>) -> Result<Self, String> {
        Ok(Self::new(root, crate::native_image::beside_executable()?))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn native_library(&self) -> &Path {
        &self.native_library
    }
}
