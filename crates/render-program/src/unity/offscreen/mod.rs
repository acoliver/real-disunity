//! Offscreen work through the checked operations.
//!
//! A host registers shaders, meshes and source textures with an [`OffscreenState`] and hands it
//! one [`OffscreenFrame`] per frame. [`OffscreenState::build`] is the one entry point: it resolves
//! the player's programs, packs their constants and returns the typed operation list of the frame
//! in call order. [`OffscreenOperations::admit`] has the contract check it and the frame retain
//! its storage. The scene encodes the admitted operations before its own, with the frame's pass
//! timer, in the same submission.

mod constants_plan;
mod defaults;
mod error;
mod finish;
mod format;
pub mod frame;
mod meshes;
mod plan;
mod readback;
mod steps;
mod textures;
mod tracker;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use worldrunner_render_gpu::submission::{FrameLease, SubmissionAuthority};
use worldrunner_render_post::StageId;
use worldrunner_render_post::contracts::{
    CheckedDraw, RasterId, ResourceUse, TypedOperation, TypedWork, UseKind,
};

use crate::arena::ProgramArena;
use crate::budget::Reservation;
use crate::error::ProgramError;
use crate::ids::{CameraId, MeshId, ShaderId, TextureId};
use crate::offscreen::AdmittedOffscreen;
use crate::resources::{BufferHandle, Pins, TextureHandle};
use crate::runtime::ProgramRuntime;
use crate::sampler::SamplerCache;
use crate::unity::shader::ProgramShader;

use constants_plan::{ConstantsLimits, ConstantsPlan};
use defaults::DefaultBuffers;
use finish::Finish;
use frame::{Mesh, OffscreenFrame, SourceTexture};
use meshes::MeshTable;
use plan::{DrawCounts, ProgramEntry, Services};
use textures::TextureTable;
use tracker::Tracker;

pub use error::OffscreenError;
pub use readback::PendingImage;

/// What offscreen work keeps between frames: shaders, meshes, textures, programs and pipelines.
pub struct OffscreenState {
    shaders: HashMap<ShaderId, ProgramShader>,
    programs: HashMap<(ShaderId, usize, u32), Arc<ProgramEntry>>,
    textures: TextureTable,
    meshes: MeshTable,
    /// The constants inputs a mesh has no channel for read, made when the first frame is built.
    defaults: Option<DefaultBuffers>,
    samplers: SamplerCache,
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

    /// The runtime texture behind an id, for a draw that samples it after the offscreen work: the
    /// color of a render texture, or its depth when it has no color.
    #[must_use]
    pub fn texture_handle(&self, id: TextureId) -> Option<TextureHandle> {
        self.textures
            .get(id)
            .ok()
            .map(textures::TextureInfo::primary)
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

    /// Resolves and packs one frame into the typed operations that run it, in call order.
    ///
    /// Nothing is recorded and nothing is submitted. Call it while no frame is being recorded,
    /// so a render texture that changed can release its old storage at once.
    ///
    /// # Errors
    /// Fails by name on a record that names something the frame or the state lacks, a pass or
    /// variant the shader has none of, a vertex channel the mesh lacks, a copy that does not fit
    /// and a format this renderer does not draw into. Nothing of the frame is kept.
    pub fn build(
        &mut self,
        runtime: &mut ProgramRuntime,
        arena: &ProgramArena,
        frame: &OffscreenFrame,
    ) -> Result<OffscreenOperations, OffscreenError> {
        if !arena.belongs_to(runtime) {
            return Err(ProgramError::ForeignArena.into());
        }
        self.textures.sync(runtime, &frame.render_textures)?;
        let defaults = match self.defaults {
            Some(defaults) => defaults,
            None => *self.defaults.insert(DefaultBuffers::create(runtime)?),
        };
        let alignment = runtime
            .device()
            .limits()
            .min_uniform_buffer_offset_alignment;
        let mut tracker = Tracker::new(arena.lease());
        let mut constants = ConstantsPlan::new(alignment, self.constants_limits(runtime));
        let mut missing = BTreeSet::new();
        let mut counts = DrawCounts::default();
        let mut shares = Vec::new();
        let (steps, reads) = {
            let mut services = Services {
                runtime: &mut *runtime,
                shaders: &mut self.shaders,
                programs: &mut self.programs,
                samplers: &mut self.samplers,
                textures: &self.textures,
                meshes: &self.meshes,
                defaults,
                tracker: &mut tracker,
                constants: &mut constants,
                missing: &mut missing,
                counts: &mut counts,
                shares: &mut shares,
            };
            for spec in &frame.render_textures {
                services.import(spec.id)?;
            }
            let steps = steps::plan_steps(&mut services, frame)?;
            (steps, later_reads(&mut services, frame)?)
        };
        let (blocks, shared_blocks) = constants.block_counts();
        let constants_use = ConstantsUse {
            buffers: constants.chunks().len(),
            bytes: constants.total_bytes(),
            largest: constants.chunks().iter().map(Vec::len).max().unwrap_or(0) as u64,
            blocks,
            shared: shared_blocks,
        };
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
        Ok(OffscreenOperations {
            device: runtime.device().clone(),
            authority: runtime.authority().clone(),
            lease: arena.lease(),
            tracker,
            operations,
            reads,
            shares,
            missing: missing.into_iter().collect(),
            counts,
            constants_use,
        })
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

/// The typed operations of a frame, ready to be checked.
pub struct OffscreenOperations {
    device: wgpu::Device,
    authority: SubmissionAuthority,
    lease: FrameLease,
    tracker: Tracker,
    operations: Vec<TypedOperation>,
    reads: Vec<ResourceUse>,
    shares: Vec<Arc<Reservation>>,
    missing: Vec<String>,
    counts: DrawCounts,
    constants_use: ConstantsUse,
}

/// What a frame's constants took: the buffers they were uploaded into, their bytes, the largest
/// buffer's bytes, and the blocks packed against the blocks that took an earlier block's slice
/// because their bytes were equal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConstantsUse {
    pub buffers: usize,
    pub bytes: u64,
    pub largest: u64,
    pub blocks: usize,
    pub shared: usize,
}

/// Texture reads that bound only the levels written with the base level, counted for one
/// texture property, one set of levels and one pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialMipReads {
    /// The shader property the texture was bound under.
    pub texture: String,
    /// The levels bound and the levels the texture has.
    pub written: u32,
    pub total: u32,
    /// The tag key the drawing pass was selected by, or `index`.
    pub key: String,
    /// The value of the key on the pass drawn, or its index for `index`.
    pub tag: Option<String>,
    pub reads: usize,
}

/// What one camera render recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDraws {
    pub camera: CameraId,
    pub target: TextureId,
    /// The draws recorded into the target.
    pub drawn: usize,
    /// The draws left out because the shader has no pass the camera selects.
    pub unmatched: usize,
    /// The draws left out because a texture the pass samples is named by no layer.
    pub missing_texture: usize,
    /// The draws left out because the mesh lacks a vertex channel the pass reads.
    pub missing_channel: usize,
}

