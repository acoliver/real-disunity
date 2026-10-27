//! A built scene's hierarchy: game objects, their transforms, components and script classes.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use worldrunner_unity_serialized::{
    ClassId,
    classes::{GameObject, MonoBehaviour, MonoScript, RectTransform, Transform},
    object::PPtr,
};

use crate::{PlayerError, player_objects::OpenFile};

/// One game object of a scene, with its children in order.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    pub name: String,
    pub active: bool,
    pub local_position: [f32; 3],
    /// Component classes; script components are named by their script's full class name.
    pub components: Vec<String>,
    pub children: Vec<SceneNode>,
}

/// Reads the hierarchy of one serialized scene file in a player's data directory. Script
/// classes are looked up in the files the scene references.
///
/// # Errors
/// Fails if a file cannot be read, an object does not decode, or a reference names a file or
/// object that does not exist.
pub fn read_scene_hierarchy(data_dir: &Path, scene: &str) -> Result<Vec<SceneNode>, PlayerError> {
    let mut files = Files {
        data_dir: data_dir.to_owned(),
        open: BTreeMap::new(),
        scripts: BTreeMap::new(),
    };
    let scene_path = data_dir.join(scene);
    files.open_path(&scene_path)?;
    let roots = files.roots(&scene_path)?;
    roots
        .into_iter()
        .map(|transform| files.node(&scene_path, transform))
        .collect()
}

struct Files {
    data_dir: PathBuf,
    open: BTreeMap<PathBuf, OpenFile>,
    scripts: BTreeMap<(PathBuf, i64), String>,
}

impl Files {
    fn open_path(&mut self, path: &Path) -> Result<&mut OpenFile, PlayerError> {
        if !self.open.contains_key(path) {
            let file = OpenFile::open(path)?;
            self.open.insert(path.to_owned(), file);
        }
        Ok(self.open.get_mut(path).expect("opened above"))
    }

    /// Transforms without a parent, ordered by path ID.
    fn roots(&mut self, path: &Path) -> Result<Vec<i64>, PlayerError> {
        let file = self.open_path(path)?;
        let ids: Vec<_> = file
            .file
            .objects()
            .iter()
            .filter(|o| matches!(o.class_id, ClassId::TRANSFORM | ClassId::RECT_TRANSFORM))
            .map(|o| o.path_id)
            .collect();
        let transforms = ids
            .into_iter()
            .map(|id| self.transform(path, id).map(|transform| (id, transform)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        crate::player_hierarchy::roots(path, &transforms)
    }

    fn transform(&mut self, path: &Path, path_id: i64) -> Result<Transform, PlayerError> {
        let file = self.open_path(path)?;
        let class = file.file.object(path_id).map(|o| o.class_id);
        if class == Some(ClassId::RECT_TRANSFORM) {
            file.decode::<RectTransform>(path_id).map(|r| r.0)
        } else {
            file.decode::<Transform>(path_id)
        }
    }

    fn node(&mut self, path: &Path, transform_id: i64) -> Result<SceneNode, PlayerError> {
        let transform = self.transform(path, transform_id)?;
        let game_object: GameObject = self
            .local(path, transform.game_object)?
            .decode(transform.game_object.path_id)?;
        let mut components = Vec::new();
        for component in &game_object.components {
            components.push(self.component_name(path, *component)?);
        }
        let children = transform
            .children
            .iter()
            .map(|child| self.node(path, child.path_id))
            .collect::<Result<_, _>>()?;
        Ok(SceneNode {
            name: game_object.name,
            active: game_object.active,
            local_position: transform.local_position,
            components,
            children,
        })
    }

    /// The open file a same-file reference points into.
    fn local(&mut self, path: &Path, pptr: PPtr) -> Result<&mut OpenFile, PlayerError> {
        if pptr.file_id != 0 {
            return Err(PlayerError::UnresolvedReference {
                path: path.to_owned(),
                file_id: pptr.file_id,
                path_id: pptr.path_id,
            });
        }
        self.open_path(path)
    }

    fn component_name(&mut self, path: &Path, component: PPtr) -> Result<String, PlayerError> {
        let file = self.local(path, component)?;
        let class = file
            .file
            .object(component.path_id)
            .map(|o| o.class_id)
            .ok_or_else(|| PlayerError::MissingObject {
                path: path.to_owned(),
                path_id: component.path_id,
            })?;
        if class != ClassId::MONO_BEHAVIOUR {
            return Ok(class.to_string());
        }
        let behaviour: MonoBehaviour = file.decode(component.path_id)?;
        self.script_name(path, behaviour.script)
    }

    /// The full class name of a script, from the file the reference points into.
    fn script_name(&mut self, path: &Path, script: PPtr) -> Result<String, PlayerError> {
        let target = self.referenced_file(path, script)?;
        let key = (target.clone(), script.path_id);
        if let Some(name) = self.scripts.get(&key) {
            return Ok(name.clone());
        }
        let script: MonoScript = self.open_path(&target)?.decode(script.path_id)?;
        let name = if script.namespace.is_empty() {
            script.class_name
        } else {
            format!("{}.{}", script.namespace, script.class_name)
        };
        self.scripts.insert(key, name.clone());
        Ok(name)
    }

    /// The path of the file a reference points into: this file, or an external file by its
    /// name in the data directory.
    fn referenced_file(&mut self, path: &Path, pptr: PPtr) -> Result<PathBuf, PlayerError> {
        if pptr.file_id == 0 {
            return Ok(path.to_owned());
        }
        let index = pptr
            .file_id
            .checked_sub(1)
            .and_then(|id| usize::try_from(id).ok());
        let file = self.open_path(path)?;
        let external = index
            .and_then(|i| file.file.externals.get(i))
            .ok_or_else(|| PlayerError::UnresolvedReference {
                path: path.to_owned(),
                file_id: pptr.file_id,
                path_id: pptr.path_id,
            })?;
        let name = external
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&external.path)
            .to_owned();
        if matches!(name.as_str(), "" | "." | "..") || name.contains(':') {
            return Err(PlayerError::UnresolvedReference {
                path: path.to_owned(),
                file_id: pptr.file_id,
                path_id: pptr.path_id,
            });
        }
        Ok(self.data_dir.join(name))
    }
}
