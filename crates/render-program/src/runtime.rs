//! The runtime: the caller's device and submission authority with the resources, modules and
//! pipelines built on them.

use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use worldrunner_render_gpu::submission::{Publications, SubmissionAuthority};

use crate::error::ProgramError;
use crate::module::StageProgram;
use crate::resources::Store;

pub use crate::dispatch::DispatchRequest;
pub use crate::draw::{DrawRequest, Geometry, VertexInput};
pub use crate::program::{ComputeProgram, GraphicsProgram};
pub use crate::readback::{PendingBufferRead, PendingTargetRead, TargetImage};
pub use crate::resources::{BoundResource, BufferHandle, Resource, TargetHandle, TextureHandle};

/// Shares the caller's wgpu device and `SubmissionAuthority`, owns the resources it creates by
/// handle, and caches shader modules and pipelines keyed by their content.
///
/// The runtime records into frames and never submits. A caller opens a frame with
/// `SubmissionAuthority::begin`, passes it to `draw`, `dispatch` and the readbacks, and
/// submits it. The runtime reaches the queue only to upload initial texture contents.
pub struct ProgramRuntime {
    pub(crate) device: wgpu::Device,
    pub(crate) authority: SubmissionAuthority,
    pub(crate) store: Store,
    /// The one sampler every sampler binding gets: linear filtering, clamped to the edge.
    pub(crate) sampler: wgpu::Sampler,
    modules: HashMap<blake3::Hash, wgpu::ShaderModule>,
    pub(crate) render_pipelines: HashMap<blake3::Hash, wgpu::RenderPipeline>,
    pub(crate) compute_pipelines: HashMap<blake3::Hash, wgpu::ComputePipeline>,
}

impl ProgramRuntime {
    #[must_use]
    pub fn new(device: wgpu::Device, authority: SubmissionAuthority) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("render-program fixed sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            device,
            authority,
            store: Store::default(),
            sampler,
            modules: HashMap::new(),
            render_pipelines: HashMap::new(),
            compute_pipelines: HashMap::new(),
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
