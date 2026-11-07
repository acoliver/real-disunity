//! Admitting prepared draws into a frame: the budget arena that bounds what the runtime holds,
//! and the owner the frame retains until the GPU has finished with them.
//!
//! The renderer reserves the arena once, through its allocation configuration, so the bytes are
//! counted in its total. The runtime then admits its own resources under a cap that is at most
//! the arena, and a draw can only be admitted against an arena that the runtime opened and the
//! frame's own authority can retain.

use std::sync::Arc;

use worldrunner_render_gpu::configuration::{AllocationReservation, AllocationSet};
use worldrunner_render_gpu::submission::{FrameLease, FrameOwner, Publications};

use crate::budget::{Backing, Ledger};
use crate::error::ProgramError;
use crate::packet::{DrawPacket, MaterialParts, RecordableDraw, TargetSignature};
use crate::resources::Pins;
use crate::runtime::ProgramRuntime;

/// What the owner keeps alive: a share of the renderer's reservation of the arena. The runtime's
/// allocations hold the other shares, so the reservation outlasts the owner for as long as any
/// of them is live.
struct Hold {
    _backing: Backing,
}

/// The renderer's reservation of the storage a runtime may use, owned by an explicit
/// [`FrameOwner`] so that frames which use the runtime's draws retain it until they complete.
pub struct ProgramArena {
    owner: FrameOwner<Hold>,
    ledger: Ledger,
    id: u64,
}

impl std::fmt::Debug for ProgramArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramArena").finish_non_exhaustive()
    }
}

impl ProgramArena {
    /// The lease frames hold to keep the arena alive. Resources registered with an executor in
    /// this arena's name are imported with it.
    pub(crate) fn lease(&self) -> FrameLease {
        self.owner.lease()
    }

    /// Whether `runtime` opened this arena.
    pub(crate) fn belongs_to(&self, runtime: &ProgramRuntime) -> bool {
        self.ledger.same(&runtime.ledger)
    }

    /// Stops the runtime allocating under the reservation and gives up the arena's share of
    /// it. The renderer's bytes stay counted until every allocation the runtime still holds,
    /// including released ones the GPU may still use, has gone. A frame that holds the arena,
    /// whether it is still being recorded or has been submitted and not completed, keeps it
    /// from retiring.
    ///
    /// # Errors
    /// Fails with `Busy` while a frame holds the arena and with `Expired` once it has retired.
    pub fn retire(&mut self) -> Result<(), ProgramError> {
        self.owner.retire()?;
        self.ledger.close_arena(self.id);
        Ok(())
    }
}

/// Dropping the arena stops the runtime allocating under it, as retiring does.
impl Drop for ProgramArena {
    fn drop(&mut self) {
        self.ledger.close_arena(self.id);
    }
}

/// Draws that were prepared for one pass signature and are held by a frame's retained arena.
/// Only [`ProgramRuntime::admit_draws`] makes one.
#[derive(Debug)]
pub struct AdmittedPrograms {
    signature: TargetSignature,
    lease: FrameLease,
    /// The reservations of everything the draws use, which each frame that records them holds.
    pins: Pins,
    draws: Vec<RecordableDraw>,
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

    /// Makes the frame `encoder` retain the arena and the reservations of the resources the
    /// draws use until the frame is cancelled, dropped or completed on the GPU. Admission does
    /// this once; a consumer that records into a later frame does it again.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when the frame belongs to another submission authority
    /// than the arena's, with `Expired` when the arena has retired or been dropped, even if
    /// the frame retained it earlier, and when `encoder` is not a frame being recorded.
    pub fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.lease.check()?;
        Publications::retain_lease(encoder, &self.lease)?;
        self.pins.retain(encoder)
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
        set.buffer("program arena", bytes, 1);
        set
    }

    /// Opens an arena over the renderer's `reservation`, owned through this runtime's
    /// submission authority. Every allocation made from now on holds a share of the reservation.
    /// Open the first arena before the runtime allocates anything: storage allocated earlier
    /// has no share of it. Allocations made under an earlier arena keep that arena's share.
    ///
    /// # Errors
    /// Fails when the reservation is smaller than the runtime's budget, since the runtime could
    /// then hold storage nobody reserved, and with `ArenaAfterAllocations` when the runtime
    /// still holds allocations made before any arena, whether live or released but not yet
    /// collected. The reservation is dropped, so the renderer's bytes are counted no longer.
    pub fn open_arena(
        &self,
        reservation: AllocationReservation,
    ) -> Result<ProgramArena, ProgramError> {
        self.collect();
        let arena = reservation
            .bytes(&self.device.limits())
            .map_err(ProgramError::InvalidAllocation)?;
        if self.budget.0 > arena {
            return Err(ProgramError::ArenaTooSmall {
                budget: self.budget.0,
                arena,
            });
        }
        let backing = Arc::new(reservation);
        let id = self.ledger.open_arena(backing.clone())?;
        Ok(ProgramArena {
            owner: FrameOwner::new(Hold { _backing: backing }, &self.authority),
            ledger: self.ledger.clone(),
            id,
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
        let items: Vec<_> = packets.iter().map(|packet| (*packet, None)).collect();
        self.admit_parts(encoder, arena, signature, &items)
    }

    /// Admits packets with the parts a material draw adds, under the rules of
    /// [`admit_draws`](Self::admit_draws).
    pub(crate) fn admit_parts(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        arena: &ProgramArena,
        signature: TargetSignature,
        items: &[(DrawPacket<'_>, Option<MaterialParts<'_>>)],
    ) -> Result<AdmittedPrograms, ProgramError> {
        self.recording(encoder)?;
        if !arena.ledger.same(&self.ledger) {
            return Err(ProgramError::ForeignArena);
        }
        let mut draws = Vec::with_capacity(items.len());
        let mut shares = Vec::new();
        for (packet, parts) in items {
            let (draw, held) = self.prepare_recordable(packet, signature, parts.as_ref())?;
            draws.push(draw);
            shares.extend(held);
        }
        let admitted = AdmittedPrograms {
            signature,
            lease: arena.owner.lease(),
            pins: Pins::new(&self.authority, shares),
            draws,
        };
        admitted.retain(encoder)?;
        Ok(admitted)
    }
}
