//! Offscreen work through the checked operations.
//!
//! A host registers shaders, meshes and source textures with an [`OffscreenState`] and hands it
//! one [`OffscreenFrame`] per frame. [`OffscreenState::build`] is the one entry point: it resolves
//! the player's programs, packs their constants and returns the typed operation list of the frame
//! in call order. [`OffscreenOperations::admit`] has the contract check it and the frame retain
//! its storage. The scene encodes the admitted operations before its own, with the frame's pass
//! timer, in the same submission.

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

use worldrunner_render_gpu::submission::{FrameLease, Publications, SubmissionAuthority};
use worldrunner_render_post::contracts::{
    CheckedDraw, CompiledOperations, ContractError, RasterId, ResourceRegistry, ResourceUse,
    TypedOperation, TypedWork, UseKind,
};
use worldrunner_render_post::{StageId, TimingState};

use crate::arena::ProgramArena;
use crate::budget::Reservation;
use crate::error::ProgramError;
use crate::ids::{MeshId, ShaderId, TextureId};
use crate::resources::{BufferHandle, Pins, TextureHandle};
use crate::runtime::ProgramRuntime;
use crate::sampler::SamplerCache;
use crate::shader::ProgramShader;

use finish::Finish;
use frame::{Mesh, OffscreenFrame, SourceTexture};
use meshes::MeshTable;
use plan::{ConstantsPlan, ProgramEntry, Services};
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
    samplers: SamplerCache,
    draws: HashMap<blake3::Hash, CheckedDraw>,
    /// The buffer every draw's constants are uploaded into, and its size.
    constants: Option<(BufferHandle, u64)>,
    retired_constants: Vec<BufferHandle>,
}

impl OffscreenState {
    #[must_use]
    pub fn new(runtime: &ProgramRuntime) -> Self {
        Self {
            shaders: HashMap::new(),
            programs: HashMap::new(),
            textures: TextureTable::default(),
            meshes: MeshTable::default(),
            samplers: SamplerCache::new(runtime.device()),
            draws: HashMap::new(),
            constants: None,
            retired_constants: Vec::new(),
        }
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
        let alignment = runtime
            .device()
            .limits()
            .min_uniform_buffer_offset_alignment;
        let mut tracker = Tracker::new(arena.lease());
        let mut constants = ConstantsPlan::new(alignment);
        let mut missing = BTreeSet::new();
        let mut shares = Vec::new();
        let (steps, reads) = {
            let mut services = Services {
                runtime: &mut *runtime,
                shaders: &mut self.shaders,
                programs: &mut self.programs,
                samplers: &mut self.samplers,
                textures: &self.textures,
                meshes: &self.meshes,
                tracker: &mut tracker,
                constants: &mut constants,
                missing: &mut missing,
                shares: &mut shares,
            };
            for spec in &frame.render_textures {
                services.import(spec.id)?;
            }
            let steps = steps::plan_steps(&mut services, frame)?;
            (steps, later_reads(&mut services, frame)?)
        };
        let mut operations = Vec::new();
        let mut constants_key = None;
        if let Some((key, upload)) =
            self.upload_constants(runtime, &mut tracker, &constants, &mut shares)?
        {
            constants_key = Some(key);
            operations.push(upload);
        }
        let mut finish = Finish {
            runtime,
            tracker: &mut tracker,
            draws: &mut self.draws,
            constants: constants_key,
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
        })
    }

    /// The upload of the frame's constants into the constants buffer, which is created or made
    /// larger when the frame needs it.
    fn upload_constants(
        &mut self,
        runtime: &mut ProgramRuntime,
        tracker: &mut Tracker,
        constants: &ConstantsPlan,
        shares: &mut Vec<Arc<Reservation>>,
    ) -> Result<
        Option<(
            worldrunner_render_post::contracts::AllocationKey,
            TypedOperation,
        )>,
        OffscreenError,
    > {
        let needed = constants.bytes.len() as u64;
        if needed == 0 {
            return Ok(None);
        }
        let handle = self.constants_buffer(runtime, needed)?;
        shares.push(runtime.store.buffer_reservation(handle)?);
        let buffer = runtime.gpu_buffer(handle)?;
        let key = tracker.import_buffer(
            &buffer,
            worldrunner_render_post::contracts::ResourceLifetime::FrameLocal,
            None,
        )?;
        let destination = tracker.buffer_view(key, 0..needed, 1)?;
        let upload = TypedOperation::new(
            RasterId::new(StageId::new("offscreen constants", 0), 0),
            TypedWork::Upload {
                destination,
                bytes: constants.bytes.clone(),
            },
        );
        Ok(Some((key, upload)))
    }

    fn constants_buffer(
        &mut self,
        runtime: &mut ProgramRuntime,
        needed: u64,
    ) -> Result<BufferHandle, OffscreenError> {
        self.retired_constants
            .retain(|buffer| matches!(runtime.release_buffer(*buffer), Err(ProgramError::Busy)));
        if let Some((handle, _)) = self.constants.filter(|(_, size)| *size >= needed) {
            return Ok(handle);
        }
        let size = needed.next_power_of_two().max(4096);
        let len = usize::try_from(size).map_err(|_| {
            ProgramError::Gpu(format!("{size} bytes of constants do not fit memory"))
        })?;
        let handle = runtime.create_buffer("offscreen constants", &vec![0; len])?;
        if let Some((old, _)) = self.constants.replace((handle, size)) {
            self.retired_constants.push(old);
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
}

impl OffscreenOperations {
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
        let admitted = AdmittedOffscreen {
            compiled,
            registry,
            lease: self.lease,
            pins: Pins::new(&self.authority, self.shares),
        };
        admitted.retain(encoder)?;
        Ok(admitted)
    }
}

/// Operations the contract accepted, held by a frame's retained arena. Only
/// [`OffscreenOperations::admit`] makes one.
pub struct AdmittedOffscreen {
    compiled: CompiledOperations,
    registry: ResourceRegistry,
    lease: FrameLease,
    pins: Pins,
}

impl std::fmt::Debug for AdmittedOffscreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmittedOffscreen").finish_non_exhaustive()
    }
}

impl AdmittedOffscreen {
    /// The identity of each operation, in order, as the frame's timing names them.
    pub fn ids(&self) -> impl Iterator<Item = RasterId> + '_ {
        self.compiled.ids()
    }

    /// Makes the frame `encoder` records retain the arena and the storage of the operations.
    /// Admission does this once; a consumer that encodes into a later frame does it again.
    ///
    /// # Errors
    /// Fails with `Expired` when the arena has retired, with `ForeignAuthority` for a frame of
    /// another authority and when `encoder` is not a frame being recorded.
    pub fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.lease.check()?;
        Publications::retain_lease(encoder, &self.lease)?;
        self.pins.retain(encoder)
    }

    /// Encodes every operation, in order, into `encoder`. With the frame's `TimingState` each
    /// operation is a timed pass of the frame.
    ///
    /// # Errors
    /// Fails with the contract's error when a resource was replaced or the arena retired since
    /// admission.
    pub fn encode(
        &self,
        timing: Option<&TimingState>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), ContractError> {
        match timing {
            Some(timing) => timing.encode_checked(&self.compiled, &self.registry, encoder),
            None => self.compiled.encode_checked(&self.registry, encoder, None),
        }
    }
}
