//! A built scene's hierarchy: game objects, their transforms, components and script classes.

use std::{collections::BTreeMap, path::Path};

use worldrunner_unity_serialized::{
    ClassId,
    classes::{GameObject, MonoBehaviour, RectTransform, Transform},
    object::PPtr,
};

use crate::{PlayerError, player_files::PlayerFiles};

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
    let mut files = PlayerFiles::new(data_dir);
    let scene_path = data_dir.join(scene);
    files.open_path(&scene_path)?;
    let roots = files.roots(&scene_path)?;
    roots
        .into_iter()
        .map(|transform| files.node(&scene_path, transform))
        .collect()
}

impl PlayerFiles {
    /// Transforms without a parent, ordered by path ID.
    pub(crate) fn roots(&mut self, path: &Path) -> Result<Vec<i64>, PlayerError> {
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

    pub(crate) fn transform(
        &mut self,
        path: &Path,
        path_id: i64,
    ) -> Result<Transform, PlayerError> {
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
        Ok(self.script_class(path, behaviour.script)?.full_name)
    }
}
