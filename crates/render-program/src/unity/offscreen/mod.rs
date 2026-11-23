//! Offscreen work through the checked operations.
//!
//! A host registers shaders, meshes and source textures with an [`OffscreenState`] and hands it
//! one [`OffscreenFrame`] per frame. [`OffscreenState::build`] is the one entry point: it resolves
//! the player's programs, packs their constants and returns the typed operation list of the frame
//! in call order. [`OffscreenState::check`] plans a frame without counting anything it would
//! write, for a host that builds parts of a refused frame to find the one at fault.
//! [`OffscreenOperations::admit`] has the contract check it and the frame retain its storage. The
//! scene encodes the admitted operations before its own, with the frame's pass timer, in the same
//! submission.

mod blit_steps;
mod build;
mod constants_plan;
mod counts;
mod defaults;
mod depth_range;
mod dots;
mod error;
mod exports;
mod finish;
mod format;
pub mod frame;
mod list_steps;
mod meshes;
mod mips;
mod operations;
mod pass_report;
mod plan;
mod probe;
mod procedural;
mod raster;
mod readback;
mod resample;
mod sampling;
mod slot_dimension;
mod steps;
mod storage_buffers;
mod tap;
mod texel_size;
mod texture_binding;
mod textures;
mod tracker;
mod views;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use worldrunner_render_post::StageId;
use worldrunner_render_post::contracts::{
    CheckedDraw, RasterId, ResourceUse, TypedOperation, TypedWork, UseKind,
};

use crate::arena::ProgramArena;
use crate::budget::Reservation;
use crate::error::ProgramError;
use crate::ids::{MeshId, ShaderId, TextureId};
use crate::program::GraphicsProgram;
use crate::resources::{BufferHandle, TextureHandle};
use crate::runtime::ProgramRuntime;
use crate::sampler::SamplerCache;
use crate::unity::shader::ProgramShader;

use constants_plan::{ConstantsLimits, ConstantsPlan};
use counts::DrawCounts;
use defaults::DefaultBuffers;
use finish::Finish;
use frame::{Mesh, OffscreenFrame, SourceTexture};
use meshes::MeshTable;
use plan::{ProgramEntry, Services};
use textures::TextureTable;
use tracker::Tracker;

pub use error::{OffscreenError, Refusal};
pub use format::RENDER_FORMATS;
pub use operations::{
    CameraCauses, CameraDraws, CameraPassReport, ConstantsUse, DefaultedChannelDraws,
    FeedbackLoopDraws, ListDraws, MissingBufferDraws, MissingChannelDraws, MissingTextureDraws,
    OffscreenOperations, PartialMipReads, StandInDraws, UnmatchedPasses,
};
pub use probe::{
    CONSTANT_COLOR, CameraProbe, ClipCounts, MemberOrigin, Origin, PassSetup, ProgramProbe,
    SampledTexture, StateVariant, TriangleCounts,
};
pub use readback::{PendingImage, TapRead};
pub use steps::DRAW_LAYERS;
pub use views::ViewCounts;

/// What offscreen work keeps between frames: shaders, meshes, textures, programs and pipelines.
pub struct OffscreenState {
    shaders: HashMap<ShaderId, ProgramShader>,
    programs: HashMap<(ShaderId, usize, u32), Arc<ProgramEntry>>,
    textures: TextureTable,
    meshes: MeshTable,
    /// The constants inputs a mesh has no channel for read, made when the first frame is built.
    defaults: Option<DefaultBuffers>,
    samplers: SamplerCache,
    /// The built-in program of a blit between textures of other sizes or formats.
    resample: Option<GraphicsProgram>,
    draws: HashMap<blake3::Hash, CheckedDraw>,
    /// The buffers the frame's constants are uploaded into, one for each chunk, with their sizes.
    constants: Vec<(BufferHandle, u64)>,
    /// The most bytes one constants buffer may hold, when the host lowers it from the default.
    constants_limit: Option<u64>,
    /// The most bytes all of a frame's constants may take, when the host lowers it from what the
    /// runtime's budget has left.
    constants_total_limit: Option<u64>,
    retired_constants: Vec<BufferHandle>,
}

