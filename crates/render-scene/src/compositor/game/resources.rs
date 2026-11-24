//! Buffers, textures and samplers a game's passes use, created and owned by the renderer from
//! the game's declarations, and the program runtime the passes draw with.
//!
//! A game describes each resource as data: a buffer's role and initial bytes, a texture's
//! format, extent, mip chain and texels, a sampler's descriptor. The renderer generation creates
//! it, counts it in the memory census and against the factory's budget, and hands back a handle.
//! A texture every factory of the generation may declare is made once and shared
//! ([`GameResources::declare_shared_texture`]). The game
//! fills buffers through [`GameResources::write`], which stages the bytes in the frame like any
//! other upload, and binds resources by handle ([`GameResources::import_buffer`] and its
//! siblings, [`GameResources::vertex_input`], [`GameResources::indices`]). It never holds the
//! device, the queue or a resource of its own. Everything is released with the generation.
use super::declared_textures::{
    GameTextureDeclaration, TextureIdentity, beyond_limits, refuse, unsampleable, upload,
};
use super::level_import::{LevelAccess, import_level};
use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// A block transfer into the texture `label` that cannot be made.
    #[error("block transfer into {label}: {reason}")]
    Transfer { label: String, reason: String },
    /// A readback of the texture or target `label` that cannot be made or delivered.
    #[error("readback of {label}: {reason}")]
    Readback { label: String, reason: String },
    /// An import of one level of the texture or target `label` that cannot be made.
    #[error("import of a level of {label}: {reason}")]
    Import { label: String, reason: String },
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

/// A declared texture; a shared one is held by every store that declared it.
#[derive(Clone)]
struct DeclaredTexture {
    label: String,
    texture: Rc<Tracked<wgpu::Texture>>,
    view: wgpu::TextureView,
    dimension: wgpu::TextureViewDimension,
    bytes: u64,
}

#[derive(Default)]
struct Store {
    buffers: Vec<DeclaredBuffer>,
    textures: Vec<DeclaredTexture>,
    samplers: Vec<wgpu::Sampler>,
    /// The index in `textures` of each shared texture this store declared, by label.
    shared: HashMap<String, u32>,
    /// Keeps the bytes of each texture in `textures` counted in this factory's runtime ledger
    /// while the store lives. A shared texture is held once by every store that declared it,
    /// since each factory counts it against its own budget.
    texture_holds: Vec<ExternalHold>,
    /// Bytes of the declared buffers and textures.
    bytes: u64,
    /// The transfers and readbacks the game asked for and the frames that recorded them.
    work: work::WorkQueue,
}

/// The textures declared shared in one renderer generation, by label, with what their first
/// declaration declared. Every factory's store of the generation holds the same table.
#[derive(Clone, Default)]
pub(crate) struct SharedTextures(Rc<RefCell<HashMap<String, (TextureIdentity, DeclaredTexture)>>>);

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
    shared: SharedTextures,
    runtime: Rc<RefCell<ProgramRuntime>>,
}

