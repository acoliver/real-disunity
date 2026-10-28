//! The serialized files of a player's data directory, opened on first use, and the
//! references between them.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use worldrunner_unity_serialized::{classes::MonoScript, object::PPtr};

use crate::{PlayerError, player_objects::OpenFile};

/// A script class as a `MonoScript` names it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScriptClass {
    /// The assembly's name without `.dll`, such as `Assembly-CSharp`.
    pub assembly: String,
    /// The namespace and class name, joined by a dot.
    pub full_name: String,
}

impl ScriptClass {
    /// The class's key in the script layouts: `Assembly:Namespace.Class`.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.assembly, self.full_name)
    }
}

pub(crate) struct PlayerFiles {
    pub(crate) data_dir: PathBuf,
    open: BTreeMap<PathBuf, OpenFile>,
    scripts: BTreeMap<(PathBuf, i64), ScriptClass>,
}

impl PlayerFiles {
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_owned(),
            open: BTreeMap::new(),
            scripts: BTreeMap::new(),
        }
    }

    pub(crate) fn open_path(&mut self, path: &Path) -> Result<&mut OpenFile, PlayerError> {
        if !self.open.contains_key(path) {
            let file = OpenFile::open(path)?;
            self.open.insert(path.to_owned(), file);
        }
        Ok(self.open.get_mut(path).expect("opened above"))
    }

    /// The open file a same-file reference points into.
    pub(crate) fn local(&mut self, path: &Path, pptr: PPtr) -> Result<&mut OpenFile, PlayerError> {
        if pptr.file_id != 0 {
            return Err(PlayerError::UnresolvedReference {
                path: path.to_owned(),
                file_id: pptr.file_id,
                path_id: pptr.path_id,
            });
        }
        self.open_path(path)
    }

    /// The class of the script a reference from `path` points to.
    pub(crate) fn script_class(
        &mut self,
        path: &Path,
        script: PPtr,
    ) -> Result<ScriptClass, PlayerError> {
        let target = self.referenced_file(path, script)?;
        let key = (target.clone(), script.path_id);
        if let Some(class) = self.scripts.get(&key) {
            return Ok(class.clone());
        }
        let script: MonoScript = self.open_path(&target)?.decode(script.path_id)?;
        let full_name = if script.namespace.is_empty() {
            script.class_name
        } else {
            format!("{}.{}", script.namespace, script.class_name)
        };
        let assembly = script
            .assembly
            .strip_suffix(".dll")
            .unwrap_or(&script.assembly)
            .to_owned();
        let class = ScriptClass {
            assembly,
            full_name,
        };
        self.scripts.insert(key, class.clone());
        Ok(class)
    }

    /// The path of the file a reference points into: this file, or an external file by its
    /// name in the data directory.
    pub(crate) fn referenced_file(
        &mut self,
        path: &Path,
        pptr: PPtr,
    ) -> Result<PathBuf, PlayerError> {
        if pptr.file_id == 0 {
            return Ok(path.to_owned());
        }
        let unresolved = || PlayerError::UnresolvedReference {
            path: path.to_owned(),
            file_id: pptr.file_id,
            path_id: pptr.path_id,
        };
        let index = pptr
            .file_id
            .checked_sub(1)
            .and_then(|id| usize::try_from(id).ok());
        let file = self.open_path(path)?;
        let external = index
            .and_then(|i| file.file.externals.get(i))
            .ok_or_else(unresolved)?;
        let name = external
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&external.path)
            .to_owned();
        if matches!(name.as_str(), "" | "." | "..") || name.contains(':') {
            return Err(unresolved());
        }
        Ok(self.data_dir.join(name))
    }
}
