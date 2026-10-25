//! A Unity scene expanded into one table of objects.
//!
//! Prefab instances are expanded from their prefab files with the instance's source-authored
//! overrides applied. Prefabs and serialized assets that objects reference (for example a prefab
//! passed to `Instantiate` or a `ScriptableObject`) are loaded once as templates or assets.
//! References between objects resolve to [`ObjectId`]s; references to other assets such as
//! textures and scripts stay external.

mod expand;
mod modify;
mod value;
mod views;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{guid::Guid, project::ImportError, project::Project};

pub use value::{Reference, Value};
pub use views::{GameObject, Transform};

/// Index of an object in [`Scene::objects`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(u32);

impl ObjectId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Where an object belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Part of the imported scene, including expanded prefab instances.
    Scene,
    /// Part of a prefab loaded as an inactive template because something references it.
    Template(Guid),
    /// Part of a serialized asset file such as a `ScriptableObject` or physics material.
    Asset(Guid),
}

/// The file and fileID an object was read from. `asset` is `None` for the scene file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    pub asset: Option<Guid>,
    pub file_id: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub class_id: i32,
    pub type_name: String,
    pub origin: Origin,
    pub source: Source,
    /// Serialized fields with prefab bookkeeping removed and references resolved.
    pub fields: Value,
    /// Removed by a prefab instance's `m_RemovedComponents` override.
    pub removed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error("{path} object {file_id}: {message}")]
    Object {
        path: PathBuf,
        file_id: i64,
        message: String,
    },
    #[error("{0}: prefab nests itself")]
    Cycle(PathBuf),
}

impl SceneError {
    pub(crate) fn object(path: &Path, file_id: i64, message: impl Into<String>) -> Self {
        Self::Object {
            path: path.to_owned(),
            file_id,
            message: message.into(),
        }
    }
}

#[derive(Debug)]
pub struct Scene {
    path: PathBuf,
    objects: Vec<Object>,
    roots: Vec<ObjectId>,
    templates: BTreeMap<Guid, ObjectId>,
}

/// Imports the scene at `path`, relative to the project root.
pub fn import_scene(project: &Project, path: &Path) -> Result<Scene, SceneError> {
    expand::Importer::new(project).scene(path)
}

impl Scene {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every object that was not removed by a prefab override.
    pub fn objects(&self) -> impl Iterator<Item = (ObjectId, &Object)> {
        (0..)
            .map(ObjectId)
            .zip(&self.objects)
            .filter(|(_, object)| !object.removed)
    }

    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.objects
            .get(id.index())
            .filter(|object| !object.removed)
    }

    /// The scene's root transforms in their authored order.
    pub fn roots(&self) -> &[ObjectId] {
        &self.roots
    }

    /// The root transform of a prefab loaded as a template.
    pub fn template(&self, prefab: Guid) -> Option<ObjectId> {
        self.templates.get(&prefab).copied()
    }

    /// References whose target is absent from the project, with the object that holds them.
    pub fn missing_references(&self) -> Vec<(ObjectId, Reference)> {
        let mut missing = Vec::new();
        for (id, object) in self.objects() {
            object.fields.visit_references(&mut |reference| {
                if matches!(reference, Reference::Missing { .. }) {
                    missing.push((id, *reference));
                }
            });
        }
        missing
    }
}
