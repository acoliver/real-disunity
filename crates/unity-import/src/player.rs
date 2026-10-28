//! A built Unity player's data directory (`<Game>_Data`): the serialized files that hold its
//! scenes and assets.

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use thiserror::Error;
use worldrunner_unity_serialized::{Header, SerializedError, SerializedFile};

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("{}: invalid scene hierarchy: {reason}", path.display())]
    InvalidHierarchy { path: PathBuf, reason: String },
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("{}: {source}", path.display())]
    Serialized {
        path: PathBuf,
        source: SerializedError,
    },
    #[error("{}: no object with path ID {path_id}", path.display())]
    MissingObject { path: PathBuf, path_id: i64 },
    /// The names decoded from `PlayerSettings` differ from the ones `app.info` holds, which
    /// the build writes from the same settings, so the decoder misread them.
    #[error(
        "{}: PlayerSettings decode to {decoded:?} but app.info names {app_info:?}",
        path.display()
    )]
    IdentityMismatch {
        path: PathBuf,
        decoded: (String, String),
        app_info: (String, String),
    },
    #[error(
        "{}: the reference to file {file_id}, path ID {path_id} names no file this one lists",
        path.display()
    )]
    UnresolvedReference {
        path: PathBuf,
        file_id: i32,
        path_id: i64,
    },
}

/// One serialized file of a player, read through its metadata.
#[derive(Debug)]
pub struct PlayerFile {
    /// The file name inside the data directory, as other files' externals name it.
    pub name: String,
    pub file: SerializedFile,
}

/// Whether a data directory entry is a serialized file: the player's global managers,
/// a built scene (`level0`, `level1`, ...) or an asset file (`*.assets`).
#[must_use]
pub fn is_serialized_name(name: &str) -> bool {
    name == "globalgamemanagers"
        || name.ends_with(".assets")
        || name
            .strip_prefix("level")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Reads the metadata of every serialized file directly inside `data_dir`, sorted by name.
/// Only each file's header and metadata are read, so large asset files cost little.
///
/// # Errors
/// Fails if the directory or a file cannot be read, or a file is not a supported
/// serialized file.
pub fn read_player(data_dir: &Path) -> Result<Vec<PlayerFile>, PlayerError> {
    let io_error = |path: &Path| {
        let path = path.to_owned();
        move |source| PlayerError::Io { path, source }
    };
    let mut names = Vec::new();
    for entry in std::fs::read_dir(data_dir).map_err(io_error(data_dir))? {
        let entry = entry.map_err(io_error(data_dir))?;
        let is_file = entry
            .file_type()
            .map_err(io_error(&entry.path()))?
            .is_file();
        if let Some(name) = entry.file_name().to_str()
            && is_file
            && is_serialized_name(name)
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let path = data_dir.join(&name);
            let file = read_metadata(&path)?;
            Ok(PlayerFile { name, file })
        })
        .collect()
}

/// Reads one serialized file's header and metadata.
///
/// # Errors
/// Fails if the file cannot be read or is not a supported serialized file.
pub fn read_metadata(path: &Path) -> Result<SerializedFile, PlayerError> {
    let io_error = |source| PlayerError::Io {
        path: path.to_owned(),
        source,
    };
    let serialized_error = |source| PlayerError::Serialized {
        path: path.to_owned(),
        source,
    };
    let mut file = File::open(path).map_err(io_error)?;
    let mut prefix = Vec::with_capacity(Header::MAX_LEN);
    (&mut file)
        .take(Header::MAX_LEN as u64)
        .read_to_end(&mut prefix)
        .map_err(io_error)?;
    let header = Header::parse(&prefix).map_err(serialized_error)?;
    let rest = header.metadata_end().saturating_sub(prefix.len() as u64);
    file.take(rest).read_to_end(&mut prefix).map_err(io_error)?;
    SerializedFile::parse(&prefix).map_err(serialized_error)
}
