//! Loads every shader of a player through the adapter's loader and counts what happened.
//!
//! The report holds counts only: how many shader objects the serialized files hold, how many
//! became a program, how many failed and by which error, and how many passes the programs have.

use std::collections::BTreeMap;
use std::path::Path;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::ProgramRuntime;
use worldrunner_render_program::unity::compute_kernel::kernel_stage;
use worldrunner_unity_import::{PlayerError, player::is_serialized_name, player_objects::OpenFile};
use worldrunner_unity_serialized::ClassId;
use worldrunner_unity_serialized::compute_shader::{ComputeShader, SPIRV_RENDERER};
use worldrunner_world_simulation::graphics::ShaderSource;

use super::compute::program_cause;
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
    /// For each 2D texture property, the shaders that declare it by the name of the default they
    /// give it (empty for none).
    pub declared_defaults: BTreeMap<String, BTreeMap<String, usize>>,
}

/// The tag a pass is selected by.
const LIGHT_MODE: &str = "LightMode";

/// # Errors
/// Fails when the data directory or one of its serialized files cannot be read as a file.
pub fn shader_census(data_directory: &Path) -> Result<ShaderCensus, PlayerError> {
    let mut census = ShaderCensus::default();
    let mut loader = ShaderLoader::new(data_directory);
    for name in serialized_names(data_directory)? {
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
                    for (property, default) in program.declared_defaults {
                        *census
                            .declared_defaults
                            .entry(property)
                            .or_default()
                            .entry(default)
                            .or_default() += 1;
                    }
                }
                Err(error) => *census.failed.entry(error.name()).or_default() += 1,
            }
        }
    }
    Ok(census)
}

/// What building a player's compute kernels on a device came to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputeCensus {
    /// Compute shader objects in the serialized files.
    pub shaders: usize,
    /// Variants of the SPIR-V platforms' kernels.
    pub variants: usize,
    /// Variants that became a stage program and a compute program on the device.
    pub programs: usize,
    /// The variants that did not, by the name of the cause a dispatch of them is left out for.
    pub failed: BTreeMap<&'static str, usize>,
}

/// Builds every variant of every compute shader of the player as a compute program on
/// `runtime`'s device, which says what the device cannot hold (a kernel that binds more storage
/// images than it allows, say) where the CPU-only `player-compute-report` cannot.
///
/// # Errors
/// Fails when the data directory or one of its serialized files cannot be read as a file.
pub fn compute_census(
    data_directory: &Path,
    runtime: &ProgramRuntime,
) -> Result<ComputeCensus, PlayerError> {
    let mut census = ComputeCensus::default();
    for name in serialized_names(data_directory)? {
        let mut open = OpenFile::open(&data_directory.join(&name))?;
        let shaders: Vec<i64> = open
            .file
            .objects()
            .iter()
            .filter(|object| object.class_id == ClassId::COMPUTE_SHADER)
            .map(|object| object.path_id)
            .collect();
        for path_id in shaders {
            census.shaders += 1;
            let Ok(shader) = open.decode::<ComputeShader>(path_id) else {
                *census.failed.entry("Unreadable").or_default() += 1;
                continue;
            };
            let Some(platform) = shader.platform(SPIRV_RENDERER) else {
                continue;
            };
            for variant in platform.kernels.iter().flat_map(|kernel| &kernel.variants) {
                census.variants += 1;
                let built = kernel_stage(variant, &platform.constant_buffers)
                    .map_err(|_| "Program")
                    .and_then(|stage| {
                        stage
                            .neutral()
                            .map_err(ProgramError::from)
                            .and_then(|neutral| runtime.compute_program(&neutral))
                            .map_err(|error| program_cause(&error))
                    });
                match built {
                    Ok(_) => census.programs += 1,
                    Err(cause) => *census.failed.entry(cause).or_default() += 1,
                }
            }
        }
    }
    Ok(census)
}

/// The names of the serialized files directly inside `data_directory`, sorted.
fn serialized_names(data_directory: &Path) -> Result<Vec<String>, PlayerError> {
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
    Ok(names)
}
