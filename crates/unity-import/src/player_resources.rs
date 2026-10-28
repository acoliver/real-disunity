//! A player's `Resources` as the managed engine loads them.
//!
//! [`plan_player_resources`] lists every path of the player's resource container with the
//! class of the object it names. Objects of classes the engine can build become plan
//! assets: a `TextAsset` names the byte range of its contents in the player's file, and a
//! script asset (a `ScriptableObject`) has its decoded fields written to a file beside the
//! plan, with references to other planned resources as `{"$asset": i}`, since a few hold
//! tables of many megabytes. Entries of other classes keep only their class, so loading one
//! names the class the engine does not build yet.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::{Value as Json, json};
use worldrunner_unity_serialized::{
    ClassId, managers::ResourceManager, object::PPtr, script_data::ScriptLayouts,
    text_asset::TextAsset,
};

use crate::{
    PlayerError,
    player_files::PlayerFiles,
    player_scripts::script_fields,
    player_values::{FieldWriter, not_held},
};

#[derive(Debug, Serialize)]
pub struct ResourcesPlan {
    /// Container entries in container order, except those that name no object. Paths are
    /// lower case, without extensions.
    pub entries: Vec<ResourceEntry>,
    pub assets: Vec<ResourceAsset>,
    pub differences: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResourceEntry {
    pub path: String,
    /// The Unity class of the object, such as `TextAsset`.
    pub class: String,
    /// The planned asset, when the engine builds this class.
    pub asset: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ResourceAsset {
    /// The contents are `length` bytes at `offset` in the player file `file`.
    TextAsset {
        name: String,
        file: PathBuf,
        offset: u64,
        length: u64,
    },
    /// A script asset of `class` in `assembly`, with its fields in the JSON file `fields`,
    /// relative to the plan's directory.
    Script {
        class: String,
        assembly: String,
        name: String,
        fields: String,
    },
}

/// An object of a player file, by file path and path ID.
type ObjectKey = (PathBuf, i64);

/// Plans the resources of the player in `data_dir`, writing script asset fields under
/// `output/resources`. Script asset fields decode with `layouts`.
///
/// # Errors
/// Fails when a file cannot be read or written, the resource manager or a text asset does
/// not decode, or a script asset's fields do not decode with its layout.
pub fn plan_player_resources(
    data_dir: &Path,
    layouts: &ScriptLayouts,
    output: &Path,
) -> Result<ResourcesPlan, PlayerError> {
    let mut files = PlayerFiles::new(data_dir);
    let managers = data_dir.join("globalgamemanagers");
    let container = resource_container(&mut files, &managers)?;
    let mut planner = ResourcePlanner {
        files,
        output: output.to_owned(),
        planned: BTreeMap::new(),
        assets: Vec::new(),
        differences: Vec::new(),
    };
    let mut entries = Vec::with_capacity(container.len());
    // Planned objects in asset order, so references between them resolve before any is planned.
    let mut queue = Vec::new();
    for (path, pptr) in container {
        if pptr.is_null() {
            planner
                .differences
                .push(format!("resource {path} names no object"));
            continue;
        }
        let file = planner.files.referenced_file(&managers, pptr)?;
        let class = planner.class(&file, pptr.path_id)?;
        let key = (file, pptr.path_id);
        if buildable(class) && !planner.planned.contains_key(&key) {
            planner.planned.insert(key.clone(), queue.len());
            queue.push((key.clone(), class));
        }
        entries.push(ResourceEntry {
            path,
            class: class.to_string(),
            asset: planner.planned.get(&key).copied(),
        });
    }
    for (key, class) in queue {
        let asset = planner.plan_asset(&key, class, layouts)?;
        planner.assets.push(asset);
    }
    Ok(ResourcesPlan {
        entries,
        assets: planner.assets,
        differences: planner.differences,
    })
}

fn buildable(class: ClassId) -> bool {
    class == ClassId::TEXT_ASSET || class == ClassId::MONO_BEHAVIOUR
}

fn resource_container(
    files: &mut PlayerFiles,
    managers: &Path,
) -> Result<Vec<(String, PPtr)>, PlayerError> {
    let open = files.open_path(managers)?;
    let Some(path_id) = open
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::RESOURCE_MANAGER)
        .map(|object| object.path_id)
    else {
        return Ok(Vec::new());
    };
    let manager: ResourceManager = open.decode(path_id)?;
    Ok(manager.container)
}

struct ResourcePlanner {
    files: PlayerFiles,
    output: PathBuf,
    /// The asset index of each planned object.
    planned: BTreeMap<ObjectKey, usize>,
    assets: Vec<ResourceAsset>,
    differences: Vec<String>,
}

impl ResourcePlanner {
    fn class(&mut self, file: &Path, path_id: i64) -> Result<ClassId, PlayerError> {
        self.files
            .open_path(file)?
            .file
            .object(path_id)
            .map(|object| object.class_id)
            .ok_or_else(|| PlayerError::MissingObject {
                path: file.to_owned(),
                path_id,
            })
    }