/// The most bytes a constants buffer holds unless the host sets less. A frame with more constants
/// than this is uploaded in several buffers.
const CONSTANTS_CHUNK: u64 = 32 << 20;

impl OffscreenState {
    #[must_use]
    pub fn new(runtime: &ProgramRuntime) -> Self {
        Self {
            shaders: HashMap::new(),
            programs: HashMap::new(),
            textures: TextureTable::default(),
            meshes: MeshTable::default(),
            defaults: None,
            samplers: SamplerCache::new(runtime.device()),
            resample: None,
            draws: HashMap::new(),
            constants: Vec::new(),
            constants_limit: None,
            constants_total_limit: None,
            retired_constants: Vec::new(),
        }
    }

    /// Sets the most bytes one constants buffer may hold, which the device's buffer size also
    /// limits. A frame whose constants are larger is uploaded in several buffers, each draw
    /// binding the buffer and offset that hold its bytes.
    pub fn set_constants_limit(&mut self, bytes: u64) {
        self.constants_limit = Some(bytes);
    }

    /// Sets the most bytes all of a frame's constants may take. By default that is what the
    /// runtime's budget has left, since the constants are storage the runtime holds.
    pub fn set_constants_total_limit(&mut self, bytes: u64) {
        self.constants_total_limit = Some(bytes);
    }

    /// The name the source of shader `id` declares, if the state holds it.
    #[must_use]
    pub fn shader_name(&self, id: ShaderId) -> Option<&str> {
        self.shaders.get(&id).map(ProgramShader::name)
    }

    pub fn add_shader(&mut self, id: ShaderId, shader: ProgramShader) {
        self.shaders.insert(id, shader);
        self.programs.retain(|(shader, _, _), _| *shader != id);
    }

    /// Uploads a source texture, replacing the one of that id.
    ///
    /// # Errors
    /// Fails on a format this renderer does not sample and when the upload fails.
    pub fn set_texture(
        &mut self,
        runtime: &mut ProgramRuntime,
        texture: &SourceTexture,
    ) -> Result<(), OffscreenError> {
        self.textures.set_source(runtime, texture)
    }

    /// Uploads a mesh, replacing the one of that id.
    ///
    /// # Errors
    /// Fails when a channel's data does not fit the vertex count, a sub-mesh reaches past the
    /// mesh, or the upload fails.
    pub fn set_mesh(
        &mut self,
        runtime: &mut ProgramRuntime,
        id: MeshId,
        mesh: &Mesh,
    ) -> Result<(), OffscreenError> {
        self.meshes.set(runtime, id, mesh)
    }

    /// Gives back the storage of the uploaded source texture `id` and returns its bytes, or `None`
    /// for an id that names no uploaded source. The storage counts against the budget until the
    /// GPU has finished with it and the runtime collects it. A render texture is released by
    /// [`retain_render_textures`](Self::retain_render_textures), not here.
    pub fn release_source_texture(
        &mut self,
        runtime: &mut ProgramRuntime,
        id: TextureId,
    ) -> Option<u64> {
        self.textures.release_source(runtime, id)
    }

    /// Gives back the storage of mesh `id` and returns its bytes, or `None` for a mesh the state
    /// does not hold. The storage counts against the budget until the runtime collects it.
    pub fn release_mesh(&mut self, runtime: &mut ProgramRuntime, id: MeshId) -> Option<u64> {
        self.meshes.release(runtime, id)
    }

    /// The bytes the render textures of `specs` would add to the runtime's storage: those the
    /// state has not made or must replace for different storage or storage-binding capability.
    /// This includes both color and depth while the old storage remains live. A host that must
    /// make room asks before it builds the frame.
    #[must_use]
    pub fn new_render_texture_bytes(
        &self,
        runtime: &ProgramRuntime,
        specs: &[frame::RenderTextureSpec],
    ) -> u64 {
        self.textures.new_render_bytes(runtime, specs)
    }

    /// The runtime texture behind an id, for a draw that samples it after the offscreen work: the
    /// color of a render texture, or its depth when it has no color.
    #[must_use]
    pub fn texture_handle(&self, id: TextureId) -> Option<TextureHandle> {
        self.textures
            .get(id)
            .ok()
            .map(textures::TextureInfo::primary)
    }

