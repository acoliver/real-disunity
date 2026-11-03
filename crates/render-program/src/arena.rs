//! Admitting prepared draws into a frame: the budget arena that bounds what the runtime holds,
//! and the owner the frame retains until the GPU has finished with them.
//!
//! The renderer reserves the arena once, through its allocation configuration, so the bytes are
//! counted in its total. The runtime then admits its own resources under a cap that is at most
//! the arena, and a draw can only be admitted against an arena that the runtime opened and the
//! frame's own authority can retain.

use worldrunner_render_gpu::configuration::{AllocationReservation, AllocationSet};
use worldrunner_render_gpu::submission::{FrameLease, FrameOwner, Publications};

use crate::budget::Ledger;
use crate::error::ProgramError;
use crate::packet::{DrawPacket, PreparedProgramDraw, TargetSignature};
use crate::runtime::ProgramRuntime;

/// What the owner keeps alive: the renderer's reservation of the arena.
struct Hold {
    _reservation: AllocationReservation,
}

/// The renderer's reservation of the storage a runtime may use, owned by an explicit
/// [`FrameOwner`] so that frames which use the runtime's draws retain it until they complete.
pub struct ProgramArena {
    owner: FrameOwner<Hold>,
    ledger: Ledger,
}

impl std::fmt::Debug for ProgramArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramArena").finish_non_exhaustive()
    }
}

impl ProgramArena {
    /// Releases the reservation. A frame that holds the arena, whether it is still being
    /// recorded or has been submitted and not completed, keeps it from retiring.
    ///
    /// # Errors
    /// Fails with `Busy` while a frame holds the arena and with `Expired` once it has retired.
    pub fn retire(&mut self) -> Result<(), ProgramError> {
        self.owner.retire()?;
        Ok(())
    }
}

/// Draws that were prepared for one pass signature and are held by a frame's retained arena.
/// Only [`ProgramRuntime::admit_draws`] makes one.
#[derive(Debug)]
pub struct AdmittedPrograms {
    signature: TargetSignature,
    lease: FrameLease,
    draws: Vec<PreparedProgramDraw>,
}

impl AdmittedPrograms {
    #[must_use]
    pub fn signature(&self) -> TargetSignature {
        self.signature
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.draws.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }

    /// Makes the frame `encoder` records retain the arena until the GPU has completed it.
    /// Admission does this once; a consumer that records into a later frame does it again.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when the frame belongs to another submission authority
    /// than the arena's, with `Expired` when the arena has retired or been dropped, and when
    /// `encoder` is not a frame being recorded.
    pub fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        Publications::retain_lease(encoder, &self.lease)?;
        Ok(())
    }

    /// Checks that a pass with `pass` attachments is one the draws were prepared for.
    ///
    /// # Errors
    /// Fails with `TargetMismatch` naming both signatures.
    pub fn check_target(&self, pass: TargetSignature) -> Result<(), ProgramError> {
        if pass == self.signature {
            return Ok(());
        }
        Err(ProgramError::TargetMismatch {
            prepared: self.signature,
            pass,
        })
    }

    /// Records every admitted draw, in admission order, into a pass whose attachments match
    /// [`signature`](Self::signature).
    pub fn record(&self, pass: &mut wgpu::RenderPass<'_>) {
        for draw in &self.draws {
            draw.record(pass);
        }
    }
}

impl ProgramRuntime {
    /// The storage the renderer reserves for an arena of `bytes`, to pass to its reservation.
    #[must_use]
    pub fn arena_allocations(bytes: u64) -> AllocationSet {
        let mut set = AllocationSet::default();
        set.buffer("player program arena", bytes, 1);
        set
    }

    /// Opens an arena over the renderer's `reservation`, owned through this runtime's
    /// submission authority.
    ///
    /// # Errors
    /// Fails when the reservation is smaller than the runtime's budget, since the runtime could
    /// then hold storage nobody reserved.
    pub fn open_arena(
        &self,
        reservation: AllocationReservation,
    ) -> Result<ProgramArena, ProgramError> {
        let arena = reservation
            .bytes(&self.device.limits())
            .map_err(ProgramError::InvalidAllocation)?;
        if self.budget.0 > arena {
            return Err(ProgramError::ArenaTooSmall {
                budget: self.budget.0,
                arena,
            });
        }
        Ok(ProgramArena {
            owner: FrameOwner::new(
                Hold {
                    _reservation: reservation,
                },
                &self.authority,
            ),
            ledger: self.ledger.clone(),
        })
    }

    /// Prepares `packets` for passes with `signature` and admits them into the frame `encoder`
    /// records, which then retains `arena` until the GPU has completed it. Nothing is admitted
    /// when any step fails.
    ///
    /// # Errors
    /// Fails when `encoder` is not the authority's current frame, the arena was opened by
    /// another runtime, a packet cannot be prepared, the frame belongs to another authority
    /// than the arena's (`ForeignAuthority`) or the arena has retired (`Expired`).
    pub fn admit_draws(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        arena: &ProgramArena,
        signature: TargetSignature,
        packets: &[DrawPacket<'_>],
    ) -> Result<AdmittedPrograms, ProgramError> {
        self.recording(encoder)?;
        if !arena.ledger.same(&self.ledger) {
            return Err(ProgramError::ForeignArena);
        }
        let draws = packets
            .iter()
            .map(|packet| self.prepare_draw(packet, signature))
            .collect::<Result<Vec<_>, _>>()?;
        let admitted = AdmittedPrograms {
            signature,
            lease: arena.owner.lease(),
            draws,
        };
        admitted.retain(encoder)?;
        Ok(admitted)
    }
}
