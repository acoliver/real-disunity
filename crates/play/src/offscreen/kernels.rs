//! Loads the stage program of a compute kernel from the player's serialized files.
//!
//! A world compute shader's source names a serialized file, relative to the player's data
//! directory with `/` separators, and the path id of the compute shader object in it. The loader
//! opens the file and decodes the object through the importer's readers. A kernel is the one at its
//! index in the list of the SPIR-V platform, whose kernels are those the world lists, and it
//! runs the variant compiled for no keywords: the world gives a dispatch no keywords. The decoded
//! object is kept by source, so a second kernel of the same shader is built from the same decode,
//! and a source that failed is not read again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use worldrunner_render_program::unity::compute_kernel::kernel_stage;
use worldrunner_render_program::unity::module::StageProgram;
use worldrunner_unity_import::player_objects::OpenFile;
use worldrunner_unity_serialized::compute_shader::{ComputeShader, SPIRV_RENDERER};
use worldrunner_world_simulation::graphics::{ShaderSource, WorldComputeShader};

use super::compute::{ComputeLoadError, ComputeSupply};
use super::shaders::data_path;

/// Loads compute kernels from one player's data directory and keeps what it decoded.
pub struct PlayerKernels {
    data_directory: PathBuf,
    decoded: BTreeMap<(String, i64), Result<Rc<ComputeShader>, ComputeLoadError>>,
}

impl PlayerKernels {
    #[must_use]
    pub fn new(data_directory: impl Into<PathBuf>) -> Self {
        Self {
            data_directory: data_directory.into(),
            decoded: BTreeMap::new(),
        }
    }

    fn shader(&mut self, source: &ShaderSource) -> Result<Rc<ComputeShader>, ComputeLoadError> {
        let key = (source.file.clone(), source.path_id);
        self.decoded
            .entry(key)
            .or_insert_with(|| decode(&self.data_directory, source))
            .clone()
    }
}

fn decode(directory: &Path, source: &ShaderSource) -> Result<Rc<ComputeShader>, ComputeLoadError> {
    let unreadable = |reason: String| ComputeLoadError::Unreadable(reason);
    let path = data_path(directory, &source.file).ok_or_else(|| {
        unreadable(format!(
            "the shader file {:?} is not a relative path inside the data directory",
            source.file
        ))
    })?;
    let mut open = OpenFile::open(&path).map_err(|error| unreadable(error.to_string()))?;
    open.decode::<ComputeShader>(source.path_id)
        .map(Rc::new)
        .map_err(|error| unreadable(error.to_string()))
}

impl ComputeSupply for PlayerKernels {
    fn describe(&mut self, shader: &WorldComputeShader, kernel: u32) -> String {
        let Ok(decoded) = self.shader(&shader.source) else {
            return String::new();
        };
        let found = decoded
            .platform(SPIRV_RENDERER)
            .and_then(|platform| platform.kernels.get(kernel as usize));
        let Some(found) = found else {
            return String::new();
        };
        match found.variant_for(&[]) {
            Some(variant) => {
                let index = found
                    .variants
                    .iter()
                    .position(|held| std::ptr::eq(held, variant))
                    .unwrap_or_default();
                format!(
                    "variant {index} of {} for no keywords, thread group size {:?}",
                    found.variants.len(),
                    variant.thread_group_size
                )
            }
            None => format!(
                "none of {} variants is for no keywords",
                found.variants.len()
            ),
        }
    }

    fn load(
        &mut self,
        shader: &WorldComputeShader,
        kernel: u32,
    ) -> Result<StageProgram, ComputeLoadError> {
        let decoded = self.shader(&shader.source)?;
        let platform = decoded
            .platform(SPIRV_RENDERER)
            .ok_or(ComputeLoadError::NoCompiledCode)?;
        let found = usize::try_from(kernel)
            .ok()
            .and_then(|index| platform.kernels.get(index))
            .ok_or(ComputeLoadError::NoSuchKernel(kernel))?;
        let listed = usize::try_from(kernel)
            .ok()
            .and_then(|index| shader.kernels.get(index));
        if listed.is_none_or(|listed| listed.name != found.name) {
            return Err(ComputeLoadError::Unreadable(
                "the object's kernel is not the one the world lists at that index".to_owned(),
            ));
        }
        let variant = found.variant_for(&[]).ok_or(ComputeLoadError::NoVariant)?;
        kernel_stage(variant, &platform.constant_buffers)
            .map_err(|error| ComputeLoadError::Program(error.to_string()))
    }
}
