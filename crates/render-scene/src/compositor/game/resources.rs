//! Buffers, textures and samplers a game's passes use, created and owned by the renderer from
//! the game's declarations, and the program runtime the passes draw with.
//!
//! A game describes each resource as data: a buffer's role and initial bytes, a texture's
//! format, extent and texels, a sampler's descriptor. The renderer generation creates it, counts
//! it in the memory census and against the factory's budget, and hands back a handle. The game
//! fills buffers through [`GameResources::write`], which stages the bytes in the frame like any
//! other upload, and binds resources by handle ([`GameResources::import_buffer`] and its
//! siblings, [`GameResources::vertex_input`], [`GameResources::indices`]). It never holds the
//! device, the queue or a resource of its own. Everything is released with the generation.
use std::cell::{RefCell, RefMut};
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use wgpu::util::DeviceExt;
use worldrunner_render_gpu::census::{ResourceClass, Tracked};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_post::{FrameError, PrepareContext};
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::{
    ExternalHold, Import, ImportedIndices, ImportedVertexInput, ProgramRuntime,
};

/// What a declared buffer is bound as. Every declared buffer can also be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameBufferUsage {
    Uniform,
    /// A storage buffer; the shaders' declaration says whether it is read-only.
    Storage,
    Vertex,
    Index,
}

/// A buffer a game declares: its role and its initial contents, which fix its size.
#[derive(Clone, Copy, Debug)]
pub struct GameBufferDeclaration<'a> {
    /// Names the buffer in errors, labels and the census.
    pub label: &'a str,
    pub usage: GameBufferUsage,
    /// The initial bytes, at least one. Zeros for a buffer the game writes before it is read.
    pub contents: &'a [u8],
}

/// A sampled texture a game declares: one level, of one or more layers or slices.
#[derive(Clone, Copy, Debug)]
pub struct GameTextureDeclaration<'a> {
    /// Names the texture in errors, labels and the census.
    pub label: &'a str,
    pub format: wgpu::TextureFormat,
    /// Width, height, and array layers (for `D2` and `D2Array`) or depth (for `D3`).
    pub size: [u32; 3],
    /// How the texture is sampled: `D2` (one layer), `D2Array` or `D3`.
    pub dimension: wgpu::TextureViewDimension,
    /// The level's texels, tightly packed rows of texels or blocks, layer after layer; `None`
    /// leaves the texture zeroed, which is the only choice for a depth format.
    pub texels: Option<&'a [u8]>,
}

macro_rules! handle {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name {
            store: u64,
            index: u32,
        }

        impl $name {
            const KIND: &'static str = $kind;
        }
    };
}

handle!(
    /// A buffer of one [`GameResources`].
    GameBuffer,
    "game buffer"
);
handle!(
    /// A texture of one [`GameResources`].
    GameTexture,
    "game texture"
);
handle!(
    /// A sampler of one [`GameResources`].
    GameSampler,
    "game sampler"
);

/// Why a game resource cannot be made, bound or written.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GameResourceError {
    /// The handle was made by another generation's or another factory's resources.
    #[error("the {0} belongs to another game resource store")]
    Foreign(&'static str),
    /// The declaration cannot be created as given.
    #[error("game resource {label}: {reason}")]
    Declaration { label: String, reason: String },
    /// The declared bytes, with what the factory's runtime holds, would pass its budget.
    #[error(
        "game resource {label} needs {bytes} bytes; with {held} held, that passes the factory's \
         budget of {budget}"
    )]
    Budget {
        label: String,
        bytes: u64,
        held: u64,
        budget: u64,
    },
    /// A write outside the buffer, or not aligned to four bytes.
    #[error("write of {len} bytes at {offset} into game buffer {label} of {size} bytes")]
    Write {
        label: String,
        offset: u64,
        len: u64,
        size: u64,
    },
    /// A buffer bound as something its usage does not allow.
    #[error("game buffer {label} is a {usage:?} buffer, not {wanted}")]
    Usage {
        label: String,
        usage: GameBufferUsage,
        wanted: &'static str,
    },
}

