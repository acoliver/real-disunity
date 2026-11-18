//! The batch ranges a mirror state was last found to hold at their batches' current content.
//!
//! Checking that a state holds a batch takes the batch's buffer size, its layout range and its
//! held content generation, three ordered-map lookups per batch and per view. Those facts change
//! only through the batch's content generation, which rises with every staged upload and every
//! buffer allocation, and through the state itself, which is replaced instead of edited once
//! shared. A state therefore remembers the keys it passed with the sum of the generations they
//! passed at, and a view that selects only remembered keys finds the state up to date without
//! checking any of them again.
use std::cell::RefCell;

use super::Halves;
use crate::instances::BatchKey;

/// What a state was found to hold, and for which sum of the batches' content generations.
#[derive(Debug)]
struct Checked {
    content: u64,
    halves: Halves,
    /// Ascending, without repeats.
    keys: Vec<BatchKey>,
}

/// The ranges of one state that held the content they were checked against.
///
/// A copy of a state is made to be edited, so a clone remembers nothing.
#[derive(Debug, Default)]
pub(super) struct Verified(RefCell<Option<Checked>>);

impl Clone for Verified {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Verified {
    /// Whether every key in `keys`, which the caller lists in ascending order, held its content
    /// at `content` with `halves`. A list in another order is not covered, only rechecked.
    pub fn covers(&self, content: u64, halves: Halves, keys: &[BatchKey]) -> bool {
        let checked = self.0.borrow();
        let Some(checked) = checked
            .as_ref()
            .filter(|checked| checked.content == content && checked.halves == halves)
        else {
            return false;
        };
        let mut remembered = checked.keys.iter();
        keys.iter().all(|key| remembered.any(|held| held == key))
    }

    /// Remembers that every key in `keys` held its content at `content` with `halves`, in
    /// addition to the keys already remembered for the same figures.
    pub fn remember(&self, content: u64, halves: Halves, keys: &[BatchKey]) {
        let mut slot = self.0.borrow_mut();
        let mut all: Vec<BatchKey> = match slot.take() {
            Some(checked) if checked.content == content && checked.halves == halves => checked.keys,
            _ => Vec::new(),
        };
        all.extend_from_slice(keys);
        all.sort_unstable();
        all.dedup();
        *slot = Some(Checked {
            content,
            halves,
            keys: all,
        });
    }
}
