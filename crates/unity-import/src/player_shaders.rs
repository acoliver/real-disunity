//! How a player's Vulkan shader modules fare in preparation for naga.

use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use worldrunner_unity_serialized::ClassId;

use crate::{
    PlayerError,
    player_objects::{OpenFile, ShaderModule, shader_modules, shader_platforms},
    shader_prep::{PrepareError, PreparedModule, check_with_naga, prepare},
};

/// Unity's `ShaderCompilerPlatform` for Vulkan.
const VULKAN: u32 = 18;

/// Counts over a player's distinct Vulkan stage modules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShaderReport {
    /// Shaders with a Vulkan platform.
    pub shaders: usize,
    /// Distinct stage modules.
    pub modules: usize,
    /// Modules that naga parses and validates after preparation.
    pub ready: usize,
    /// Ready modules in which combined image samplers were split.
    pub split: usize,
    /// Failing modules by cause.
    pub failures: BTreeMap<String, usize>,
    /// The first failure of each cause, naming its shader and module.
    pub examples: BTreeMap<String, String>,
}

impl ShaderReport {
    fn record(
        &mut self,
        outcome: Result<PreparedModule, PrepareError>,
        place: impl FnOnce() -> String,
    ) {
        match outcome {
            Ok(prepared) => {
                self.ready += 1;
                let split = prepared
                    .bindings
                    .iter()
                    .any(|b| b.sampler_binding.is_some());
                self.split += usize::from(split);
            }
            Err(error) => {
                let cause = error.cause();
                *self.failures.entry(cause.clone()).or_default() += 1;
                self.examples
                    .entry(cause)
                    .or_insert_with(|| format!("{}: {error}", place()));
            }
        }
    }
}

/// Prepares a module and checks it with naga.
fn prepare_and_check(module: &ShaderModule) -> Result<PreparedModule, PrepareError> {
    let prepared = prepare(&module.words)?;
    check_with_naga(&prepared.words)?;
    Ok(prepared)
}

/// Prepares every distinct Vulkan stage module of the shaders in `paths` and checks each with
/// naga. A module that several shaders or variants share is counted once.
///
/// # Errors
/// Fails if a file cannot be read, or a shader's modules cannot be decoded; a module that
/// decodes but does not prepare is counted, not an error.
pub fn shader_report(paths: &[PathBuf]) -> Result<ShaderReport, PlayerError> {
    let mut report = ShaderReport::default();
    let mut seen = HashSet::new();
    for path in paths {
        let mut open = OpenFile::open(path)?;
        let shaders: Vec<i64> = open
            .file
            .objects()
            .iter()
            .filter(|object| object.class_id == ClassId::SHADER)
            .map(|object| object.path_id)
            .collect();
        for path_id in shaders {
            let platforms = shader_platforms(&mut open, path_id)?;
            let Some(platform) = platforms.iter().position(|&kind| kind == VULKAN) else {
                continue;
            };
            report.shaders += 1;
            for module in shader_modules(&mut open, path_id, platform)? {
                if seen.insert(module.words.clone()) {
                    report.modules += 1;
                    let place = || {
                        let at = format!("{}:{path_id}", path.display());
                        format!("{at} {}.{}", module.blob_index, module.slot)
                    };
                    report.record(prepare_and_check(&module), place);
                }
            }
        }
    }
    Ok(report)
}
