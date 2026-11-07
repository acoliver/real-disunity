//! Script assets and fonts a player scene's script fields reference, as scene plan assets.
//!
//! A referenced `ScriptableObject` plans once, with its class, its assembly and its own fields,
//! which may name other assets or the asset itself. A script component of a game object is
//! not an asset, so a reference to one binds null.

use serde_json::Value as Json;
use worldrunner_unity_serialized::object::PPtr;

use crate::{
    plan::PlanAsset,
    player_plan::PlanSource,
    player_resources::{FontRange, ObjectKey, font_range},
    player_scripts::{Referenced, referenced_script, script_fields},
    player_shader_refs::SceneSource,
    player_values::{FieldWriter, not_held},
};

/// The class of an asset whose script the build stripped, which loads as a plain
/// `ScriptableObject` holding no data.
const PLAIN_CLASS: &str = "UnityEngine.ScriptableObject";
const PLAIN_ASSEMBLY: &str = "UnityEngine.CoreModule";

impl SceneSource {
    /// The index of the script asset `pptr` names, planning it and the assets its fields name
    /// when they are new, or why the scene does not hold it.
    pub(crate) fn script_asset(&mut self, key: &ObjectKey, pptr: PPtr) -> Result<usize, String> {
        if let Some((_, index)) = self.planned.iter().find(|(known, _)| known == key) {
            return Ok(*index);
        }
        let (file, path_id) = key;
        let layouts = self.layouts.clone();
        match referenced_script(self.files_mut(), file, *path_id, &layouts)
            .map_err(|cause| cause.to_string())?
        {
            Referenced::Asset => {}
            Referenced::Component => return Err(not_held(pptr)),
            Referenced::Unreadable(cause) => {
                return Err(format!(
                    "references object {} of file {}, a script asset that does not decode: {cause}",
                    pptr.path_id, pptr.file_id
                ));
            }
        }
        let data = script_fields(self.files_mut(), file, *path_id, &layouts)
            .map_err(|cause| cause.to_string())?
            .map_err(|(script, reason)| format!("{script}: {reason}"))?;
        let (class, assembly) = data.class.map_or_else(
            || (PLAIN_CLASS.to_owned(), PLAIN_ASSEMBLY.to_owned()),
            |class| (class.full_name, class.assembly),
        );
        // Planned before its fields, so assets that name each other end the walk.
        let index = self.push_asset(
            key.clone(),
            PlanAsset::Script {
                script: String::new(),
                class: class.clone(),
                assembly: Some(assembly.clone()),
                name: data.name.clone(),
                fields: Json::Null,
            },
        );
        let mut found = Vec::new();
        let fields = FieldWriter::new(|reference| self.outside(file, reference), &mut found)
            .fields(&format!("asset {}", data.name), &data.fields);
        self.differences.append(&mut found);
        self.assets[index] = PlanAsset::Script {
            script: String::new(),
            class,
            assembly: Some(assembly),
            name: data.name,
            fields,
        };
        Ok(index)
    }
}

impl SceneSource {
    /// The index of the font `pptr` names, planned with the range of its font file when it is
    /// new, or why the scene does not hold it.
    pub(crate) fn font_asset(&mut self, key: &ObjectKey, pptr: PPtr) -> Result<usize, String> {
        if let Some((_, index)) = self.planned.iter().find(|(known, _)| known == key) {
            return Ok(*index);
        }
        let FontRange {
            name,
            file,
            offset,
            length,
        } = font_range(self.files_mut(), &key.0, key.1).map_err(|cause| {
            format!(
                "references font object {} of file {}, which did not plan: {cause}",
                pptr.path_id, pptr.file_id
            )
        })?;
        Ok(self.push_asset(
            key.clone(),
            PlanAsset::Font {
                name,
                file,
                offset,
                length,
            },
        ))
    }
}