    fn plan_asset(
        &mut self,
        key: &ObjectKey,
        class: ClassId,
        layouts: &ScriptLayouts,
    ) -> Result<ResourceAsset, PlayerError> {
        let (file, path_id) = key;
        if class == ClassId::TEXT_ASSET {
            return self.text_asset(file, *path_id);
        }
        let data = script_fields(&mut self.files, file, *path_id, layouts)?.map_err(
            |(script, reason)| PlayerError::InvalidHierarchy {
                path: file.clone(),
                reason: format!("resource object {path_id}: script {script}: {reason}"),
            },
        )?;
        let Some(script) = data.class else {
            // A script asset whose script the build stripped holds no data; Unity loads it
            // as a plain ScriptableObject.
            return Ok(ResourceAsset::Script {
                class: "UnityEngine.ScriptableObject".to_owned(),
                assembly: "UnityEngine.CoreModule".to_owned(),
                name: data.name,
                fields: self.write_fields(&json!({}))?,
            });
        };
        let place = format!("resource {}", data.name);
        let planned = &self.planned;
        let files = &mut self.files;
        let fields = FieldWriter::new(
            |pptr| resource_reference(files, planned, file, pptr),
            &mut self.differences,
        )
        .fields(&place, &data.fields);
        Ok(ResourceAsset::Script {
            class: script.full_name,
            assembly: script.assembly,
            name: data.name,
            fields: self.write_fields(&fields)?,
        })
    }

    /// Writes the fields of the asset being planned and returns the file's relative path.
    fn write_fields(&self, fields: &Json) -> Result<String, PlayerError> {
        let relative = format!("resources/{}.json", self.assets.len());
        let target = self.output.join(&relative);
        let io = |path: &Path| {
            let path = path.to_owned();
            move |source| PlayerError::Io { path, source }
        };
        let parent = target.parent().expect("resource files have a directory");
        std::fs::create_dir_all(parent).map_err(io(parent))?;
        let bytes = serde_json::to_vec(fields).expect("JSON values serialize");
        std::fs::write(&target, bytes).map_err(io(&target))?;
        Ok(relative)
    }

    fn text_asset(&mut self, file: &Path, path_id: i64) -> Result<ResourceAsset, PlayerError> {
        let open = self.files.open_path(file)?;
        let asset: TextAsset = open.decode(path_id)?;
        let object = *open.file.object(path_id).expect("decoded objects exist");
        let at = |position: usize| object.offset + position as u64;
        Ok(ResourceAsset::TextAsset {
            name: asset.name,
            file: std::path::absolute(file).map_err(|error| PlayerError::Io {
                path: file.to_owned(),
                source: error,
            })?,
            offset: at(asset.contents.start),
            length: (asset.contents.end - asset.contents.start) as u64,
        })
    }
}

/// `{"$asset": i}` for a reference to a planned resource, or why there is none.
fn resource_reference(
    files: &mut PlayerFiles,
    planned: &BTreeMap<ObjectKey, usize>,
    owner: &Path,
    pptr: PPtr,
) -> Result<Json, String> {
    let index = files
        .referenced_file(owner, pptr)
        .ok()
        .and_then(|file| planned.get(&(file, pptr.path_id)));
    index
        .map(|index| json!({ "$asset": index }))
        .ok_or_else(|| not_held(pptr))
}