/// Why one camera render left draws out: the texture properties no layer named and the
/// (source channel, shader input) pairs its meshes lacked, each with the draws it cost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CameraCauses {
    pub textures: std::collections::BTreeMap<String, usize>,
    pub channels: std::collections::BTreeMap<(i8, i8), usize>,
}

/// Camera draws left out because the mesh lacks a vertex channel the pass reads, counted for one
/// source channel and the shader input it feeds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingChannelDraws {
    pub channel: i8,
    pub input: i8,
    pub draws: usize,
}

/// How one camera render drew: the formats of its target, the fixed-function state of the draws it
/// recorded and the constants and state properties its draws read that no layer gave a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraPassReport {
    pub camera: CameraId,
    pub target: TextureId,
    pub color: Option<wgpu::TextureFormat>,
    pub depth: Option<wgpu::TextureFormat>,
    /// The state of the recorded draws in words, with the number of draws that had each.
    pub states: Vec<(String, usize)>,
    /// The names of the values no layer gave, which the draws read as zero or the pass's default.
    pub without_value: Vec<String>,
}

/// Camera draws that read a shader input their mesh has no channel for and got a constant instead,
/// counted for one source channel, the shader input it feeds and the format the input was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultedChannelDraws {
    pub channel: i8,
    pub input: i8,
    pub format: wgpu::VertexFormat,
    pub draws: usize,
}

/// Camera draws left out because no layer names a texture the pass samples, counted for one
/// texture property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingTextureDraws {
    pub texture: String,
    pub draws: usize,
}

/// Camera draws left out for want of a pass, counted for the shaders whose passes carry the same
/// tag values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnmatchedPasses {
    /// The tag key the camera selects by, or `index` when it selects a pass by position.
    pub key: String,
    /// The values the camera selects, in order of preference.
    pub selected: Vec<String>,
    /// The values of the key that the shader's passes have, in pass order.
    pub tags: Vec<String>,
    /// The passes of the shader's first sub-shader.
    pub passes: usize,
    pub draws: usize,
}

impl OffscreenOperations {
    /// The camera draws left out because the material's shader has no pass the camera selects.
    /// A renderer without a matching pass is not drawn by that camera; the count keeps a selector
    /// that matches nothing from passing as an empty image.
    #[must_use]
    pub fn unmatched_draws(&self) -> usize {
        self.counts.unmatched
    }

