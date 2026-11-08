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

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use worldrunner_render_program::error::{ProgramError, StorageImageError};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::runtime::{
    BoundResource, BufferHandle, ComputeProgram, ProgramRuntime,
};
use worldrunner_render_program::unity::constants::pack_layout;
use worldrunner_render_program::unity::module::StageProgram;
use worldrunner_render_program::unity::offscreen::OffscreenState;
use worldrunner_render_program::unity::targets::{
    BufferTarget, ConstantBufferTarget, ImageTarget, TextureTarget,
};
use worldrunner_world_simulation::graphics::{
    ComputeBindings, ComputeBufferId, ComputeShaderId, GraphicsSnapshot, OffscreenWork, TextureRef,
    WorldComputeShader,
};

use super::ids;
use super::properties::values;

/// How a dispatch was bound and what came of it, for the report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DispatchTrace {
    /// The compute shader's name and the kernel's.
    pub shader: String,
    pub kernel: String,
    /// The variant of the kernel that runs, as the supply describes it.
    pub variant: String,
    pub groups: [u32; 3],
    /// The thread group size the world holds for the kernel.
    pub thread_group: Option<[u32; 3]>,
    /// The numeric values the dispatch sets, as `name=value`.
    pub constants: Vec<String>,
    /// The members of the kernel's constant buffers that no value was set for.
    pub constants_missing: Vec<String>,
    /// The textures the kernel declares, in the order it declares them.
    pub textures: Vec<TextureTrace>,
    /// The storage images the kernel declares, in the order it declares them, with what the
    /// dispatch bound to each.
    pub images: Vec<TextureTrace>,
    /// The names of the textures the dispatch sets, whether or not the kernel declares them.
    pub textures_set: Vec<String>,
    /// The buffers the kernel declares.
    pub buffers: Vec<BufferTrace>,
    /// `bound` once everything was bound, `ran` once its submission completed, else the name of
    /// the reason the dispatch was left out.
    pub outcome: String,
    /// Whether the buffers were read back after the dispatch.
    pub read_back: bool,
}

impl DispatchTrace {
    /// The trace as report lines: the dispatch, then what it set and bound.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "dispatch {}.{} ({}) groups {:?}, world thread group {:?}: {}, read back {}",
            self.shader,
            self.kernel,
            self.variant,
            self.groups,
            self.thread_group,
            self.outcome,
            self.read_back
        )];
        lines.push(format!(
            "  constants set [{}], without a value [{}]",
            self.constants.join(", "),
            self.constants_missing.join(", ")
        ));
        lines.push(format!("  textures set [{}]", self.textures_set.join(", ")));
        for texture in &self.textures {
            lines.push(format!(
                "  texture {} from {}, on the GPU {}, camera renders before {} drew {} left out {} \
                 for want of a pass, {} for want of a texture and {} for want of a mesh channel",
                texture.name,
                texture.source,
                texture.resolved,
                texture.camera_renders,
                texture.camera_draws,
                texture.camera_unmatched,
                texture.camera_missing_texture,
                texture.camera_missing_channel
            ));
            if let Some(after) = &texture.after_operations {
                lines.push(format!(
                    "    after the operations before the dispatch: {after}"
                ));
            }
            if let Some(before) = &texture.before_dispatch {
                lines.push(format!("    right before the dispatch: {before}"));
            }
            for note in &texture.camera_notes {
                lines.push(format!("    {note}"));
            }
            if let Some(test) = &texture.test_render {
                lines.push(format!("    test render: {test}"));
            }
            for variant in &texture.variant_renders {
                lines.push(format!("    test render with {variant}"));
            }
            for source in &texture.sampled {
                lines.push(format!(
                    "    sampled {} (texture {}): uploaded {}, read back from the GPU {}",
                    source.name,
                    source.texture.0,
                    source.uploaded,
                    source.on_the_gpu.as_deref().unwrap_or("not read")
                ));
            }
        }
        for image in &self.images {
            lines.push(format!(
                "  storage image {} from {}, on the GPU {}",
                image.name, image.source, image.resolved
            ));
        }
        for buffer in &self.buffers {
            lines.push(format!(
                "  buffer {} {} bytes, non-zero words {} before, {:?} after",
                buffer.name, buffer.bytes, buffer.nonzero_before, buffer.nonzero_after
            ));
        }
        lines
    }
}