impl From<GameResourceError> for FrameError {
    fn from(error: GameResourceError) -> Self {
        Self::FeaturePreparation {
            feature: "game resources",
            reason: error.to_string(),
        }
    }
}

struct DeclaredBuffer {
    label: String,
    usage: GameBufferUsage,
    buffer: Tracked<wgpu::Buffer>,
    /// Keeps the buffer's bytes counted in the program runtime's budget while it lives.
    _hold: ExternalHold,
}

struct DeclaredTexture {
    texture: Tracked<wgpu::Texture>,
    view: wgpu::TextureView,
    dimension: wgpu::TextureViewDimension,
    /// Keeps the texture's bytes counted in the program runtime's budget while it lives.
    _hold: ExternalHold,
}

#[derive(Default)]
struct Store {
    buffers: Vec<DeclaredBuffer>,
    textures: Vec<DeclaredTexture>,
    samplers: Vec<wgpu::Sampler>,
    /// Bytes of the declared buffers and textures.
    bytes: u64,
}

/// Numbers every store, so a handle of one is refused by another.
static NEXT_STORE: AtomicU64 = AtomicU64::new(1);

/// The resources and program runtime of one factory in one renderer generation. Cloning shares
/// them; they are released when the generation and every feature it built are dropped.
#[derive(Clone)]
pub struct GameResources {
    id: u64,
    device: wgpu::Device,
    authority: SubmissionAuthority,
    budget: AllocationBudget,
    store: Rc<RefCell<Store>>,
    runtime: Rc<RefCell<ProgramRuntime>>,
}

impl GameResources {
    /// The resources of a factory with `budget`, on the renderer's device and authority.
    pub(crate) fn new(
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        budget: AllocationBudget,
    ) -> Self {
        Self {
            id: NEXT_STORE.fetch_add(1, Ordering::Relaxed),
            device: device.clone(),
            authority: authority.clone(),
            budget,
            store: Rc::default(),
            runtime: Rc::new(RefCell::new(ProgramRuntime::new(
                device.clone(),
                authority.clone(),
                budget,
            ))),
        }
    }

