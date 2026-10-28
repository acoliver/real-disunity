//! A built scene of a player as the managed loader builds it: the same [`ScenePlan`] a
//! project scene gives, from the player's serialized files and script layouts.
//!
//! A project plan fails on anything the engine cannot build. A player scene is planned in
//! full instead: an engine component without a plan entry becomes
//! [`PlanComponent::Unsupported`], and a script field that references something the plan
//! does not hold is planned as `null`, each listed in [`ScenePlan::differences`]. Running the
//! scene then shows which of them the game uses first.

mod values;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use worldrunner_unity_serialized::{
    ClassId,
    classes::{GameObject, Transform},
    managers::TagManager,
    object::PPtr,
    script_data::ScriptLayouts,
};

use crate::{
    PlayerError,
    components::RectTransform,
    plan::{PlanComponent, PlanObject, ScenePlan},
    player_files::PlayerFiles,
    player_scripts::script_fields,
};

/// Unity's built-in tags by index; user tags start at [`FIRST_USER_TAG`].
const BUILTIN_TAGS: [(u16, &str); 7] = [
    (0, "Untagged"),
    (1, "Respawn"),
    (2, "Finish"),
    (3, "EditorOnly"),
    (5, "MainCamera"),
    (6, "Player"),
    (7, "GameController"),
];
const FIRST_USER_TAG: u16 = 20_000;

/// Plans scene file `scene` (such as `level0`) of the player in `data_dir`, decoding script
/// fields with `layouts`.
///
/// # Errors
/// Fails when a file cannot be read, an object of the hierarchy does not decode, or a script
/// component's fields do not decode with its layout.
pub fn plan_player_scene(
    data_dir: &Path,
    scene: &str,
    layouts: &ScriptLayouts,
) -> Result<ScenePlan, PlayerError> {
    let mut planner = PlayerPlanner {
        files: PlayerFiles::new(data_dir),
        path: data_dir.join(scene),
        tags: tag_names(data_dir)?,
        objects: Vec::new(),
        index: BTreeMap::new(),
        differences: Vec::new(),
    };
    planner.files.open_path(&planner.path)?;
    for root in planner.files.roots(&planner.path)? {
        planner.add_object(root, None)?;
    }
    let mut objects = Vec::with_capacity(planner.objects.len());
    for i in 0..planner.objects.len() {
        objects.push(planner.plan_object(i, layouts)?);
    }
    Ok(ScenePlan {
        scene: scene.to_owned(),
        objects,
        assets: Vec::new(),
        sprites: Vec::new(),
        textures: Vec::new(),
        fonts: Vec::new(),
        axes: Vec::new(),
        differences: planner.differences,
    })
}

/// The tag names by tag index: Unity's built-in tags and the user tags of the player's tag
/// manager, when it has `globalgamemanagers`.
fn tag_names(data_dir: &Path) -> Result<BTreeMap<u16, String>, PlayerError> {
    let mut names: BTreeMap<u16, String> = BUILTIN_TAGS
        .iter()
        .map(|&(index, name)| (index, name.to_owned()))
        .collect();
    let path = data_dir.join("globalgamemanagers");
    if !path.exists() {
        return Ok(names);
    }
    let mut managers = crate::player_objects::OpenFile::open(&path)?;
    let tag_manager = managers
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::TAG_MANAGER)
        .map(|object| object.path_id);
    if let Some(path_id) = tag_manager {
        let manager: TagManager = managers.decode(path_id)?;
        for (offset, tag) in (FIRST_USER_TAG..).zip(manager.tags) {
            names.insert(offset, tag);
        }
    }
    Ok(names)
}

/// A scene object found while walking the hierarchy.
struct Found {
    game_object: GameObject,
    transform: Transform,
    transform_id: i64,
    parent: Option<usize>,
}

/// What a same-file reference to an object of the scene resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Object(usize),
    Transform(usize),
    Component(usize, usize),
}

