//! The runtime: the caller's device and submission authority with the resources, modules and
//! pipelines built on them.

use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet};
use worldrunner_render_gpu::submission::{Publications, SubmissionAuthority};

use crate::budget::{Ledger, Note, Reservation};
use crate::error::ProgramError;
use crate::module::StageProgram;
use crate::registry::Registry;
use crate::resources::{Pins, Store};
use crate::stats::Counters;

pub use crate::arena::{AdmittedPrograms, ProgramArena};
pub use crate::batch::{
    GeometryDraw, PassCommands, PipelineDraw, PreparedPipeline, VertexBufferSpec, VertexLayoutSpec,
};
pub use crate::budget::Holding;
pub use crate::dispatch::DispatchRequest;
pub use crate::draw::{DrawRequest, Geometry, VertexInput};
pub use crate::import::{
    Import, ImportedDraw, ImportedGeometry, ImportedIndices, ImportedResource, ImportedTexture,
    ImportedVertexInput,
};
pub use crate::material::{MaterialDraw, MaterialHandle, MaterialPacket};
pub use crate::packet::{DrawPacket, MAX_COLOR_ATTACHMENTS, PreparedProgramDraw, TargetSignature};
pub use crate::program::{ComputeProgram, ExternalSet, GraphicsProgram};
pub use crate::readback::{PendingBufferRead, PendingTargetRead, TargetImage};
pub use crate::registry::{ProgramDesc, ProgramHandle, StageDesc, VariantDesc};
pub use crate::resources::{
    BoundResource, BufferHandle, Keepalive, Resource, TargetHandle, TextureHandle,
};
pub use crate::set::{ImportTable, PreparedSet};
pub use crate::stats::RuntimeStats;

/// Shares the caller's wgpu device and `SubmissionAuthority`, owns the resources it creates by
/// handle, and caches shader modules and pipelines keyed by their content.
///
/// The runtime records into frames and never submits. A caller opens a frame with
/// `SubmissionAuthority::begin`, passes it to `draw`, `dispatch` and the readbacks, and
/// submits it. The runtime reaches the queue only to upload initial texture contents.
///
/// Every buffer, texture, render target, depth attachment and readback staging buffer is
/// admitted against the `AllocationBudget` given at construction before it is created. Pipelines,
/// shader modules, bind groups and the sampler are not counted: render-gpu's allocation sets
/// describe buffer, texture and readback storage only.
pub struct ProgramRuntime {
    pub(crate) device: wgpu::Device,
    pub(crate) authority: SubmissionAuthority,
    pub(crate) store: Store,
    /// Registered programs and the materials made from them.
    pub(crate) registry: Registry,
    pub(crate) ledger: Ledger,
    pub(crate) budget: AllocationBudget,
    /// The one sampler every sampler binding gets: linear filtering, clamped to the edge.
    pub(crate) sampler: wgpu::Sampler,
    modules: HashMap<blake3::Hash, wgpu::ShaderModule>,
    /// Bind group layouts by the slots they were made for, so programs with equal slots at a set
    /// share one. Programs are built through `&self`, hence the lock.
    set_layouts: Mutex<HashMap<blake3::Hash, wgpu::BindGroupLayout>>,
    pub(crate) render_pipelines: HashMap<blake3::Hash, wgpu::RenderPipeline>,
    pub(crate) compute_pipelines: HashMap<blake3::Hash, wgpu::ComputePipeline>,
    pub(crate) counters: Counters,
}

/// Gives each runtime its own number, so a handle from another runtime is refused.
static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(1);