/// A texture a kernel declares and what the dispatch bound to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextureTrace {
    pub name: String,
    /// `render`, `asset`, `global`, `none` for a reference to nothing, or `unset` when the
    /// dispatch sets no texture of that name.
    pub source: String,
    /// Whether the texture is on the GPU to bind.
    pub resolved: bool,
    pub texture: Option<TextureId>,
    /// The camera renders into the texture before the dispatch, the draws they recorded and the
    /// draws they left out for want of a pass.
    pub camera_renders: usize,
    pub camera_draws: usize,
    pub camera_unmatched: usize,
    pub camera_missing_texture: usize,
    pub camera_missing_channel: usize,
    /// What a render texture held in texels after the operations before the dispatch and right
    /// before the dispatch, as counts and a value range.
    pub after_operations: Option<String>,
    pub before_dispatch: Option<String>,
    /// How each camera render into the texture drew: its target, state and the values it lacked.
    pub camera_notes: Vec<String>,
    /// How many texels of the camera's test render into a scratch copy differ from its clear,
    /// which are the texels a fragment was written to, against all the texels.
    pub test_render: Option<String>,
    /// The same for the changed test renders, each as `what is changed: counts`, in the order of
    /// the variants.
    pub variant_renders: Vec<String>,
    /// The uploaded textures the camera renders into this texture sampled.
    pub sampled: Vec<SampledSource>,
}

/// An uploaded texture a camera render sampled: what the bytes uploaded held and what the GPU
/// copy holds when read back, as counts of texels (blocks, for a block-compressed format).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampledSource {
    pub name: String,
    pub texture: TextureId,
    pub uploaded: String,
    pub on_the_gpu: Option<String>,
}

/// A buffer a kernel declares and what the dispatch bound to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BufferTrace {
    pub name: String,
    pub id: Option<ComputeBufferId>,
    pub bytes: usize,
    /// Four-byte words that are not zero when the buffer was uploaded and after the flush.
    pub nonzero_before: usize,
    pub nonzero_after: Option<usize>,
}

/// The four-byte words of `bytes` that are not zero.
#[must_use]
pub fn nonzero_words(bytes: &[u8]) -> usize {
    bytes
        .chunks_exact(4)
        .filter(|word| word.iter().any(|byte| *byte != 0))
        .count()
}

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

    /// Which variant of the kernel `load` builds, in words for a report.
    fn describe(&mut self, _shader: &WorldComputeShader, _kernel: u32) -> String {
        String::new()
    }
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
    images: Vec<ImageTarget>,
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
        (bindings, trace): (&ComputeBindings, &mut DispatchTrace),
    ) -> Result<Vec<BoundResource>, String> {
        let mut bound = Vec::new();
        for target in &kernel.buffers {
            let name = target
                .name
                .resolve(&[])
                .map_err(|_| "BufferName".to_owned())?;
            trace.buffers.push(BufferTrace {
                name: name.to_owned(),
                ..BufferTrace::default()
            });
            let (_, buffer) = bindings
                .buffers
                .iter()
                .find(|(bound, _)| bound == name)
                .ok_or_else(|| "UnboundBuffer".to_owned())?;
            let handle = self
                .upload(runtime, snapshot, *buffer)
                .map_err(str::to_owned)?;
            if let Some((_, original)) = self.held.get(buffer)
                && let Some(entry) = trace.buffers.last_mut()
            {
                entry.id = Some(*buffer);
                entry.bytes = original.len();
                entry.nonzero_before = nonzero_words(original);
            }
            bound.push(BoundResource::buffer(target.set, target.binding, handle));
        }
        Ok(bound)
    }
}

