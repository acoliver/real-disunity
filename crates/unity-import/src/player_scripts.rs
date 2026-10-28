//! Script components of a player's serialized files, decoded with script layouts.

use std::{collections::BTreeMap, path::Path};

use worldrunner_unity_serialized::{
    ClassId,
    classes::MonoBehaviour,
    object::decode,
    script_data::{ScriptLayouts, Value, read_script},
};

use crate::{
    PlayerError,
    player_files::{PlayerFiles, ScriptClass},
};

/// How many failures of each cause [`script_census`] keeps as examples.
const EXAMPLES_KEPT: usize = 3;

/// Counts of a player's `MonoBehaviour` objects whose script fields decode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptCensus {
    pub objects: usize,
    pub decoded: usize,
    /// Decoded components with no script, such as assets whose script the build stripped.
    /// They have no script data.
    pub missing_scripts: usize,
    /// Failures by script class (or by cause, when the class is unknown): a count and the
    /// first errors, each naming its file and path ID.
    pub failures: BTreeMap<String, (usize, Vec<String>)>,
}

/// Decodes the script fields of every `MonoBehaviour` in the named files of a data directory.
///
/// # Errors
/// Fails when a file cannot be read or an object's header does not decode; script data that
/// does not decode is counted.
pub fn script_census(
    data_dir: &Path,
    files: &[String],
    layouts: &ScriptLayouts,
) -> Result<ScriptCensus, PlayerError> {
    let mut census = ScriptCensus::default();
    let mut player = PlayerFiles::new(data_dir);
    for name in files {
        let path = data_dir.join(name);
        let ids: Vec<i64> = player
            .open_path(&path)?
            .file
            .objects()
            .iter()
            .filter(|o| o.class_id == ClassId::MONO_BEHAVIOUR)
            .map(|o| o.path_id)
            .collect();
        for path_id in ids {
            census.objects += 1;
            match script_fields(&mut player, &path, path_id, layouts)? {
                Ok(data) => {
                    census.decoded += 1;
                    census.missing_scripts += usize::from(data.class.is_none());
                }
                Err((group, error)) => {
                    let entry = census.failures.entry(group).or_default();
                    entry.0 += 1;
                    if entry.1.len() < EXAMPLES_KEPT {
                        entry.1.push(format!("{name}:{path_id}: {error}"));
                    }
                }
            }
        }
    }
    Ok(census)
}

/// Why a script component's fields do not decode: a group (the script class key, or the
/// cause when there is no class) and the error.
pub(crate) type ScriptFailure = (String, String);

/// A script component's class and decoded fields. A component without a script has no
/// class and no fields.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptData {
    pub class: Option<ScriptClass>,
    /// The object's name: an asset's name, empty for most components.
    pub name: String,
    pub enabled: bool,
    pub fields: Vec<(String, Value)>,
}

/// A `MonoBehaviour`'s script fields, or why they do not decode.
///
/// # Errors
/// Fails when the file cannot be read or the object's header does not decode.
pub(crate) fn script_fields(
    player: &mut PlayerFiles,
    path: &Path,
    path_id: i64,
    layouts: &ScriptLayouts,
) -> Result<Result<ScriptData, ScriptFailure>, PlayerError> {
    let open = player.open_path(path)?;
    let object = *open
        .file
        .object(path_id)
        .ok_or_else(|| PlayerError::MissingObject {
            path: path.to_owned(),
            path_id,
        })?;
    let bytes = open.bytes(&object)?;
    let big_endian = open.file.header.big_endian;
    let behaviour: MonoBehaviour =
        decode(&bytes, &object, big_endian).map_err(|source| open.error(source))?;
    if behaviour.script.is_null() {
        let size = behaviour.script_data.len();
        return Ok(if size == 0 {
            Ok(ScriptData {
                class: None,
                name: behaviour.name,
                enabled: behaviour.enabled,
                fields: Vec::new(),
            })
        } else {
            Err((
                "no script".to_owned(),
                format!("{size} bytes of script data"),
            ))
        });
    }
    let class = match player.script_class(path, behaviour.script) {
        Ok(class) => class,
        Err(error) => return Ok(Err(("script reference".to_owned(), error.to_string()))),
    };
    let Some(root) = layouts.find(&class.key()) else {
        return Ok(Err((class.key(), "no layout".to_owned())));
    };
    let fields = read_script(
        &bytes,
        behaviour.script_data.start,
        big_endian,
        layouts,
        root,
    );
    Ok(match fields {
        Ok(fields) => Ok(ScriptData {
            class: Some(class),
            name: behaviour.name,
            enabled: behaviour.enabled,
            fields,
        }),
        Err(error) => Err((class.key(), error.to_string())),
    })
}
