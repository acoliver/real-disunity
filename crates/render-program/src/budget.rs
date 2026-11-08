//! Descriptor-byte admission of every allocation the runtime holds, against render-gpu's
//! `AllocationSet` rules and one `AllocationBudget`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use worldrunner_render_gpu::configuration::{
    AllocationBudget, AllocationReservation, AllocationSet, ConfigurationError,
};
use worldrunner_render_gpu::submission::LeaseError;

use crate::error::ProgramError;

/// The renderer's reservation of the storage the runtime may use. Every allocation made under
/// it holds a share, so the renderer's bytes stay counted until the last of them is gone.
pub(crate) type Backing = Arc<AllocationReservation>;

/// Whether the runtime allocates under an arena's reservation.
#[derive(Default)]
enum Arena {
    /// No arena was opened: the runtime is bounded by its own budget alone.
    #[default]
    None,
    Open {
        id: u64,
        backing: Backing,
    },
    /// The arena retired or was dropped. What it admitted keeps its share, and nothing new is
    /// admitted, since it would hold storage nobody reserved.
    Closed,
}

/// What an allocation was made for, for the census of what the runtime holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Note {
    /// The label the resource was created under.
    pub(crate) label: String,
    /// Its shape when that tells the allocations apart, such as a texture's size and format.
    pub(crate) detail: String,
}

impl Note {
    pub(crate) fn new(label: &str, detail: impl Into<String>) -> Self {
        Self {
            label: label.to_owned(),
            detail: detail.into(),
        }
    }
}

/// One live allocation set: what it is, the descriptor bytes it alone accounts for and its note.
struct Live {
    set: AllocationSet,
    own: u64,
    note: Note,
}

#[derive(Default)]
struct Held {
    next: u64,
    arena: Arena,
    arenas: u64,
    sets: HashMap<u64, Live>,
    bytes: u64,
    /// The most bytes held at once since the ledger was made.
    peak: u64,
}

/// The allocations of one kind that are live: how many, their bytes together and the largest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holding {
    pub label: String,
    pub detail: String,
    pub count: usize,
    pub bytes: u64,
    pub largest: u64,
}

/// The allocations that are live now: resources the runtime owns, released resources the GPU
/// may still be using, and the staging of readbacks and writes that their frames still hold.
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
        note: Note,
    ) -> Result<Reservation, ProgramError> {
        let mut held = lock(&self.0);
        let backing = match &held.arena {
            Arena::None => None,
            Arena::Open { backing, .. } => Some(backing.clone()),
            Arena::Closed => return Err(LeaseError::Expired.into()),
        };
        let mut peak = AllocationSet::default();
        for live in held.sets.values() {
            peak.extend(live.set.clone());
        }
        peak.extend(set.clone());
        let total = peak
            .validate_budget::<Infallible>(limits, budget)
            .map_err(refusal)?;
        held.next += 1;
        let id = held.next;
        let own = total.saturating_sub(held.bytes);
        held.sets.insert(id, Live { set, own, note });
        held.bytes = total;
        held.peak = held.peak.max(total);
        Ok(Reservation {
            ledger: self.clone(),
            id,
            _backing: backing,
        })
    }

    /// Makes `backing` the reservation later allocations are made under and returns the id
    /// that closes it again.
    ///
    /// # Errors
    /// Fails when allocations made before any arena are still held, since the reservation could
    /// not cover them and would be given back while they are live.
    pub(crate) fn open_arena(&self, backing: Backing) -> Result<u64, ProgramError> {
        let mut held = lock(&self.0);
        if matches!(held.arena, Arena::None) && !held.sets.is_empty() {
            return Err(ProgramError::ArenaAfterAllocations { live: held.bytes });
        }
        held.arenas += 1;
        let id = held.arenas;
        held.arena = Arena::Open { id, backing };
        Ok(id)
    }

    /// Stops admitting allocations under arena `id`, unless a newer arena has replaced it.
    pub(crate) fn close_arena(&self, id: u64) {
        let mut held = lock(&self.0);
        if matches!(held.arena, Arena::Open { id: open, .. } if open == id) {
            held.arena = Arena::Closed;
        }
    }

    /// Whether both handles are the one ledger.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn live_bytes(&self) -> u64 {
        lock(&self.0).bytes
    }

    pub(crate) fn peak_bytes(&self) -> u64 {
        lock(&self.0).peak
    }

    /// The live allocations grouped by label and detail, the greatest bytes first.
    pub(crate) fn holdings(&self) -> Vec<Holding> {
        let held = lock(&self.0);
        let mut groups: HashMap<(&str, &str), Holding> = HashMap::new();
        for live in held.sets.values() {
            let group = groups
                .entry((&live.note.label, &live.note.detail))
                .or_insert_with(|| Holding {
                    label: live.note.label.clone(),
                    detail: live.note.detail.clone(),
                    count: 0,
                    bytes: 0,
                    largest: 0,
                });
            group.count += 1;
            group.bytes += live.own;
            group.largest = group.largest.max(live.own);
        }
        let mut holdings: Vec<Holding> = groups.into_values().collect();
        holdings
            .sort_by(|a, b| (b.bytes, &a.label, &a.detail).cmp(&(a.bytes, &b.label, &b.detail)));
        holdings
    }
}

/// Holds an admitted allocation; dropping it returns the bytes to the budget.
pub(crate) struct Reservation {
    ledger: Ledger,
    id: u64,
    _backing: Option<Backing>,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut held = lock(&self.ledger.0);
        if let Some(live) = held.sets.remove(&self.id) {
            held.bytes = held.bytes.saturating_sub(live.own);
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
