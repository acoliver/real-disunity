//! Compute dispatches of a frame as render-program dispatches.
//!
//! A [`ComputeSupply`] gives the stage program of a kernel, the way a [`ShaderSupply`] gives the
//! passes of a shader. The adapter keeps a kernel until its shader's generation changes. For each
//! dispatch it packs the properties into the kernel's constant buffers, binds the textures the
//! properties name by the names the kernel declares, and uploads the compute buffers the kernel
//! names, binding them by name too. What cannot be bound leaves the dispatch out and is counted
//! by the name of the cause.
//!
//! [`ShaderSupply`]: super::shaders::ShaderSupply

use std::collections::BTreeMap;
use std::rc::Rc;

use worldrunner_render_program::constants::pack_layout;
use worldrunner_render_program::module::StageProgram;
use worldrunner_render_program::offscreen::OffscreenState;
use worldrunner_render_program::runtime::{
    BoundResource, BufferHandle, ComputeProgram, ProgramRuntime,
};
use worldrunner_render_program::targets::{BufferTarget, ConstantBufferTarget, TextureTarget};
use worldrunner_world_simulation::graphics::{
    ComputeBindings, ComputeBufferId, ComputeShaderId, GraphicsSnapshot, TextureRef,
    WorldComputeShader,
};

use super::ids;
use super::properties::values;

/// Why a kernel could not be loaded.
#[derive(Clone, Debug, thiserror::Error)]
pub enum ComputeLoadError {
    #[error("the shader has no kernel {0}")]
    NoSuchKernel(u32),
    #[error("the shader's compiled code is not in the player")]
    NoCompiledCode,
    #[error("the kernel has no variant for no keywords")]
    NoVariant,
    #[error("{0}")]
    Unreadable(String),
    #[error("the kernel does not build a stage program: {0}")]
    Program(String),
}

impl ComputeLoadError {
    /// The name of the error's kind, for reports.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::NoSuchKernel(_) => "NoSuchKernel",
            Self::NoCompiledCode => "NoCompiledCode",
            Self::NoVariant => "NoVariant",
            Self::Unreadable(_) => "Unreadable",
            Self::Program(_) => "Program",
        }
    }
}

/// Where the compiled kernels of a player's compute shaders come from.
pub trait ComputeSupply {
    /// The stage program of kernel `kernel` of `shader`.
    ///
    /// # Errors
    /// Fails when the shader has no such kernel, its compiled code cannot be found or read, or
    /// the program does not describe a compute stage.
    fn load(
        &mut self,
        shader: &WorldComputeShader,
        kernel: u32,
    ) -> Result<StageProgram, ComputeLoadError>;
}

/// A supply that has no compiled kernels: every dispatch is left out as `NoCompiledCode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoKernels;

impl ComputeSupply for NoKernels {
    fn load(&mut self, _: &WorldComputeShader, _: u32) -> Result<StageProgram, ComputeLoadError> {
        Err(ComputeLoadError::NoCompiledCode)
    }
}

/// A kernel ready to dispatch, with the places its named resources bind.
struct Kernel {
    program: ComputeProgram,
    constants: Vec<ConstantBufferTarget>,
    textures: Vec<TextureTarget>,
    buffers: Vec<BufferTarget>,
}

struct Held {
    generation: u64,
    /// The kernel, or the name of the error that stopped it.
    kernel: Result<Rc<Kernel>, String>,
}

/// What the adapter keeps between frames: the kernels it has loaded.
pub struct ComputeAdapter {
    supply: Box<dyn ComputeSupply>,
    kernels: BTreeMap<(ComputeShaderId, u32), Held>,
}

/// A dispatch ready to record.
pub struct Prepared {
    kernel: Rc<Kernel>,
    bindings: Vec<BoundResource>,
    groups: [u32; 3],
}

impl Prepared {
    /// The program, the bound resources and the group counts of the dispatch.
    #[must_use]
    pub fn request(&self) -> worldrunner_render_program::runtime::DispatchRequest<'_> {
        worldrunner_render_program::runtime::DispatchRequest {
            program: &self.kernel.program,
            bindings: &self.bindings,
            groups: self.groups,
        }
    }
}

/// The compute buffers of a flush, on the GPU. Each is uploaded from the contents the snapshot
/// holds the first time a dispatch binds it, so a later dispatch sees what an earlier one wrote.
#[derive(Default)]
pub struct GpuBuffers {
    held: BTreeMap<ComputeBufferId, (BufferHandle, Vec<u8>)>,
    constants: Vec<BufferHandle>,
}

impl GpuBuffers {
    /// The buffers a dispatch bound, with the contents they were uploaded with.
    pub fn bound(&self) -> impl Iterator<Item = (ComputeBufferId, BufferHandle, &[u8])> {
        self.held
            .iter()
            .map(|(id, (handle, original))| (*id, *handle, original.as_slice()))
    }