/// The textures `kernel` declares, bound by name to the textures the dispatch's properties name.
fn textures(
    kernel: &Kernel,
    state: &OffscreenState,
    (bindings, trace): (&ComputeBindings, &mut DispatchTrace),
) -> Result<Vec<BoundResource>, String> {
    let mut bound = Vec::new();
    for target in &kernel.textures {
        let name = target
            .name
            .resolve(&[])
            .map_err(|_| "TextureName".to_owned())?;
        let mut entry = TextureTrace {
            name: name.to_owned(),
            source: "unset".to_owned(),
            ..TextureTrace::default()
        };
        let texture = bindings
            .properties
            .textures
            .iter()
            .find(|(property, _)| property == name)
            .map(|(_, slot)| &slot.texture);
        let Some(texture) = texture else {
            trace.textures.push(entry);
            return Err("MissingTexture".to_owned());
        };
        let id = match texture {
            TextureRef::Render(id) => Some(ids::render_texture(*id)),
            TextureRef::Asset(identity) => ids::stored_texture(*identity),
            TextureRef::None | TextureRef::Global(_) => None,
        };
        texture_source(texture).clone_into(&mut entry.source);
        entry.texture = id;
        let Some(id) = id else {
            trace.textures.push(entry);
            return Err("MissingTexture".to_owned());
        };
        let handle = state.texture_handle(id);
        entry.resolved = handle.is_some();
        trace.textures.push(entry);
        let handle = handle.ok_or_else(|| "TextureUnavailable".to_owned())?;
        bound.push(BoundResource::texture(target.set, target.binding, handle));
    }
    Ok(bound)
}

/// The storage images `kernel` declares, bound by name to the render textures the dispatch's
/// properties name. Only a render texture can be one, because the host makes those with storage
/// binding usage when a kernel binds them; the name of the cause is `StorageSource` for any other
/// texture, `StorageFormat` for a render texture whose format the device cannot bind as storage.
fn images(
    kernel: &Kernel,
    state: &OffscreenState,
    (bindings, trace): (&ComputeBindings, &mut DispatchTrace),
) -> Result<Vec<BoundResource>, String> {
    let mut bound = Vec::new();
    for target in &kernel.images {
        let name = target
            .name
            .resolve(&[])
            .map_err(|_| "TextureName".to_owned())?;
        let mut entry = TextureTrace {
            name: name.to_owned(),
            source: "unset".to_owned(),
            ..TextureTrace::default()
        };
        let texture = bindings
            .properties
            .textures
            .iter()
            .find(|(property, _)| property == name)
            .map(|(_, slot)| &slot.texture);
        let Some(texture) = texture else {
            trace.images.push(entry);
            return Err("MissingTexture".to_owned());
        };
        texture_source(texture).clone_into(&mut entry.source);
        let id = match texture {
            TextureRef::Render(id) => Some(ids::render_texture(*id)),
            TextureRef::Asset(_) | TextureRef::Global(_) | TextureRef::None => None,
        };
        entry.texture = id;
        let Some(id) = id else {
            trace.images.push(entry);
            return Err("StorageSource".to_owned());
        };
        let held = state.texture_storage(id).zip(state.texture_handle(id));
        entry.resolved = held.is_some();
        trace.images.push(entry);
        let ((_, storage), handle) = held.ok_or_else(|| "TextureUnavailable".to_owned())?;
        if !storage {
            return Err("StorageFormat".to_owned());
        }
        bound.push(BoundResource::texture(target.set, target.binding, handle));
    }
    Ok(bound)
}

/// The name of the cause a kernel's program cannot be built for: a storage image the device
/// cannot hold, or `ProgramLayout`.
pub(super) fn program_cause(error: &ProgramError) -> &'static str {
    match error {
        ProgramError::StorageImage { .. } | ProgramError::StorageImageLimit { .. } => {
            storage_cause(error)
        }
        _ => "ProgramLayout",
    }
}

/// The name of the cause a texture is refused as a storage image for.
fn storage_cause(error: &ProgramError) -> &'static str {
    match error {
        ProgramError::StorageImage { error, .. } => match error {
            StorageImageError::Format { .. } => "StorageFormatMismatch",
            StorageImageError::Usage { .. } => "StorageUsage",
            StorageImageError::Dimension { .. } => "StorageDimension",
            StorageImageError::Multisampled { .. } => "StorageSamples",
            StorageImageError::Unsupported { .. } => "StorageUnsupported",
        },
        ProgramError::StorageImageLimit { .. } => "StorageLimit",
        ProgramError::AliasedTexture { .. } => "StorageAliased",
        _ => "StorageImage",
    }
}

/// What kind of texture a reference names, for the trace.
fn texture_source(texture: &TextureRef) -> &'static str {
    match texture {
        TextureRef::Render(_) => "render",
        TextureRef::Asset(_) => "asset",
        TextureRef::Global(_) => "global",
        TextureRef::None => "none",
    }
}