    /// Whether `id` is a render texture the state holds that no frame it built has written. Storage
    /// made again starts unwritten, and a texture a kernel writes as a storage image is never
    /// counted as unwritten. An id the state holds no render texture for is not one.
    #[must_use]
    pub fn render_texture_is_unwritten(&self, id: TextureId) -> bool {
        self.textures.never_written(id)
    }

    /// The render textures the state holds and the bytes each takes in the runtime's budget, in
    /// id order. A host that must make room chooses among them.
    #[must_use]
    pub fn render_textures_held(&self) -> Vec<(TextureId, u64)> {
        self.textures.render_held()
    }

    /// Gives back the storage of the render texture `id` and returns its bytes, or `None` for an id
    /// that names no render texture the state holds. The next frame that names the texture makes
    /// it again, empty. The storage counts against the budget until the GPU has finished with it
    /// and the runtime collects it.
    pub fn release_render_texture(
        &mut self,
        runtime: &mut ProgramRuntime,
        id: TextureId,
    ) -> Option<u64> {
        self.textures.release_render(runtime, id)
    }

    /// The bytes of staging a read of the first mip of texture `id` takes from the budget, or
    /// `None` for an id the state does not hold and for a format that cannot be copied by byte.
    #[must_use]
    pub fn read_bytes(&self, id: TextureId) -> Option<u64> {
        self.textures.read_bytes(id)
    }

    /// Marks render textures as ones a kernel binds as storage images. A marked texture is made
    /// with storage binding usage when the device binds its color format as a storage image, and
    /// is made again, empty, by the first frame that names it after it was marked while it had
    /// none. A texture stays marked, so later frames keep it. A marked texture whose format the
    /// device cannot bind is made without storage binding, as [`texture_storage`](Self::texture_storage)
    /// shows.
    pub fn bind_as_storage(&mut self, ids: impl IntoIterator<Item = TextureId>) {
        self.textures.bind_as_storage(ids);
    }

    /// The color format of the render texture `id` and whether it allows storage binding, or
    /// `None` for an id that names no texture the state holds. A depth-only render texture
    /// reports its depth format.
    #[must_use]
    pub fn texture_storage(&self, id: TextureId) -> Option<(wgpu::TextureFormat, bool)> {
        self.textures
            .get(id)
            .ok()
            .map(|info| (info.format, info.storage))
    }

    /// The dimension of the view a draw or a kernel samples the texture through: the one its
    /// spec names, and for an uploaded source 2D, cube or 2D array by its
    /// [`SourceDimension`](frame::SourceDimension). `None` for an id that names no texture the
    /// state holds.
    #[must_use]
    pub fn texture_view_dimension(&self, id: TextureId) -> Option<wgpu::TextureViewDimension> {
        let info = self.textures.get(id).ok()?;
        info.dimension.view_dimension().ok()
    }

    /// The dimension of the view a kernel binds the texture as a storage image through. It is
    /// the sampled view, except that a cube or a cube array is written face by face through a 2D
    /// array of their faces, since a storage image has no cube view. `None` for an id that names
    /// no texture the state holds.
    #[must_use]
    pub fn storage_view_dimension(&self, id: TextureId) -> Option<wgpu::TextureViewDimension> {
        let view = self.texture_view_dimension(id)?;
        Some(match view {
            wgpu::TextureViewDimension::Cube | wgpu::TextureViewDimension::CubeArray => {
                wgpu::TextureViewDimension::D2Array
            }
            other => other,
        })
    }

    /// The dimension, with its layers or depth, the state made the texture `id` with, or `None`
    /// for an id that names no texture the state holds.
    #[must_use]
    pub fn texture_dimension(&self, id: TextureId) -> Option<frame::RenderDimension> {
        self.textures.get(id).ok().map(|info| info.dimension)
    }

    /// Gives back the storage of the render textures that `live` does not name. A render texture
    /// the frames named earlier and its owner has since destroyed holds storage for as long as the
    /// state keeps it, so a host calls this with the render textures that exist before it builds.
    /// Call it while no frame is being recorded; storage the runtime cannot release yet is
    /// released by the next call.
    pub fn retain_render_textures(
        &mut self,
        runtime: &mut ProgramRuntime,
        live: &BTreeSet<TextureId>,
    ) {
        self.textures.retain_render_textures(runtime, live);
    }