impl ProgramRuntime {
    #[must_use]
    pub fn new(
        device: wgpu::Device,
        authority: SubmissionAuthority,
        budget: AllocationBudget,
    ) -> Self {
        let sampler = device.create_sampler(&crate::sampler::fixed_descriptor());
        Self {
            device,
            authority,
            store: Store::default(),
            registry: Registry::new(NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed)),
            ledger: Ledger::default(),
            budget,
            sampler,
            modules: HashMap::new(),
            set_layouts: Mutex::new(HashMap::new()),
            render_pipelines: HashMap::new(),
            compute_pipelines: HashMap::new(),
            counters: Counters::default(),
        }
    }

    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The authority every frame this runtime records into must come from.
    #[must_use]
    pub fn authority(&self) -> &SubmissionAuthority {
        &self.authority
    }

    #[must_use]
    pub fn budget(&self) -> AllocationBudget {
        self.budget
    }

    /// Descriptor bytes held now: live resources, released resources that submitted work may
    /// still use until [`collect`](Self::collect) sees that work complete, and the staging of
    /// readbacks and writes until their frames are cancelled, dropped or completed.
    #[must_use]
    pub fn live_bytes(&self) -> u64 {
        self.ledger.live_bytes()
    }

    /// The most descriptor bytes this runtime has held at once.
    #[must_use]
    pub fn peak_bytes(&self) -> u64 {
        self.ledger.peak_bytes()
    }

    /// What [`live_bytes`](Self::live_bytes) is made of, by the label each resource was created
    /// under and its shape, the greatest first. Released resources the GPU may still use are
    /// in it until [`collect`](Self::collect) frees them.
    #[must_use]
    pub fn holdings(&self) -> Vec<Holding> {
        self.ledger.holdings()
    }

    /// Frees released resources whose last use has completed on the GPU. Every allocation does
    /// this first.
    pub fn collect(&self) {
        self.store.collect(self.authority.completed_serial());
    }

    /// Admits `allocations` against the budget on top of everything live, or refuses them by
    /// name before anything is created.
    pub(crate) fn admit(
        &self,
        allocations: AllocationSet,
        note: Note,
    ) -> Result<Reservation, ProgramError> {
        self.collect();
        self.ledger
            .admit(&self.device.limits(), self.budget, allocations, note)
    }

    /// Checks that `encoder` records a frame the authority is preparing and that the device is
    /// still usable, so recorded work can only reach the queue through the authority.
    ///
    /// # Errors
    /// Fails with the device-loss message or when the encoder is not the authority's.
    pub(crate) fn recording(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.authority.check_device()?;
        Publications::for_encoder(encoder)?;
        Ok(())
    }

    /// Makes the frame `encoder` records hold `shares`, the reservations of the resources a call
    /// reads or writes, until the frame is cancelled, dropped or completed on the GPU, whatever
    /// happens to the runtime meanwhile. Every entry point that records into a frame does this
    /// once its call is valid, so they share one retention rule.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when `encoder` belongs to another authority and when it is
    /// not a frame being recorded.
    pub(crate) fn hold_for_frame(
        &self,
        encoder: &wgpu::CommandEncoder,
        shares: Vec<Arc<Reservation>>,
    ) -> Result<(), ProgramError> {
        Pins::new(&self.authority, shares).retain(encoder)
    }

    /// Shader modules compiled so far; stages with the same content share one.
    #[must_use]
    pub fn cached_modules(&self) -> usize {
        self.modules.len()
    }

    /// Render and compute pipelines built so far; equal programs and state share one.
    #[must_use]
    pub fn cached_pipelines(&self) -> usize {
        self.render_pipelines.len() + self.compute_pipelines.len()
    }

    /// What this runtime has built since it was made. Every count only rises, so a cost test
    /// compares a snapshot taken before a frame with one taken after.
    #[must_use]
    pub fn stats(&self) -> RuntimeStats {
        self.counters.snapshot()
    }

    /// The interned bind group layouts. A layout is only ever added, so a panic while the lock
    /// was held leaves the map consistent.
    pub(crate) fn lock_set_layouts(
        &self,
    ) -> MutexGuard<'_, HashMap<blake3::Hash, wgpu::BindGroupLayout>> {
        self.set_layouts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The compiled module for a stage, built from the parsed naga module on first use.
    ///
    /// # Errors
    /// Fails when the device rejects the module.
    pub(crate) fn shader_module(
        &mut self,
        stage: &StageProgram,
    ) -> Result<wgpu::ShaderModule, ProgramError> {
        if let Some(module) = self.modules.get(&stage.content_key()) {
            return Ok(module.clone());
        }
        let module = self.guarded(|| {
            self.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&stage.module().entry_point),
                    source: wgpu::ShaderSource::Naga(Cow::Owned(stage.naga_module().clone())),
                })
        })?;
        self.counters.module_built();
        self.modules.insert(stage.content_key(), module.clone());
        Ok(module)
    }

    /// Runs `work` and reports a GPU validation error it caused as a named error instead of
    /// letting wgpu's default handler panic.
    pub(crate) fn guarded<T>(&self, work: impl FnOnce() -> T) -> Result<T, ProgramError> {
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let value = work();
        let mut pending = pin!(scope.pop());
        let mut context = Context::from_waker(Waker::noop());
        // Native backends resolve the scope when it is popped; the poll covers a backend that
        // needs the device to make progress first.
        for _ in 0..PENDING_POLLS {
            if let Poll::Ready(error) = pending.as_mut().poll(&mut context) {
                return error.map_or(Ok(value), |error| Err(ProgramError::Gpu(error.to_string())));
            }
            self.poll_device()?;
        }
        Err(ProgramError::Gpu(
            "the error scope did not resolve".to_owned(),
        ))
    }

    pub(crate) fn poll_device(&self) -> Result<(), ProgramError> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map(|_| ())
            .map_err(|error| ProgramError::Gpu(error.to_string()))
    }
}

/// Times a pending error scope is polled, with the device polled between.
const PENDING_POLLS: usize = 4;