    /// The program runtime the factory's passes build programs, sets and draws with, made with
    /// the factory's budget. Do not hold it across a call that borrows it again.
    ///
    /// # Panics
    /// When it is already borrowed.
    pub fn runtime(&self) -> RefMut<'_, ProgramRuntime> {
        self.runtime.borrow_mut()
    }

    /// Bytes of the declared buffers and textures.
    pub fn bytes(&self) -> u64 {
        self.store.borrow().bytes
    }

    /// Admits `bytes` more against the budget, which the program runtime counts too: what the
    /// declared resources hold is in its ledger, so one budget bounds both. The hold keeps the
    /// bytes counted while the resource lives.
    fn admit(&self, label: &str, bytes: u64) -> Result<ExternalHold, GameResourceError> {
        let runtime = self.runtime.borrow();
        let held = runtime.live_bytes();
        if held
            .checked_add(bytes)
            .is_none_or(|total| total > self.budget.0)
        {
            return Err(GameResourceError::Budget {
                label: label.to_owned(),
                bytes,
                held,
                budget: self.budget.0,
            });
        }
        runtime
            .hold_external(label, bytes)
            .map_err(|error| refuse(label, &error.to_string()))
    }

    /// Creates the buffer `declaration` describes, holding its contents.
    ///
    /// # Errors
    /// [`GameResourceError::Declaration`] for empty contents, [`GameResourceError::Budget`]
    /// beyond the factory's budget.
    pub fn declare_buffer(
        &self,
        declaration: &GameBufferDeclaration<'_>,
    ) -> Result<GameBuffer, GameResourceError> {
        let label = declaration.label;
        if declaration.contents.is_empty() {
            return Err(refuse(label, "a buffer needs at least one byte"));
        }
        let (usage, class) = match declaration.usage {
            GameBufferUsage::Uniform => (wgpu::BufferUsages::UNIFORM, ResourceClass::Uniform),
            GameBufferUsage::Storage => (wgpu::BufferUsages::STORAGE, ResourceClass::Uniform),
            GameBufferUsage::Vertex => (wgpu::BufferUsages::VERTEX, ResourceClass::Mesh),
            GameBufferUsage::Index => (wgpu::BufferUsages::INDEX, ResourceClass::Mesh),
        };
        let bytes =
            (declaration.contents.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        let hold = self.admit(label, bytes)?;
        let buffer = self.authority.census().create_buffer_init(
            &self.device,
            class,
            &wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: declaration.contents,
                usage: usage | wgpu::BufferUsages::COPY_DST,
            },
        );
        let mut store = self.store.borrow_mut();
        store.bytes += buffer.size();
        store.buffers.push(DeclaredBuffer {
            label: label.to_owned(),
            usage: declaration.usage,
            buffer,
            _hold: hold,
        });
        Ok(GameBuffer {
            store: self.id,
            index: index(store.buffers.len()),
        })
    }

    /// Creates the texture `declaration` describes, holding its texels or zeros.
    ///
    /// # Errors
    /// [`GameResourceError::Declaration`] for an empty extent, a dimension other than `D2`,
    /// `D2Array` and `D3`, more than one layer for `D2`, a depth or block-compressed 3D texture,
    /// texels for a depth format or texels that are not exactly the level, an extent or layer
    /// count past the device's limits or a format it lacks the feature for;
    /// [`GameResourceError::Budget`] beyond the budget.
    pub fn declare_texture(
        &self,
        declaration: &GameTextureDeclaration<'_>,
    ) -> Result<GameTexture, GameResourceError> {
        let descriptor = texture_descriptor(declaration)?;
        beyond_device(&self.device, &descriptor)?;
        let bytes = worldrunner_render_gpu::census::texture_backing_bytes(&descriptor);
        let hold = self.admit(declaration.label, bytes)?;
        let texture = match declaration.texels {
            Some(texels) => self.authority.census().track_texture(
                ResourceClass::Texture,
                self.device.create_texture_with_data(
                    self.authority.queue(),
                    &descriptor,
                    wgpu::util::TextureDataOrder::LayerMajor,
                    texels,
                ),
            ),
            None => self.authority.census().create_texture(
                &self.device,
                ResourceClass::Texture,
                &descriptor,
            ),
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(declaration.dimension),
            ..wgpu::TextureViewDescriptor::default()
        });
        let mut store = self.store.borrow_mut();
        store.bytes += bytes;
        store.textures.push(DeclaredTexture {
            texture,
            view,
            dimension: declaration.dimension,
            _hold: hold,
        });
        Ok(GameTexture {
            store: self.id,
            index: index(store.textures.len()),
        })
    }

    /// Creates a sampler with `descriptor`. Samplers are not counted against the budget.
    pub fn declare_sampler(&self, descriptor: &wgpu::SamplerDescriptor<'_>) -> GameSampler {
        let mut store = self.store.borrow_mut();
        store.samplers.push(self.device.create_sampler(descriptor));
        GameSampler {
            store: self.id,
            index: index(store.samplers.len()),
        }
    }

    /// The whole buffer for the binding sourced from `semantic`.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's handle.
    pub fn import_buffer(
        &self,
        semantic: EngineSemantic,
        buffer: GameBuffer,
    ) -> Result<Import, GameResourceError> {
        let store = self.store.borrow();
        let declared = self.buffer(&store, buffer)?;
        Ok(Import::buffer(semantic, (*declared.buffer).clone()))
    }

    /// The texture, through its declared view, for the binding sourced from `semantic`.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's handle.
    pub fn import_texture(
        &self,
        semantic: EngineSemantic,
        texture: GameTexture,
    ) -> Result<Import, GameResourceError> {
        let store = self.store.borrow();
        let declared = store
            .textures
            .get(self.slot(texture.store, texture.index, GameTexture::KIND)?)
            .ok_or(GameResourceError::Foreign(GameTexture::KIND))?;
        Ok(Import::texture_view(
            semantic,
            (*declared.texture).clone(),
            declared.view.clone(),
            declared.dimension,
        ))
    }

    /// The sampler for the binding sourced from `semantic`.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's handle.
    pub fn import_sampler(
        &self,
        semantic: EngineSemantic,
        sampler: GameSampler,
    ) -> Result<Import, GameResourceError> {
        let store = self.store.borrow();
        let declared = store
            .samplers
            .get(self.slot(sampler.store, sampler.index, GameSampler::KIND)?)
            .ok_or(GameResourceError::Foreign(GameSampler::KIND))?;
        Ok(Import::sampler(semantic, declared.clone()))
    }

    /// A vertex buffer as a draw reads it: `stride` bytes a step, feeding `attributes`.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's handle and
    /// [`GameResourceError::Usage`] for a buffer not declared as vertex data.
    pub fn vertex_input(
        &self,
        buffer: GameBuffer,
        (stride, step): (u64, wgpu::VertexStepMode),
        attributes: Vec<wgpu::VertexAttribute>,
    ) -> Result<ImportedVertexInput, GameResourceError> {
        let store = self.store.borrow();
        let declared = self.buffer_as(&store, buffer, GameBufferUsage::Vertex)?;
        Ok(ImportedVertexInput {
            buffer: (*declared.buffer).clone(),
            stride,
            step,
            attributes,
        })
    }

    /// The indices `range` of an index buffer, of `format`.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's handle and
    /// [`GameResourceError::Usage`] for a buffer not declared as indices. A range past the
    /// buffer is the draw's to refuse.
    pub fn indices(
        &self,
        buffer: GameBuffer,
        format: wgpu::IndexFormat,
        range: Range<u32>,
    ) -> Result<ImportedIndices, GameResourceError> {
        let store = self.store.borrow();
        let declared = self.buffer_as(&store, buffer, GameBufferUsage::Index)?;
        Ok(ImportedIndices {
            buffer: (*declared.buffer).clone(),
            format,
            first: range.start,
            count: range.end.saturating_sub(range.start),
            base_vertex: 0,
        })
    }

    /// Stages `bytes` into `buffer` at `offset` in the frame `context` prepares, in staging
    /// order with the frame's other uploads. A frame that is cancelled copies nothing.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's handle and
    /// [`GameResourceError::Write`] for a range outside the buffer or not aligned to four bytes.
    pub fn write(
        &self,
        context: &mut PrepareContext<'_>,
        buffer: GameBuffer,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), GameResourceError> {
        let store = self.store.borrow();
        let declared = self.buffer(&store, buffer)?;
        let len = bytes.len() as u64;
        let size = declared.buffer.size();
        let aligned = offset.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
            && len.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        if !aligned || offset.checked_add(len).is_none_or(|end| end > size) {
            return Err(GameResourceError::Write {
                label: declared.label.clone(),
                offset,
                len,
                size,
            });
        }
        context.upload_buffer(&declared.buffer, offset, bytes);
        Ok(())
    }

    fn slot(&self, store: u64, index: u32, kind: &'static str) -> Result<usize, GameResourceError> {
        if store != self.id {
            return Err(GameResourceError::Foreign(kind));
        }
        usize::try_from(index).map_err(|_| GameResourceError::Foreign(kind))
    }

    fn buffer<'a>(
        &self,
        store: &'a Store,
        buffer: GameBuffer,
    ) -> Result<&'a DeclaredBuffer, GameResourceError> {
        store
            .buffers
            .get(self.slot(buffer.store, buffer.index, GameBuffer::KIND)?)
            .ok_or(GameResourceError::Foreign(GameBuffer::KIND))
    }

    fn buffer_as<'a>(
        &self,
        store: &'a Store,
        buffer: GameBuffer,
        usage: GameBufferUsage,
    ) -> Result<&'a DeclaredBuffer, GameResourceError> {
        let declared = self.buffer(store, buffer)?;
        if declared.usage != usage {
            return Err(GameResourceError::Usage {
                label: declared.label.clone(),
                usage: declared.usage,
                wanted: match usage {
                    GameBufferUsage::Vertex => "vertex data",
                    GameBufferUsage::Index => "indices",
                    GameBufferUsage::Uniform => "a uniform buffer",
                    GameBufferUsage::Storage => "a storage buffer",
                },
            });
        }
        Ok(declared)
    }
}

