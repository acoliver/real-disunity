//! A Unity project on disk: its editor version and the GUID catalog built from `.meta` files.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::{
    guid::{Guid, ReferenceError},
    yaml::{self, Document, Limits, Mapping, Node, YamlError},
};

/// Largest project file the importer reads.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: {source}")]
    Yaml { path: PathBuf, source: YamlError },
    #[error("{path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("{0} is not a Unity project: missing {1}")]
    NotProject(PathBuf, &'static str),
    #[error("{0}: symbolic links are not followed")]
    Symlink(PathBuf),
    #[error("{0}: path must be relative and stay inside the project")]
    Escape(PathBuf),
    #[error("{0}: file exceeds {MAX_FILE_BYTES} bytes")]
    TooLarge(PathBuf),
    #[error("asset GUID {guid} is claimed by both {first} and {second}")]
    DuplicateGuid {
        guid: Guid,
        first: PathBuf,
        second: PathBuf,
    },
}

impl ImportError {
    fn invalid(path: &Path, message: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.to_owned(),
            message: message.into(),
        }
    }
}

/// One cataloged asset. Paths are relative to the project root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub path: PathBuf,
    /// The importer named in the `.meta` file, such as `TextureImporter`.
    pub importer: String,
    pub folder: bool,
}

#[derive(Debug)]
pub struct Project {
    root: PathBuf,
    editor_version: String,
    assets: BTreeMap<Guid, Asset>,
    orphan_metas: Vec<PathBuf>,
    limits: Limits,
}

impl Project {
    /// Opens the project at `root` and catalogs every `.meta` file under `Assets`.
    pub fn open(root: &Path) -> Result<Self, ImportError> {
        Self::open_with(root, Limits::default())
    }

    pub fn open_with(root: &Path, limits: Limits) -> Result<Self, ImportError> {
        if !root.join("Assets").is_dir() {
            return Err(ImportError::NotProject(root.to_owned(), "Assets"));
        }
        let version = Path::new("ProjectSettings/ProjectVersion.txt");
        if !root.join(version).is_file() {
            return Err(ImportError::NotProject(
                root.to_owned(),
                "ProjectVersion.txt",
            ));
        }
        let mut project = Self {
            root: root.to_owned(),
            editor_version: String::new(),
            assets: BTreeMap::new(),
            orphan_metas: Vec::new(),
            limits,
        };
        project.editor_version = scalar_text(&project.read_plain(version)?, "m_EditorVersion")
            .ok_or_else(|| ImportError::invalid(version, "missing m_EditorVersion"))?;
        for meta in meta_files(root)? {
            project.catalog(&meta)?;
        }
        Ok(project)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn editor_version(&self) -> &str {
        &self.editor_version
    }

    pub fn asset(&self, guid: Guid) -> Option<&Asset> {
        self.assets.get(&guid)
    }

    /// The GUID of the asset at `path`, relative to the project root.
    pub fn guid_of(&self, path: &Path) -> Option<Guid> {
        self.assets
            .iter()
            .find_map(|(guid, asset)| (asset.path == path).then_some(*guid))
    }

    pub fn assets(&self) -> impl Iterator<Item = (Guid, &Asset)> {
        self.assets.iter().map(|(guid, asset)| (*guid, asset))
    }

    /// `.meta` files whose asset is absent. Unity leaves these behind; they are not errors
    /// until something references their GUID.
    pub fn orphan_metas(&self) -> &[PathBuf] {
        &self.orphan_metas
    }

    /// Reads the Unity documents of a scene, prefab or serialized asset.
    pub fn read_documents(&self, relative: &Path) -> Result<Vec<Document>, ImportError> {
        let text = self.read_text(relative)?;
        yaml::parse_documents(&text, self.limits).map_err(|source| ImportError::Yaml {
            path: relative.to_owned(),
            source,
        })
    }

    /// Reads a header-less YAML file such as a `.meta` file.
    pub fn read_plain(&self, relative: &Path) -> Result<Mapping, ImportError> {
        let text = self.read_text(relative)?;
        yaml::parse_plain(&text, self.limits).map_err(|source| ImportError::Yaml {
            path: relative.to_owned(),
            source,
        })
    }

    /// Reads a project file's bytes, such as an image.
    pub fn read_bytes(&self, relative: &Path) -> Result<Vec<u8>, ImportError> {
        let path = self.contained(relative)?;
        let io = |source| ImportError::Io {
            path: relative.to_owned(),
            source,
        };
        if fs::metadata(&path).map_err(io)?.len() > MAX_FILE_BYTES {
            return Err(ImportError::TooLarge(relative.to_owned()));
        }
        fs::read(&path).map_err(io)
    }

    fn read_text(&self, relative: &Path) -> Result<String, ImportError> {
        String::from_utf8(self.read_bytes(relative)?)
            .map_err(|_| ImportError::invalid(relative, "file is not UTF-8 text"))
    }

    /// Resolves `relative` under the root without `..`, absolute parts or symbolic links.
    fn contained(&self, relative: &Path) -> Result<PathBuf, ImportError> {
        let mut path = self.root.clone();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(ImportError::Escape(relative.to_owned()));
            };
            path.push(part);
            if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(ImportError::Symlink(relative.to_owned()));
            }
        }
        Ok(path)
    }

    fn catalog(&mut self, meta: &Path) -> Result<(), ImportError> {
        let mapping = self.read_plain(meta)?;
        let guid = scalar_text(&mapping, "guid")
            .ok_or_else(|| ImportError::invalid(meta, "missing guid"))?
            .parse::<Guid>()
            .map_err(|error: ReferenceError| ImportError::invalid(meta, error.to_string()))?;
        let path = meta.with_extension("");
        if fs::symlink_metadata(self.root.join(&path)).is_err() {
            self.orphan_metas.push(meta.to_owned());
            return Ok(());
        }
        let asset = Asset {
            path,
            importer: importer(&mapping),
            folder: scalar_text(&mapping, "folderAsset").as_deref() == Some("yes"),
        };
        if let Some(first) = self.assets.get(&guid) {
            return Err(ImportError::DuplicateGuid {
                guid,
                first: first.path.clone(),
                second: asset.path,
            });
        }
        self.assets.insert(guid, asset);
        Ok(())
    }
}

fn scalar_text(mapping: &Mapping, key: &str) -> Option<String> {
    mapping
        .get(key)
        .and_then(Node::as_scalar)
        .map(|scalar| scalar.text().to_owned())
}

fn importer(mapping: &Mapping) -> String {
    mapping
        .iter()
        .find(|(key, _)| key.ends_with("Importer"))
        .map_or_else(String::new, |(key, _)| key.to_owned())
}

/// Every `.meta` file under `Assets`, relative to `root`, in sorted order.
fn meta_files(root: &Path) -> Result<Vec<PathBuf>, ImportError> {
    let mut found = Vec::new();
    let mut pending = vec![PathBuf::from("Assets")];
    while let Some(dir) = pending.pop() {
        let io = |source| ImportError::Io {
            path: dir.clone(),
            source,
        };
        for entry in fs::read_dir(root.join(&dir)).map_err(io)? {
            let entry = entry.map_err(io)?;
            let relative = dir.join(entry.file_name());
            let kind = entry.file_type().map_err(io)?;
            if kind.is_symlink() {
                return Err(ImportError::Symlink(relative));
            }
            if kind.is_dir() {
                pending.push(relative);
            } else if relative.extension().is_some_and(|ext| ext == "meta") {
                found.push(relative);
            }
        }
    }
    found.sort();
    Ok(found)
}