    /// Every buffer made for the flush, to release once its submission has completed.
    pub fn handles(&self) -> impl Iterator<Item = BufferHandle> + '_ {
        self.held
            .values()
            .map(|(handle, _)| *handle)
            .chain(self.constants.iter().copied())
    }

    fn upload(
        &mut self,
        runtime: &mut ProgramRuntime,
        snapshot: &GraphicsSnapshot,
        id: ComputeBufferId,
    ) -> Result<BufferHandle, &'static str> {
        if let Some((handle, _)) = self.held.get(&id) {
            return Ok(*handle);
        }
        let buffer = snapshot.compute_buffers.get(&id).ok_or("StaleBuffer")?;
        let handle = runtime
            .create_buffer("compute buffer", &buffer.contents)
            .map_err(|_| "BufferUpload")?;
        self.held.insert(id, (handle, buffer.contents.clone()));
        Ok(handle)
    }

    /// The buffers `kernel` declares, bound by name to the buffers the dispatch bound.
    fn bind(
        &mut self,
        kernel: &Kernel,
        (runtime, snapshot): (&mut ProgramRuntime, &GraphicsSnapshot),
        bindings: &ComputeBindings,
    ) -> Result<Vec<BoundResource>, String> {
        let mut bound = Vec::new();
        for target in &kernel.buffers {
            let name = target
                .name
                .resolve(&[])
                .map_err(|_| "BufferName".to_owned())?;
            let (_, buffer) = bindings
                .buffers
                .iter()
                .find(|(bound, _)| bound == name)
                .ok_or_else(|| "UnboundBuffer".to_owned())?;
            let handle = self
                .upload(runtime, snapshot, *buffer)
                .map_err(str::to_owned)?;
            bound.push(BoundResource::buffer(target.set, target.binding, handle));
        }
        Ok(bound)
    }
}

/// The textures `kernel` declares, bound by name to the textures the dispatch's properties name.
fn textures(
    kernel: &Kernel,
    state: &OffscreenState,
    bindings: &ComputeBindings,
) -> Result<Vec<BoundResource>, String> {
    let mut bound = Vec::new();
    for target in &kernel.textures {
        let name = target
            .name
            .resolve(&[])
            .map_err(|_| "TextureName".to_owned())?;
        let texture = bindings
            .properties
            .textures
            .iter()
            .find(|(property, _)| property == name)
            .map(|(_, slot)| &slot.texture)
            .ok_or_else(|| "MissingTexture".to_owned())?;
        let id = match texture {
            TextureRef::Render(id) => Some(ids::render_texture(*id)),
            TextureRef::Asset(identity) => ids::stored_texture(*identity),
            TextureRef::None | TextureRef::Global(_) => None,
        }
        .ok_or_else(|| "MissingTexture".to_owned())?;
        let handle = state
            .texture_handle(id)
            .ok_or_else(|| "TextureUnavailable".to_owned())?;
        bound.push(BoundResource::texture(target.set, target.binding, handle));
    }
    Ok(bound)
}

impl ComputeAdapter {
    #[must_use]
    pub fn new(supply: impl ComputeSupply + 'static) -> Self {
        Self {
            supply: Box::new(supply),
            kernels: BTreeMap::new(),
        }
    }

    /// The kernel, loaded again when its shader's generation rose.
    fn kernel(
        &mut self,
        runtime: &ProgramRuntime,
        id: ComputeShaderId,
        shader: &WorldComputeShader,
        index: u32,
    ) -> Result<Rc<Kernel>, String> {
        if let Some(held) = self.kernels.get(&(id, index))
            && held.generation == shader.generation
        {
            return held.kernel.clone();
        }
        let kernel = self.load(runtime, shader, index);
        self.kernels.insert(
            (id, index),
            Held {
                generation: shader.generation,
                kernel: kernel.clone(),
            },
        );
        kernel
    }

    fn load(
        &mut self,
        runtime: &ProgramRuntime,
        shader: &WorldComputeShader,
        index: u32,
    ) -> Result<Rc<Kernel>, String> {
        if index as usize >= shader.kernels.len() {
            return Err(ComputeLoadError::NoSuchKernel(index).name().to_owned());
        }
        let stage = self
            .supply
            .load(shader, index)
            .map_err(|error| error.name().to_owned())?;
        let program = runtime
            .compute_program(&stage)
            .map_err(|_| "ProgramLayout".to_owned())?;
        let layout = |_| "TargetLayout".to_owned();
        Ok(Rc::new(Kernel {
            program,
            constants: stage.constant_buffer_targets().map_err(layout)?,
            textures: stage.texture_targets().map_err(layout)?,
            buffers: stage.buffer_targets().map_err(layout)?,
        }))
    }

    /// Prepares one dispatch of `snapshot`'s work, uploading what it binds. The error is the
    /// name of the reason the dispatch is left out.
    ///
    /// # Errors
    /// Fails by name when the shader or a buffer no longer exists, the kernel cannot be
    /// loaded, a name the kernel declares has nothing bound to it or a texture it names is not
    /// on the GPU.
    pub fn prepare(
        &mut self,
        (runtime, state): (&mut ProgramRuntime, &OffscreenState),
        snapshot: &GraphicsSnapshot,
        buffers: &mut GpuBuffers,
        dispatch: (ComputeShaderId, u32, [u32; 3], &ComputeBindings),
    ) -> Result<Prepared, String> {
        let (id, index, groups, bindings) = dispatch;
        let shader = snapshot
            .compute_shaders
            .get(&id)
            .ok_or_else(|| "StaleShader".to_owned())?;
        let kernel = self.kernel(runtime, id, shader, index)?;
        let mut bound = Vec::new();
        let set = values(&bindings.properties);
        for target in &kernel.constants {
            let slot = kernel
                .program
                .layout()
                .slot(target.set, target.binding)
                .map_or(0, |slot| {
                    usize::try_from(slot.min_size).unwrap_or(usize::MAX)
                });
            let packed = pack_layout(&target.buffer, &[], &[&set], slot)
                .map_err(|_| "Constants".to_owned())?;
            let handle = runtime
                .create_buffer("compute constants", &packed.bytes)
                .map_err(|_| "BufferUpload".to_owned())?;
            buffers.constants.push(handle);
            bound.push(BoundResource::buffer(target.set, target.binding, handle));
        }
        bound.extend(textures(&kernel, state, bindings)?);
        bound.extend(buffers.bind(&kernel, (runtime, snapshot), bindings)?);
        Ok(Prepared {
            kernel,
            bindings: bound,
            groups,
        })
    }
}