    /// The camera draws left out because no layer names a texture the pass samples, by the
    /// texture property, sorted by name. The rest of the camera's draws are recorded.
    #[must_use]
    pub fn missing_texture_draws(&self) -> Vec<MissingTextureDraws> {
        self.counts
            .missing_textures
            .iter()
            .map(|(texture, draws)| MissingTextureDraws {
                texture: texture.clone(),
                draws: *draws,
            })
            .collect()
    }

    /// The camera draws left out because the mesh lacks a vertex channel the pass reads, by the
    /// channel and the input it feeds, sorted. The rest of the camera's draws are recorded.
    #[must_use]
    pub fn missing_channel_draws(&self) -> Vec<MissingChannelDraws> {
        self.counts
            .missing_channels
            .iter()
            .map(|(&(channel, input), &draws)| MissingChannelDraws {
                channel,
                input,
                draws,
            })
            .collect()
    }

    /// The draws that read an input their mesh has no channel for, which they got a constant for.
    #[must_use]
    pub fn defaulted_channel_draws(&self) -> Vec<DefaultedChannelDraws> {
        self.counts.defaulted.clone()
    }

    /// How each camera render drew, in the order of [`camera_draws`](Self::camera_draws).
    #[must_use]
    pub fn camera_passes(&self) -> Vec<CameraPassReport> {
        self.counts.passes.clone()
    }

    /// What each camera render recorded and left out, in call order.
    #[must_use]
    pub fn camera_draws(&self) -> Vec<CameraDraws> {
        self.counts.cameras.clone()
    }

    /// Why each camera render left draws out, in the order of [`camera_draws`](Self::camera_draws).
    #[must_use]
    pub fn camera_causes(&self) -> Vec<CameraCauses> {
        self.counts.camera_causes.clone()
    }

    /// The bytes of the frame's constants uploads: the constants of the draws that were kept, each
    /// block starting at the device's uniform offset alignment, and blocks of equal bytes shared.
    /// Zero when no draw has constants.
    #[must_use]
    pub fn constants_bytes(&self) -> u64 {
        self.constants_use.bytes
    }

    /// The buffers the constants are uploaded into: one unless the frame's constants exceed what a
    /// buffer may hold.
    #[must_use]
    pub fn constants_buffers(&self) -> usize {
        self.constants_use.buffers
    }

    /// The buffers, bytes and blocks of the frame's constants.
    #[must_use]
    pub fn constants_use(&self) -> ConstantsUse {
        self.constants_use
    }

    /// The programs this frame's draws built because the state had none for their shader, pass and
    /// variant yet. A frame whose draws all find theirs built none.
    #[must_use]
    pub fn programs_built(&self) -> usize {
        self.counts.programs_built
    }

    /// The time this frame's draws spent choosing and building their programs.
    #[must_use]
    pub fn program_time(&self) -> std::time::Duration {
        self.counts.program_time
    }

    /// The camera draws left out for want of a pass, by the tags the shader's passes have
    /// against the ones the camera selected.
    #[must_use]
    pub fn unmatched_passes(&self) -> Vec<UnmatchedPasses> {
        self.counts.unmatched_passes.clone()
    }

    /// The reads counted by `partial_mip_reads`, by texture property, levels and drawing pass.
    #[must_use]
    pub fn partial_mip_reads_by_pass(&self) -> Vec<PartialMipReads> {
        self.counts.partial_reads.clone()
    }

    /// The texture reads that bound only the levels written at the same point as the base level.
    #[must_use]
    pub fn partial_mip_reads(&self) -> usize {
        self.counts.partial_mips
    }

    /// The operations, in call order. The constants upload comes first when any draw has constants.
    #[must_use]
    pub fn operations(&self) -> &[TypedOperation] {
        &self.operations
    }

    /// The names of constant buffer members and state properties no layer supplied. They keep the
    /// packer's value, zero, or the pass's fixed state.
    #[must_use]
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// Has the contract check the whole list and the reads the scene will make after it, and has
    /// `encoder`'s frame retain the arena and the storage the operations use until the GPU has
    /// completed the frame. Nothing is retained when the check fails.
    ///
    /// # Errors
    /// Fails with the contract's error for the operation at fault: a read without a producer, a
    /// hazard, a missing usage; and when `encoder` is not a frame of the runtime's authority or
    /// the arena has retired.
    pub fn admit(
        self,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<AdmittedOffscreen, OffscreenError> {
        let registry = self.tracker.registry;
        let compiled =
            registry.compile_operations_with_reads(&self.device, &self.operations, &self.reads)?;
        let admitted = AdmittedOffscreen::new(
            compiled,
            registry,
            self.lease,
            Pins::new(&self.authority, self.shares),
        );
        admitted.retain(encoder)?;
        Ok(admitted)
    }
}
