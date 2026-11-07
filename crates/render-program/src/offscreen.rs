//! Offscreen operations the checked-operations contract accepted.
//!
//! A host builds the operations of a frame (the Unity adapter's `unity::offscreen` does), has the
//! contract check them and gets an [`AdmittedOffscreen`]. A renderer that encodes the frame holds
//! only that, so it needs no knowledge of how the operations were made.

use worldrunner_render_gpu::submission::{FrameLease, Publications};
use worldrunner_render_post::TimingState;
use worldrunner_render_post::contracts::{
    CompiledOperations, ContractError, RasterId, ResourceRegistry,
};

use crate::error::ProgramError;
use crate::resources::Pins;

/// Operations the contract accepted, held by a frame's retained arena. Only the code that
/// builds a frame's operations makes one.
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
    pub(crate) fn new(
        compiled: CompiledOperations,
        registry: ResourceRegistry,
        lease: FrameLease,
        pins: Pins,
    ) -> Self {
        Self {
            compiled,
            registry,
            lease,
            pins,
        }
    }

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