/// The numeric values a dispatch sets, as `name=value` in name order within each kind.
fn constant_values(bindings: &ComputeBindings) -> Vec<String> {
    let properties = &bindings.properties;
    let floats = properties.floats.iter().map(|(n, v)| format!("{n}={v}"));
    let ints = properties.ints.iter().map(|(n, v)| format!("{n}={v}"));
    let vectors = properties.vectors.iter().map(|(n, v)| format!("{n}={v:?}"));
    let matrices = properties
        .matrices
        .iter()
        .map(|(n, _)| format!("{n}=matrix"));
    floats.chain(ints).chain(vectors).chain(matrices).collect()
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
        let program = stage
            .neutral()
            .map_err(ProgramError::from)
            .and_then(|neutral| runtime.compute_program(&neutral))
            .map_err(|error| program_cause(&error).to_owned())?;
        let layout = |_| "TargetLayout".to_owned();
        Ok(Rc::new(Kernel {
            program,
            constants: stage.constant_buffer_targets().map_err(layout)?,
            textures: stage.texture_targets().map_err(layout)?,
            images: stage.image_targets().map_err(layout)?,
            buffers: stage.buffer_targets().map_err(layout)?,
        }))
    }

    /// The render textures the dispatches of `snapshot` bind as storage images, by the names their
    /// kernels declare. A host marks them before it builds the frame, so they are made with
    /// storage binding usage. A kernel that cannot be loaded names none, and its dispatch is left
    /// out by the cause when it is prepared.
    pub fn storage_textures(
        &mut self,
        runtime: &ProgramRuntime,
        snapshot: &GraphicsSnapshot,
    ) -> BTreeSet<TextureId> {
        let mut found = BTreeSet::new();
        for work in &snapshot.work {
            let OffscreenWork::Dispatch {
                shader,
                kernel,
                bindings,
                ..
            } = work
            else {
                continue;
            };
            let Some(world_shader) = snapshot.compute_shaders.get(shader) else {
                continue;
            };
            let Ok(kernel) = self.kernel(runtime, *shader, world_shader, *kernel) else {
                continue;
            };
            for target in &kernel.images {
                let Ok(name) = target.name.resolve(&[]) else {
                    continue;
                };
                let property = bindings
                    .properties
                    .textures
                    .iter()
                    .find(|(property, _)| property == name);
                if let Some((_, slot)) = property
                    && let TextureRef::Render(id) = &slot.texture
                {
                    found.insert(ids::render_texture(*id));
                }
            }
        }
        found
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
        trace: &mut DispatchTrace,
    ) -> Result<Prepared, String> {
        let (id, index, groups, bindings) = dispatch;
        trace.groups = groups;
        trace.constants = constant_values(bindings);
        trace.textures_set = bindings
            .properties
            .textures
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let shader = snapshot
            .compute_shaders
            .get(&id)
            .ok_or_else(|| "StaleShader".to_owned())?;
        shader.name.clone_into(&mut trace.shader);
        if let Some(listed) = shader.kernels.get(index as usize) {
            listed.name.clone_into(&mut trace.kernel);
            trace.thread_group = listed.thread_group_size;
        }
        trace.variant = self.supply.describe(shader, index);
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
            trace
                .constants_missing
                .extend(packed.missing.iter().cloned());
            let handle = runtime
                .create_buffer("compute constants", &packed.bytes)
                .map_err(|_| "BufferUpload".to_owned())?;
            buffers.constants.push(handle);
            bound.push(BoundResource::buffer(target.set, target.binding, handle));
        }
        bound.extend(textures(&kernel, state, (bindings, trace))?);
        bound.extend(images(&kernel, state, (bindings, trace))?);
        bound.extend(buffers.bind(&kernel, (runtime, snapshot), (bindings, trace))?);
        // A texture the kernel writes must suit its binding and appear once, or the dispatch would
        // fail the whole submission it is recorded into.
        runtime
            .check_storage_images(&kernel.program, &bound)
            .map_err(|error| storage_cause(&error).to_owned())?;
        "bound".clone_into(&mut trace.outcome);
        Ok(Prepared {
            kernel,
            bindings: bound,
            groups,
        })
    }
}
