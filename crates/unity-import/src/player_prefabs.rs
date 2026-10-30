//! Resource game objects as prefabs. A prefab is a scene plan of the root object and its
//! descendants, every object a template that belongs to no scene, planned by the code that
//! plans a scene's objects.

use std::collections::BTreeMap;

use worldrunner_unity_serialized::{ClassId, classes::GameObject, script_data::ScriptLayouts};

use crate::{
    PlayerError,
    plan::ScenePlan,
    player_files::PlayerFiles,
    player_hierarchy::invalid,
    player_plan::PlayerPlanner,
    player_resources::{ObjectKey, Registry},
};

/// The transform of the object's root when the object is a prefab root: its transform has no
/// parent. `None` for a child object.
///
/// # Errors
/// Fails when the object or its transform does not decode, or the object has no transform.
pub(crate) fn root_transform(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<Option<i64>, PlayerError> {
    let (path, path_id) = key;
    let game_object: GameObject = files.open_path(path)?.decode(*path_id)?;
    for component in &game_object.components {
        let class = files
            .local(path, *component)?
            .file
            .object(component.path_id)
            .map(|object| object.class_id);
        if matches!(class, Some(ClassId::TRANSFORM | ClassId::RECT_TRANSFORM)) {
            let transform = files.transform(path, component.path_id)?;
            return Ok(transform.father.is_null().then_some(component.path_id));
        }
    }
    Err(invalid(
        path,
        format!("object {path_id} ({}) has no transform", game_object.name),
    ))
}

/// The name of the game object, or its place in its file when it does not decode.
pub(crate) fn prefab_name(files: &mut PlayerFiles, key: &ObjectKey) -> String {
    files
        .open_path(&key.0)
        .and_then(|open| open.decode::<GameObject>(key.1))
        .map_or_else(|_| format!("object {}", key.1), |object| object.name)
}

/// Plans the prefab whose root object is `key`.
///
/// # Errors
/// Fails when the object is not a prefab root, or an object of its hierarchy does not decode,
/// or a script component's fields do not decode with its layout.
pub(crate) fn plan_prefab(
    registry: &mut Registry,
    key: &ObjectKey,
    tags: &BTreeMap<u16, String>,
    layouts: &ScriptLayouts,
) -> Result<ScenePlan, PlayerError> {
    let name = prefab_name(&mut registry.files, key);
    let Some(root) = root_transform(&mut registry.files, key)? else {
        return Err(invalid(
            &key.0,
            format!("object {} ({name}) is not a prefab root", key.1),
        ));
    };
    let mut planner = PlayerPlanner::new(key.0.clone(), tags, true);
    planner.add_object(registry, root, None)?;
    let objects = planner.plan_objects(registry, layouts)?;
    Ok(ScenePlan {
        scene: name,
        objects,
        assets: Vec::new(),
        sprites: Vec::new(),
        textures: Vec::new(),
        fonts: Vec::new(),
        axes: Vec::new(),
        layers: Vec::new(),
        differences: planner.differences,
    })
}

/// Whether the object is a root, for a reference that names a game object.
pub(crate) fn is_root(files: &mut PlayerFiles, key: &ObjectKey) -> bool {
    matches!(root_transform(files, key), Ok(Some(_)))
}