impl GameResources {
    /// The resources of a factory with `budget`, on the renderer's device and authority, sharing
    /// `shared` with the generation's other factories.
    pub(crate) fn new(
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        budget: AllocationBudget,
        shared: SharedTextures,
    ) -> Self {
        Self {
            id: NEXT_STORE.fetch_add(1, Ordering::Relaxed),
            device: device.clone(),
            authority: authority.clone(),
            budget,
            store: Rc::default(),
            shared,
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

    /// Names this factory's store in the engine's records of who holds a shared resource.
    pub(super) fn id(&self) -> u64 {
        self.id
    }

    /// Bytes of the declared buffers and textures.
    pub fn bytes(&self) -> u64 {
        self.store.borrow().bytes
    }

    /// Admits `bytes` more against the budget, which the program runtime counts too: what the
    /// declared resources hold is in its ledger, so one budget bounds both. The hold keeps the
    /// bytes counted while the resource lives.
    pub(super) fn admit(&self, label: &str, bytes: u64) -> Result<ExternalHold, GameResourceError> {
        self.fits(label, bytes)?;
        self.runtime
            .borrow()
            .hold_external(label, bytes)
            .map_err(|error| refuse(label, &error.to_string()))
    }

    /// Whether `bytes` more fit the budget with everything the runtime's ledger holds, counting
    /// nothing: for staging the runtime admits itself when the frame records it.
    fn fits(&self, label: &str, bytes: u64) -> Result<(), GameResourceError> {
        let held = self.runtime.borrow().live_bytes();
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
        Ok(())
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
    /// `D2Array` and `D3`, more than one layer for `D2`, a mip count the extent does not have,
    /// a depth or block-compressed 3D texture, texels for a depth format or a 3D texture deeper
    /// than one slice, texels that are not exactly the levels [`GameTextureDeclaration::levels`]
    /// lists, an extent or layer count past the device's limits, or a format the device cannot
    /// sample; [`GameResourceError::Budget`] beyond the budget.
    pub fn declare_texture(
        &self,
        declaration: &GameTextureDeclaration<'_>,
    ) -> Result<GameTexture, GameResourceError> {
        let (declared, hold) = self.create_texture(declaration)?;
        Ok(self.hold_texture(declared, hold))
    }

    /// Declares a texture the generation's other factories may declare too, under the same
    /// label: the first declaration creates it, a later one, from this factory or another, gets
    /// the same texture as a handle of its own store. Every factory that declares it counts it
    /// against its own budget, as it would hold it alone, and holds it once in its runtime's
    /// ledger however often it declares it; the memory census counts it once.
    ///
    /// # Errors
    /// As [`Self::declare_texture`], and [`GameResourceError::Declaration`] when the
    /// declaration differs from the one that created the texture (in format, extent,
    /// dimension, mip count or texels).
    pub fn declare_shared_texture(
        &self,
        declaration: &GameTextureDeclaration<'_>,
    ) -> Result<GameTexture, GameResourceError> {
        let label = declaration.label;
        let identity = declaration.identity();
        let differs = || {
            refuse(
                label,
                "the declaration differs from the shared texture of that label",
            )
        };
        let existing = self.shared.0.borrow().get(label).cloned();
        let Some((first, declared)) = existing else {
            let (declared, hold) = self.create_texture(declaration)?;
            self.shared
                .0
                .borrow_mut()
                .insert(label.to_owned(), (identity, declared.clone()));
            let texture = self.hold_texture(declared, hold);
            self.store
                .borrow_mut()
                .shared
                .insert(label.to_owned(), texture.index);
            return Ok(texture);
        };
        if first != identity {
            return Err(differs());
        }
        if let Some(&index) = self.store.borrow().shared.get(label) {
            return Ok(GameTexture {
                store: self.id,
                index,
            });
        }
        let hold = self.admit(label, declared.bytes)?;
        let texture = self.hold_texture(declared, hold);
        self.store
            .borrow_mut()
            .shared
            .insert(label.to_owned(), texture.index);
        Ok(texture)
    }

    /// Creates a declared texture, admitted against the budget, with the hold that keeps its
    /// bytes counted, but not yet held by a store.
    fn create_texture(
        &self,
        declaration: &GameTextureDeclaration<'_>,
    ) -> Result<(DeclaredTexture, ExternalHold), GameResourceError> {
        let descriptor = declaration.descriptor()?;
        beyond_limits(&self.device.limits(), &descriptor)?;
        if let Some(reason) = unsampleable(declaration.format, self.device.features()) {
            return Err(refuse(declaration.label, &reason));
        }
        let bytes = worldrunner_render_gpu::census::texture_backing_bytes(&descriptor);
        let hold = self.admit(declaration.label, bytes)?;
        let census = self.authority.census();
        let texture = match declaration.texels {
            Some(texels) => census.track_texture(
                ResourceClass::Texture,
                upload(&self.device, self.authority.queue(), declaration, texels)?,
            ),
            None => census.create_texture(&self.device, ResourceClass::Texture, &descriptor),
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(declaration.dimension),
            ..wgpu::TextureViewDescriptor::default()
        });
        let declared = DeclaredTexture {
            label: declaration.label.to_owned(),
            texture: Rc::new(texture),
            view,
            dimension: declaration.dimension,
            bytes,
        };
        Ok((declared, hold))
    }

    /// Holds an admitted texture in this store, counting its bytes, with the hold that keeps
    /// them in the runtime's ledger.
    fn hold_texture(&self, declared: DeclaredTexture, hold: ExternalHold) -> GameTexture {
        let mut store = self.store.borrow_mut();
        store.bytes += declared.bytes;
        store.textures.push(declared);
        store.texture_holds.push(hold);
        GameTexture {
            store: self.id,
            index: index(store.textures.len()),
        }
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
        let declared = self.texture(&store, texture)?;
        Ok(Import::texture_view(
            semantic,
            wgpu::Texture::clone(&declared.texture),
            declared.view.clone(),
            declared.dimension,
        ))
    }

    /// Level `level.mip` of layer `level.layer` of a declared texture as a 2D view of exactly
    /// that level, for the binding sourced from `semantic`: a `texture_2d` the shader samples
    /// or loads at its level 0. [`GameTargets::import_level`](super::GameTargets::import_level)
    /// does the same for a target.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's handle, and
    /// [`GameResourceError::Import`], naming the texture, for a 3D texture, a mip or layer
    /// outside the texture, or a texture without the usage `access` needs: a declared texture
    /// is sampled, never a storage image.
    pub fn import_texture_level(
        &self,
        semantic: EngineSemantic,
        texture: GameTexture,
        level: GameLevel,
        access: LevelAccess,
    ) -> Result<Import, GameResourceError> {
        let store = self.store.borrow();
        let declared = self.texture(&store, texture)?;
        import_level(
            &declared.label,
            &declared.texture,
            semantic,
            level,
            (access, wgpu::TextureViewDimension::D2),
        )
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

    fn texture<'a>(
        &self,
        store: &'a Store,
        texture: GameTexture,
    ) -> Result<&'a DeclaredTexture, GameResourceError> {
        store
            .textures
            .get(self.slot(texture.store, texture.index, GameTexture::KIND)?)
            .ok_or(GameResourceError::Foreign(GameTexture::KIND))
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

#[path = "resource_work.rs"]
mod work;
pub use work::{GameBlockDestination, GameLevel, GameReadback};

/// The index of the entry just pushed into a list of `len`.
fn index(len: usize) -> u32 {
    u32::try_from(len - 1).expect("fewer than 2^32 declared resources")
}