/// The index of the entry just pushed into a list of `len`.
fn index(len: usize) -> u32 {
    u32::try_from(len - 1).expect("fewer than 2^32 declared resources")
}

fn refuse(label: &str, reason: &str) -> GameResourceError {
    GameResourceError::Declaration {
        label: label.to_owned(),
        reason: reason.to_owned(),
    }
}

/// The descriptor of a declared texture, refused when the declaration cannot be created.
fn texture_descriptor<'a>(
    declaration: &GameTextureDeclaration<'a>,
) -> Result<wgpu::TextureDescriptor<'a>, GameResourceError> {
    let GameTextureDeclaration {
        label,
        format,
        size: [width, height, layers],
        dimension,
        texels,
    } = *declaration;
    let texture_dimension = match dimension {
        wgpu::TextureViewDimension::D2 if layers == 1 => wgpu::TextureDimension::D2,
        wgpu::TextureViewDimension::D2Array => wgpu::TextureDimension::D2,
        wgpu::TextureViewDimension::D3 => wgpu::TextureDimension::D3,
        _ => {
            return Err(refuse(
                label,
                "a declared texture is sampled as D2 (one layer), D2Array or D3",
            ));
        }
    };
    if width == 0 || height == 0 || layers == 0 {
        return Err(refuse(label, "the extent is empty"));
    }
    let (block_width, block_height) = format.block_dimensions();
    if !width.is_multiple_of(block_width) || !height.is_multiple_of(block_height) {
        return Err(refuse(
            label,
            "the extent is not whole blocks of the format",
        ));
    }
    if texture_dimension == wgpu::TextureDimension::D3 {
        if format.is_depth_stencil_format() {
            return Err(refuse(label, "a depth texture is not 3D"));
        }
        if format.is_compressed() {
            return Err(refuse(label, "a 3D texture is not block-compressed"));
        }
    }
    let mut usage = wgpu::TextureUsages::TEXTURE_BINDING;
    if let Some(texels) = texels {
        if format.is_depth_stencil_format() {
            return Err(refuse(label, "a depth texture is declared without texels"));
        }
        let block = format
            .block_copy_size(None)
            .ok_or_else(|| refuse(label, "the format has no single-plane block size"))?;
        let needed = u64::from(width / block_width)
            * u64::from(height / block_height)
            * u64::from(block)
            * u64::from(layers);
        if texels.len() as u64 != needed {
            return Err(refuse(
                label,
                &format!("the texels are {} bytes, the level {needed}", texels.len()),
            ));
        }
        usage |= wgpu::TextureUsages::COPY_DST;
    }
    Ok(wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: texture_dimension,
        format,
        usage,
        view_formats: &[],
    })
}

/// Refuses a texture the device cannot create (an extent or layer count past its limits, a format
/// it lacks the feature for), which wgpu would otherwise report as a validation panic.
fn beyond_device(
    device: &wgpu::Device,
    descriptor: &wgpu::TextureDescriptor<'_>,
) -> Result<(), GameResourceError> {
    let label = descriptor.label.unwrap_or_default();
    let limits = device.limits();
    let wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: depth,
    } = descriptor.size;
    let extent = width.max(height);
    let (extent_fits, layers_fit) = match descriptor.dimension {
        wgpu::TextureDimension::D3 => (extent.max(depth) <= limits.max_texture_dimension_3d, true),
        _ => (
            extent <= limits.max_texture_dimension_2d,
            depth <= limits.max_texture_array_layers,
        ),
    };
    if !extent_fits {
        return Err(refuse(
            label,
            "the extent is beyond the device's texture limit",
        ));
    }
    if !layers_fit {
        return Err(refuse(
            label,
            "there are more layers than the device allows",
        ));
    }
    if !device
        .features()
        .contains(descriptor.format.required_features())
    {
        return Err(refuse(label, "the device does not support the format"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