    /// Plans one frame into its operations and the render textures they write, and keeps nothing of
    /// what the frame writes: [`build`](Self::build) counts the writes once the frame is built, and
    /// [`check`](Self::check) never does.
    fn assemble(
        &mut self,
        runtime: &mut ProgramRuntime,
        arena: &ProgramArena,
        frame: &OffscreenFrame,
    ) -> Result<(OffscreenOperations, Vec<TextureId>), Refusal> {
        if !arena.belongs_to(runtime) {
            return Err(OffscreenError::from(ProgramError::ForeignArena).into());
        }
        self.textures.sync(runtime, &frame.render_textures)?;
        let mut defaults = match self.defaults {
            Some(defaults) => defaults,
            None => DefaultBuffers::create(runtime)?,
        };
        if dots::frame_has_batch(frame) {
            defaults.provide_indirect(runtime)?;
        }
        self.defaults = Some(defaults);
        let alignment = runtime
            .device()
            .limits()
            .min_uniform_buffer_offset_alignment;
        let mut tracker = Tracker::new(arena.lease());
        let mut constants = ConstantsPlan::new(alignment, self.constants_limits(runtime));
        let mut missing = BTreeSet::new();
        let mut counts = DrawCounts::default();
        let mut shares = Vec::new();
        let (steps, taps, reads) = {
            let mut services = Services {
                runtime: &mut *runtime,
                shaders: &mut self.shaders,
                programs: &mut self.programs,
                samplers: &mut self.samplers,
                resample: &mut self.resample,
                textures: &self.textures,
                meshes: &self.meshes,
                undeclared_textures: frame.undeclared_textures,
                defaults,
                tracker: &mut tracker,
                constants: &mut constants,
                missing: &mut missing,
                counts: &mut counts,
                shares: &mut shares,
                probing: false,
                constant_fragment: false,
            };
            // A 3D texture is for kernels; only a record that names it is refused.
            for spec in &frame.render_textures {
                if !spec.dimension.is_volume() {
                    services.import(spec.id)?;
                }
            }
            let (steps, taps) = steps::plan_steps(&mut services, frame)?;
            (steps, taps, later_reads(&mut services, frame)?)
        };
        let (blocks, shared_blocks) = constants.block_counts();
        let constants_use = ConstantsUse {
            buffers: constants.chunks().len(),
            bytes: constants.total_bytes(),
            largest: constants.chunks().iter().map(Vec::len).max().unwrap_or(0) as u64,
            blocks,
            shared: shared_blocks,
        };
        let planned_draws = steps.iter().map(steps::Step::planned_draws).sum();
        let mut operations = Vec::new();
        let mut constants_keys = Vec::new();
        for (key, upload) in
            self.upload_constants(runtime, &mut tracker, &constants, &mut shares)?
        {
            constants_keys.push(key);
            operations.push(upload);
        }
        let mut finish = Finish {
            runtime,
            tracker: &mut tracker,
            draws: &mut self.draws,
            constants: constants_keys,
        };
        operations.extend(finish.operations(steps)?);
        let written = tracker.written_textures();
        let operations = OffscreenOperations {
            device: runtime.device().clone(),
            authority: runtime.authority().clone(),
            lease: arena.lease(),
            tracker,
            operations,
            reads,
            exports: Vec::new(),
            shares,
            missing: missing.into_iter().collect(),
            counts,
            constants_use,
            planned_draws,
            taps,
        };
        Ok((operations, written))
    }

    /// What the frame's constants must stay within: a block fits a uniform binding, a chunk fits a
    /// buffer, and all of them fit what the runtime's budget has left.
    fn constants_limits(&self, runtime: &ProgramRuntime) -> ConstantsLimits {
        let limits = runtime.device().limits();
        let chunk = self
            .constants_limit
            .unwrap_or(CONSTANTS_CHUNK)
            .min(limits.max_buffer_size);
        ConstantsLimits {
            block: limits.max_uniform_buffer_binding_size,
            // A buffer's size is a multiple of four.
            chunk: chunk - chunk % 4,
            total: self.constants_total_limit.unwrap_or_else(|| {
                // The buffers this state already holds are reused or given back, so they are free.
                let held: u64 = self.constants.iter().map(|(_, size)| size).sum();
                runtime
                    .budget()
                    .0
                    .saturating_sub(runtime.live_bytes())
                    .saturating_add(held)
            }),
        }
    }

