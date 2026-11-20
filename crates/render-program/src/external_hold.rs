//! Bytes of a resource the runtime's owner keeps itself, counted against the runtime's budget.

use crate::budget::{Note, Reservation};
use crate::error::ProgramError;
use crate::runtime::ProgramRuntime;
use worldrunner_render_gpu::configuration::AllocationSet;

/// `bytes` of a resource its owner holds outside the runtime, counted in
/// [`live_bytes`](ProgramRuntime::live_bytes) and against the runtime's budget until it drops.
pub struct ExternalHold {
    _reservation: Reservation,
}

impl std::fmt::Debug for ExternalHold {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExternalHold")
            .finish_non_exhaustive()
    }
}

impl ProgramRuntime {
    /// Counts `bytes` of a resource the owner creates and keeps itself, so one budget bounds what
    /// the owner holds and what the runtime allocates. The owner calls it before it creates the
    /// resource and keeps the hold as long as the resource lives.
    ///
    /// # Errors
    /// Fails, with nothing counted, when the bytes would pass the budget or the device's limits.
    pub fn hold_external(&self, label: &str, bytes: u64) -> Result<ExternalHold, ProgramError> {
        let mut allocations = AllocationSet::default();
        allocations.buffer("render-program external resource", bytes, 1);
        let reservation = self.admit(allocations, Note::new(label, "external"))?;
        Ok(ExternalHold {
            _reservation: reservation,
        })
    }
}
