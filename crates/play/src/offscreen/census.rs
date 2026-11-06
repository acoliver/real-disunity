//! Loads every shader of a player through the adapter's loader and counts what happened.
//!
//! The report holds counts only: how many shader objects the serialized files hold, how many
//! became a program, how many failed and by which error, and how many passes the programs have.

use std::collections::BTreeMap;
use std::path::Path;

use worldrunner_unity_import::{PlayerError, player::is_serialized_name, player_objects::OpenFile};
use worldrunner_unity_serialized::ClassId;
use worldrunner_world_simulation::graphics::ShaderSource;

use super::shaders::ShaderLoader;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShaderCensus {
    /// Shader objects in the serialized files.
    pub shaders: usize,
    /// Shaders that became a program.
    pub loaded: usize,
    /// Shaders that did not, by the name of the error.
    pub failed: BTreeMap<&'static str, usize>,
    /// Passes of the first sub-shader, summed over the loaded shaders.
    pub passes: usize,
    /// Those passes by the value of their `LightMode` tag, `(none)` for a pass without one.
    pub light_modes: BTreeMap<String, usize>,
    /// Loaded shaders none of whose passes has a `LightMode` tag.
    pub shaders_without_light_mode: usize,
}

/// The tag a pass is selected by.
const LIGHT_MODE: &str = "LightMode";

/// # Errors
/// Fails when the data directory or one of its serialized files cannot be read as a file.
pub fn shader_census(data_directory: &Path) -> Result<ShaderCensus, PlayerError> {
    let mut census = ShaderCensus::default();
    let mut loader = ShaderLoader::new(data_directory);
    let entries = std::fs::read_dir(data_directory).map_err(|source| PlayerError::Io {
        path: data_directory.to_owned(),
        source,
    })?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_serialized_name(name))
        .collect();
    names.sort();
    for name in names {
        let open = OpenFile::open(&data_directory.join(&name))?;
        let shaders: Vec<i64> = open
            .file
            .objects()
            .iter()
            .filter(|object| object.class_id == ClassId::SHADER)
            .map(|object| object.path_id)
            .collect();
        for path_id in shaders {
            census.shaders += 1;
            let source = ShaderSource {
                file: name.clone(),
                path_id,
            };
            match loader.load(&source) {
                Ok(program) => {
                    census.loaded += 1;
                    census.passes += program.passes;
                    let mut tagged = false;
                    for pass in 0..program.passes {
                        let mode = program.program.pass_tag(pass, LIGHT_MODE);
                        tagged |= mode.is_some();
                        *census
                            .light_modes
                            .entry(mode.unwrap_or_else(|| "(none)".to_owned()))
                            .or_default() += 1;
                    }
                    census.shaders_without_light_mode += usize::from(!tagged);
                }
                Err(error) => *census.failed.entry(error.name()).or_default() += 1,
            }
        }
    }
    Ok(census)
}
