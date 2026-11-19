//! Format-neutral handoff and admission of offscreen work.
//!
//! A producer builds a [`PendingOffscreen`] packet. The consumer admits it on its own recording
//! and gets an [`AdmittedOffscreen`] to encode in that frame.

use std::sync::Arc;

use worldrunner_render_gpu::submission::{FrameLease, Publications, SubmissionAuthority};
use worldrunner_render_post::contracts::{
    CompiledOperations, ContractError, RasterId, ResourceRegistry, ResourceUse, TypedOperation,
};
use worldrunner_render_post::{
    FrameError, FrameOperations, OperationsOutput, ResourceRef, TimingState,
};

use crate::budget::Reservation;
use crate::error::ProgramError;
use crate::resources::Pins;

/// A checked export and the precise texture it names in the frame plan.
pub(crate) struct Export {
    pub(crate) output: OperationsOutput,
    pub(crate) texture: wgpu::Texture,
}

/// The typed operations of one frame, awaiting admission on the frame that will run them.
pub struct PendingOffscreen {
    pub(crate) device: wgpu::Device,
    pub(crate) authority: SubmissionAuthority,
    pub(crate) lease: FrameLease,
    pub(crate) registry: ResourceRegistry,
    pub(crate) operations: Vec<TypedOperation>,
    pub(crate) reads: Vec<ResourceUse>,
    pub(crate) exports: Vec<Export>,
    pub(crate) shares: Vec<Arc<Reservation>>,
}

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error(transparent)]
    Program(#[from] ProgramError),
}

impl PendingOffscreen {
    /// Whether the pending work exports `texture` itself as `resource`.
    #[must_use]
    pub fn exports_texture(&self, resource: ResourceRef, texture: &wgpu::Texture) -> bool {
        self.exports
            .iter()
            .any(|export| export.output.resource == resource && export.texture == *texture)
    }

    /// Drops the named export when the frame plan does not host it. The work still runs and
    /// writes its textures. Returns whether the export was present.
    pub fn withdraw_export(&mut self, resource: ResourceRef) -> bool {
        let before = self.exports.len();
        self.exports
            .retain(|export| export.output.resource != resource);
        self.exports.len() != before
    }

    /// Checks the operations and later reads, then retains the arena and storage on `encoder`'s
    /// frame until it completes. A failed contract check retains nothing on the frame.
    ///
    /// # Errors
    /// Fails with the contract's error for invalid operations or reads, or when the arena has
    /// retired or `encoder` is not recording a frame of the producer's submission authority.
    pub fn admit(
        self,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<AdmittedOffscreen, AdmissionError> {
        let compiled = self.registry.compile_operations_with_reads(
            &self.device,
            &self.operations,
            &self.reads,
        )?;
        let admitted = AdmittedOffscreen {
            compiled,
            registry: self.registry,
            lease: self.lease,
            pins: Pins::new(&self.authority, self.shares),
            exports: self
                .exports
                .into_iter()
                .map(|export| export.output)
                .collect(),
        };
        admitted.retain(encoder)?;
        Ok(admitted)
    }
}

/// Operations the contract accepted, held by a frame's retained arena. Only the code that
/// builds a frame's operations makes one.
pub struct AdmittedOffscreen {
    compiled: CompiledOperations,
    registry: ResourceRegistry,
    lease: FrameLease,
    pins: Pins,
    exports: Vec<OperationsOutput>,
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

/// A frame plan positions the admitted operations among its passes and the executor encodes them
/// there with the frame's timer.
///
/// The outputs are the textures the host exported before admission. Nothing else the operations
/// write is reported, so a frame that exports nothing leaves its operations unreadable to the
/// scene's passes.
impl FrameOperations for AdmittedOffscreen {
    fn produces(&self) -> &[OperationsOutput] {
        &self.exports
    }

    fn encode_operations(
        &self,
        timing: Option<&TimingState>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        self.encode(timing, encoder).map_err(FrameError::from)
    }
}