struct PlayerPlanner {
    files: PlayerFiles,
    path: PathBuf,
    tags: BTreeMap<u16, String>,
    objects: Vec<Found>,
    /// The plan entry each scene object's path ID resolves to.
    index: BTreeMap<i64, Target>,
    differences: Vec<String>,
}

impl PlayerPlanner {
    /// Adds an object and its children, depth first, parents before children.
    fn add_object(&mut self, transform_id: i64, parent: Option<usize>) -> Result<(), PlayerError> {
        let transform = self.files.transform(&self.path, transform_id)?;
        let game_object: GameObject = self
            .files
            .local(&self.path, transform.game_object)?
            .decode(transform.game_object.path_id)?;
        let at = self.objects.len();
        self.index
            .insert(transform.game_object.path_id, Target::Object(at));
        self.index.insert(transform_id, Target::Transform(at));
        let mut planned = 0;
        for component in &game_object.components {
            if component.path_id != transform_id {
                self.index
                    .insert(component.path_id, Target::Component(at, planned));
                planned += 1;
            }
        }
        let children: Vec<i64> = transform.children.iter().map(|c| c.path_id).collect();
        self.objects.push(Found {
            game_object,
            transform,
            transform_id,
            parent,
        });
        for child in children {
            self.add_object(child, Some(at))?;
        }
        Ok(())
    }

    fn plan_object(
        &mut self,
        i: usize,
        layouts: &ScriptLayouts,
    ) -> Result<PlanObject, PlayerError> {
        let found = &self.objects[i];
        let game_object = found.game_object.clone();
        let transform = found.transform.clone();
        let transform_id = found.transform_id;
        let parent = found.parent;
        let tag = self.tags.get(&game_object.tag).cloned().unwrap_or_else(|| {
            self.differences.push(format!(
                "{}: tag {} is not defined",
                game_object.name, game_object.tag
            ));
            "Untagged".to_owned()
        });
        let mut components = Vec::new();
        for component in &game_object.components {
            if component.path_id != transform_id {
                components.push(self.component(&game_object.name, *component, layouts)?);
            }
        }
        Ok(PlanObject {
            name: game_object.name,
            tag,
            layer: game_object.layer.into(),
            active: game_object.active,
            parent,
            template: false,
            position: transform.local_position.map(f64::from),
            rotation: transform.local_rotation.map(f64::from),
            scale: transform.local_scale.map(f64::from),
            rect: transform.rect.map(|rect| RectTransform {
                anchor_min: rect.anchor_min.map(f64::from),
                anchor_max: rect.anchor_max.map(f64::from),
                anchored_position: rect.anchored_position.map(f64::from),
                size_delta: rect.size_delta.map(f64::from),
                pivot: rect.pivot.map(f64::from),
            }),
            components,
        })
    }

    fn component(
        &mut self,
        owner: &str,
        component: PPtr,
        layouts: &ScriptLayouts,
    ) -> Result<PlanComponent, PlayerError> {
        let path = self.path.clone();
        let class = self
            .files
            .local(&path, component)?
            .file
            .object(component.path_id)
            .map(|object| object.class_id)
            .ok_or_else(|| PlayerError::MissingObject {
                path: path.clone(),
                path_id: component.path_id,
            })?;
        if class != ClassId::MONO_BEHAVIOUR {
            return Ok(PlanComponent::Unsupported {
                class: class.to_string(),
            });
        }
        let data = script_fields(&mut self.files, &path, component.path_id, layouts)?.map_err(
            |(script, reason)| PlayerError::InvalidHierarchy {
                path: path.clone(),
                reason: format!("{owner}: script {script}: {reason}"),
            },
        )?;
        let Some(script) = data.class else {
            self.differences.push(format!(
                "{owner}: a script component without a script is not built"
            ));
            return Ok(PlanComponent::Unsupported {
                class: "MonoBehaviour".to_owned(),
            });
        };
        let place = format!("{owner}.{}", script.full_name);
        let fields = self.fields(&place, &data.fields);
        Ok(PlanComponent::Script {
            script: String::new(),
            class: script.full_name,
            assembly: Some(script.assembly),
            enabled: data.enabled,
            fields,
        })
    }
}
