//! How a player's Vulkan shader modules fare in preparation for naga.

use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use worldrunner_unity_serialized::ClassId;

use crate::{
    PlayerError,
    player_objects::{OpenFile, ShaderModule, shader_modules, shader_platforms},
    shader_prep::{Backend, NagaModule, PrepareError, PreparedModule, check_with_naga, prepare},
};

/// Unity's `ShaderCompilerPlatform` for Vulkan.
pub const VULKAN: u32 = 18;

/// Tells the Vulkan platform of each shader of a player, and whether the player has any.
/// Every walk that collects a player's Vulkan programs passes its shaders through one census, so
/// that a player whose shaders have none is named, not read as having no programs.
#[derive(Debug, Default)]
pub(crate) struct VulkanCensus {
    shaders: usize,
    vulkan: usize,
}

impl VulkanCensus {
    /// Counts a shader that lists `platforms`, and gives the index of its Vulkan platform, if any.
    pub(crate) fn platform(&mut self, platforms: &[u32]) -> Option<usize> {
        self.shaders += 1;
        let platform = platforms.iter().position(|&kind| kind == VULKAN);
        self.vulkan += usize::from(platform.is_some());
        platform
    }

    /// Ends the walk.
    ///
    /// # Errors
    /// Fails if the player holds shaders and none has a Vulkan platform. A player without shaders
    /// is not an error.
    pub(crate) fn finish(self) -> Result<(), PlayerError> {
        if self.shaders > 0 && self.vulkan == 0 {
            return Err(PlayerError::NoSpirvPrograms {
                shaders: self.shaders,
            });
        }
        Ok(())
    }
}

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
    /// Ready modules that naga writes for each backend.
    pub written: BTreeMap<Backend, usize>,
    /// Failures by cause: modules that are not ready, and ready modules a backend cannot be
    /// written for, whose causes name the backend.
    pub failures: BTreeMap<String, usize>,
    /// The first failure of each cause, naming its shader and module.
    pub examples: BTreeMap<String, String>,
}

impl ShaderReport {
    fn record(&mut self, module: &ShaderModule, place: &dyn Fn() -> String) {
        let (prepared, naga) = match prepare_and_check(module) {
            Ok(checked) => checked,
            Err(error) => return self.fail(&error, place),
        };
        self.ready += 1;
        let split = prepared
            .bindings
            .iter()
            .any(|b| b.sampler_binding.is_some());
        self.split += usize::from(split);
        for backend in Backend::ALL {
            match backend.write(&naga) {
                Ok(()) => *self.written.entry(backend).or_default() += 1,
                Err(error) => self.fail(&error, place),
            }
        }
    }

    fn fail(&mut self, error: &PrepareError, place: &dyn Fn() -> String) {
        let cause = error.cause();
        *self.failures.entry(cause.clone()).or_default() += 1;
        self.examples
            .entry(cause)
            .or_insert_with(|| format!("{}: {error}", place()));
    }
}

/// Prepares a module and checks it with naga.
fn prepare_and_check(module: &ShaderModule) -> Result<(PreparedModule, NagaModule), PrepareError> {
    let prepared = prepare(&module.words)?;
    let naga = check_with_naga(&prepared.words)?;
    Ok((prepared, naga))
}

/// Prepares every distinct Vulkan stage module of the shaders in `paths`, checks each with
/// naga and writes each that passes for every [`Backend`]. A module that several shaders or
/// variants share is counted once.
///
/// # Errors
/// Fails if a file cannot be read, a shader's modules cannot be decoded, or the player has
/// shaders and none with a Vulkan platform; a module that decodes but does not prepare is
/// counted, not an error.
pub fn shader_report(paths: &[PathBuf]) -> Result<ShaderReport, PlayerError> {
    let mut report = ShaderReport::default();
    let mut census = VulkanCensus::default();
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
            let Some(platform) = census.platform(&platforms) else {
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
                    report.record(&module, &place);
                }
            }
        }
    }
    census.finish()?;
    Ok(report)
}
