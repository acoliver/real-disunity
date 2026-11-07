//! Loads the program of a world shader from the player's serialized files.
//!
//! A world shader's source names a serialized file, relative to the player's data directory with
//! `/` separators, and the path id of the shader object in it. The loader opens the file, decodes
//! the object and takes the chunks of its Vulkan platform, through the importer's readers, and
//! builds the [`ProgramShader`] that render-program resolves passes and variants from. The
//! decoded shader and its chunks are kept by source, so a second world shader with the same
//! source is built from the same decode, and a source that failed is not read again.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use worldrunner_render_program::unity::shader::{ProgramShader, ResolveError};
use worldrunner_unity_import::{
    PlayerError,
    player_objects::{OpenFile, VulkanShader, vulkan_shader},
    player_shader_refs::shader_render_queue,
};
use worldrunner_unity_serialized::shader::Shader;
use worldrunner_world_simulation::graphics::ShaderSource;

use super::defaults::{DefaultTexture, declared, declared_names};

#[derive(Clone, Debug, thiserror::Error)]
pub enum ShaderLoadError {
    #[error("the shader file {file:?} is not a relative path inside the data directory")]
    OutsideDataDirectory { file: String },
    #[error("{0}")]
    Read(String),
    #[error("the shader has no Vulkan platform")]
    NoVulkanPlatform,
    #[error("the shader's render queue cannot be read: {0}")]
    Queue(String),
    #[error("the shader's programs cannot be read: {0}")]
    Programs(String),
}

impl ShaderLoadError {
    /// The name of the variant, for counts by cause.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::OutsideDataDirectory { .. } => "OutsideDataDirectory",
            Self::Read(_) => "Read",
            Self::NoVulkanPlatform => "NoVulkanPlatform",
            Self::Queue(_) => "Queue",
            Self::Programs(_) => "Programs",
        }
    }
}

impl From<PlayerError> for ShaderLoadError {
    fn from(error: PlayerError) -> Self {
        Self::Read(error.to_string())
    }
}

impl From<ResolveError> for ShaderLoadError {
    fn from(error: ResolveError) -> Self {
        Self::Programs(error.to_string())
    }
}

/// A shader decoded from a file: the object and the chunks of its Vulkan platform.
#[derive(Clone)]
struct Loaded {
    shader: Shader,
    chunks: Vec<Vec<u8>>,
    queue: i32,
}

/// Where the adapter gets the program of a world shader from.
pub trait ShaderSupply {
    /// The program of `source`.
    ///
    /// # Errors
    /// Fails by name when the program cannot be supplied.
    fn load(&mut self, source: &ShaderSource) -> Result<LoadedShader, ShaderLoadError>;
}

impl ShaderSupply for ShaderLoader {
    fn load(&mut self, source: &ShaderSource) -> Result<LoadedShader, ShaderLoadError> {
        Self::load(self, source)
    }
}

/// Loads shaders from one player's data directory and keeps what it decoded.
pub struct ShaderLoader {
    data_directory: PathBuf,
    loaded: BTreeMap<(String, i64), Result<Loaded, ShaderLoadError>>,
}

/// A shader ready to register, with what the adapter needs to know about it.
pub struct LoadedShader {
    pub program: ProgramShader,
    /// The passes of the first sub-shader.
    pub passes: usize,
    /// The shader's own render queue, which a material that names none takes.
    pub queue: i32,
    /// The texture properties whose declared default the host supplies.
    pub texture_defaults: Vec<(String, DefaultTexture)>,
    /// Every 2D texture property with the name of the default it declares, empty for none.
    pub declared_defaults: Vec<(String, String)>,
}

impl ShaderLoader {
    #[must_use]
    pub fn new(data_directory: impl Into<PathBuf>) -> Self {
        Self {
            data_directory: data_directory.into(),
            loaded: BTreeMap::new(),
        }
    }

    /// The program of `source`, a new one for each call; the decode is shared.
    ///
    /// # Errors
    /// Fails when the file is outside the data directory or cannot be read, the object does not
    /// decode or has no Vulkan platform, or its table of programs is malformed.
    pub fn load(&mut self, source: &ShaderSource) -> Result<LoadedShader, ShaderLoadError> {
        let key = (source.file.clone(), source.path_id);
        if !self.loaded.contains_key(&key) {
            let decoded = self.decode(source);
            self.loaded.insert(key.clone(), decoded);
        }
        let loaded = self.loaded[&key].clone()?;
        let texture_defaults = declared(&loaded.shader);
        let declared_defaults = declared_names(&loaded.shader);
        let program = ProgramShader::new(loaded.shader, loaded.chunks)?;
        Ok(LoadedShader {
            passes: program.pass_count(),
            program,
            queue: loaded.queue,
            texture_defaults,
            declared_defaults,
        })
    }

    /// How many distinct sources were decoded or tried.
    #[must_use]
    pub fn sources(&self) -> usize {
        self.loaded.len()
    }

    fn decode(&self, source: &ShaderSource) -> Result<Loaded, ShaderLoadError> {
        let path = self.path_of(&source.file)?;
        let mut open = OpenFile::open(&path)?;
        let VulkanShader { shader, chunks } =
            vulkan_shader(&mut open, source.path_id)?.ok_or(ShaderLoadError::NoVulkanPlatform)?;
        let queue = shader_render_queue(&shader).map_err(ShaderLoadError::Queue)?;
        Ok(Loaded {
            shader,
            chunks,
            queue,
        })
    }

    /// The file under the data directory, refusing a path that leaves it.
    fn path_of(&self, file: &str) -> Result<PathBuf, ShaderLoadError> {
        data_path(&self.data_directory, file).ok_or_else(|| ShaderLoadError::OutsideDataDirectory {
            file: file.to_owned(),
        })
    }
}

/// The file `file` names under `directory`, or `None` for a path that leaves it: an absolute
/// one, one with `..` or a backslash, or an empty one.
pub(super) fn data_path(directory: &Path, file: &str) -> Option<PathBuf> {
    let relative = Path::new(file);
    let plain = relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    (!file.is_empty() && !file.contains('\\') && plain).then(|| directory.join(relative))
}