    /// The upload of each chunk of the frame's constants into a constants buffer, which is created
    /// or made larger when the chunk needs it. Buffers past the first that no chunk needs are given back.
    fn upload_constants(
        &mut self,
        runtime: &mut ProgramRuntime,
        tracker: &mut Tracker,
        constants: &ConstantsPlan,
        shares: &mut Vec<Arc<Reservation>>,
    ) -> Result<
        Vec<(
            worldrunner_render_post::contracts::AllocationKey,
            TypedOperation,
        )>,
        OffscreenError,
    > {
        self.retired_constants
            .retain(|buffer| matches!(runtime.release_buffer(*buffer), Err(ProgramError::Busy)));
        // The first buffer stays for the next frame whatever it needs; the others, which only a
        // frame with more constants than one buffer holds uses, are given back.
        let kept = constants.chunks().len().max(1).min(self.constants.len());
        self.retired_constants.extend(
            self.constants
                .split_off(kept)
                .into_iter()
                .map(|(buffer, _)| buffer),
        );
        let mut uploads = Vec::new();
        for (index, chunk) in constants.chunks().iter().enumerate() {
            let needed = chunk.len() as u64;
            let handle = self.constants_buffer(runtime, index, needed, constants.chunk_limit())?;
            shares.push(runtime.store.buffer_reservation(handle)?);
            let buffer = runtime.gpu_buffer(handle)?;
            let key = tracker.import_buffer(
                &buffer,
                worldrunner_render_post::contracts::ResourceLifetime::FrameLocal,
                None,
            )?;
            let destination = tracker.buffer_view(key, 0..needed, 1)?;
            let upload = TypedOperation::new(
                RasterId::new(
                    StageId::new("offscreen constants", 0),
                    u32::try_from(index).unwrap_or(u32::MAX),
                ),
                TypedWork::Upload {
                    destination,
                    bytes: chunk.clone(),
                },
            );
            uploads.push((key, upload));
        }
        Ok(uploads)
    }

    /// The buffer of chunk `index`, made larger than any it had when `needed` does not fit it. A
    /// buffer is a power of two bytes, or `limit` when that is less.
    fn constants_buffer(
        &mut self,
        runtime: &mut ProgramRuntime,
        index: usize,
        needed: u64,
        limit: u64,
    ) -> Result<BufferHandle, OffscreenError> {
        if let Some(&(handle, size)) = self.constants.get(index)
            && size >= needed
        {
            return Ok(handle);
        }
        let size = needed.next_power_of_two().max(4096).min(limit).max(needed);
        let len = usize::try_from(size).map_err(|_| {
            ProgramError::Gpu(format!("{size} bytes of constants do not fit memory"))
        })?;
        let handle = runtime.create_buffer("offscreen constants", &vec![0; len])?;
        if index < self.constants.len() {
            let (old, _) = std::mem::replace(&mut self.constants[index], (handle, size));
            self.retired_constants.push(old);
        } else {
            self.constants.push((handle, size));
        }
        Ok(handle)
    }
}

/// The sampled reads of the ranges the host names, at the versions the frame leaves them.
fn later_reads(
    services: &mut Services<'_>,
    frame: &OffscreenFrame,
) -> Result<Vec<ResourceUse>, OffscreenError> {
    let mut reads = Vec::new();
    for range in &frame.later_reads {
        let (imported, _) = services.import(range.texture)?;
        for (mip, layer) in range
            .mips
            .clone()
            .flat_map(|mip| range.layers.clone().map(move |layer| (mip, layer)))
        {
            if mip >= imported.mips || layer >= imported.layers {
                return Err(OffscreenError::NoSuchSubresource {
                    texture: range.texture,
                    mip,
                    element: layer,
                });
            }
            let version = services.tracker.cell(imported.primary, mip, layer);
            let view = services.tracker.cell_view(
                imported.primary,
                (mip, layer),
                wgpu::TextureAspect::All,
                version,
            )?;
            steps::push_use(&mut reads, ResourceUse::read(view, UseKind::Sampled));
        }
    }
    Ok(reads)
}
