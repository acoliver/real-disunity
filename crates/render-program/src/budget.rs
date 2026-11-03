//! Descriptor-byte admission of every allocation the runtime holds, against render-gpu's
//! `AllocationSet` rules and one `AllocationBudget`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, ConfigurationError};

use crate::error::ProgramError;

#[derive(Default)]
struct Held {
    next: u64,
    /// Each live set with the descriptor bytes it alone accounts for.
    sets: HashMap<u64, (AllocationSet, u64)>,
    bytes: u64,
}

/// The allocations that are live now: resources the runtime owns, released resources the GPU
/// may still be using, and readbacks that have not been resolved.
#[derive(Clone, Default)]
pub(crate) struct Ledger(Arc<Mutex<Held>>);

fn lock(held: &Mutex<Held>) -> MutexGuard<'_, Held> {
    held.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Ledger {
    /// Admits `set` on top of everything live, or refuses it and changes nothing.
    pub(crate) fn admit(
        &self,
        limits: &wgpu::Limits,
        budget: AllocationBudget,
        set: AllocationSet,
    ) -> Result<Reservation, ProgramError> {
        let mut held = lock(&self.0);
        let mut peak = AllocationSet::default();
        for (live, _) in held.sets.values() {
            peak.extend(live.clone());
        }
        peak.extend(set.clone());
        let total = peak
            .validate_budget::<Infallible>(limits, budget)
            .map_err(refusal)?;
        held.next += 1;
        let id = held.next;
        let own = total.saturating_sub(held.bytes);
        held.sets.insert(id, (set, own));
        held.bytes = total;
        Ok(Reservation {
            ledger: self.clone(),
            id,
        })
    }

    /// Whether both handles are the one ledger.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn live_bytes(&self) -> u64 {
        lock(&self.0).bytes
    }
}

/// Holds an admitted allocation; dropping it returns the bytes to the budget.
pub(crate) struct Reservation {
    ledger: Ledger,
    id: u64,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut held = lock(&self.ledger.0);
        if let Some((_, own)) = held.sets.remove(&self.id) {
            held.bytes = held.bytes.saturating_sub(own);
        }
    }
}

fn refusal(error: ConfigurationError<Infallible>) -> ProgramError {
    match error {
        ConfigurationError::Budget { required, budget } => {
            ProgramError::OverBudget { required, budget }
        }
        ConfigurationError::Invalid(reason) => ProgramError::InvalidAllocation(reason),
        ConfigurationError::TextureArrayLayers { .. } => {
            ProgramError::InvalidAllocation("texture array layers")
        }
        ConfigurationError::ForeignDevice => ProgramError::InvalidAllocation("foreign device"),
        ConfigurationError::Allocation(never) => match never {},
        ConfigurationError::Submission(error) => error.into(),
    }
}
