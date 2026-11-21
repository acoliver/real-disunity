//! The batch ranges a mirror state was last found to hold at their batches' current content.
//!
//! Checking that a state holds a batch takes the batch's buffer size, its layout range and its
//! held content generation, three ordered-map lookups per batch and per view. Those facts change
//! only through the batch's content generation, which rises with every staged upload and every
//! buffer allocation, and through the state itself, which is replaced instead of edited once
//! shared. A state therefore remembers the keys it passed with the sum of the generations they
//! passed at. A view that selects only remembered keys finds the state up to date without
//! checking any of them again, and a view that selects some other batch as well checks only that
//! batch.
use std::cell::RefCell;

use super::Halves;
use crate::instances::BatchKey;

/// What a state was found to hold, and for which sum of the batches' content generations.
#[derive(Clone, Debug)]
struct Checked {
    content: u64,
    halves: Halves,
    /// Ascending, without repeats.
    keys: Vec<BatchKey>,
}

/// The ranges of one state that held the content they were checked against.
///
/// A copy of a state keeps what it remembers: the copy is made to add ranges or to give a batch
/// a new range, and a batch whose range moved is forgotten with [`Verified::forget`].
#[derive(Clone, Debug, Default)]
pub(super) struct Verified(RefCell<Option<Checked>>);

impl Verified {
    /// The keys of `keys`, which the caller lists in ascending order, that did not hold their
    /// content at `content` with `halves`: all of them unless the figures match. A list in
    /// another order reports keys that are remembered, which is only a recheck.
    pub fn missing(&self, content: u64, halves: Halves, keys: &[BatchKey]) -> Vec<BatchKey> {
        let checked = self.0.borrow();
        let Some(checked) = checked
            .as_ref()
            .filter(|checked| checked.content == content && checked.halves == halves)
        else {
            return keys.to_vec();
        };
        let mut remembered = checked.keys.iter().peekable();
        keys.iter()
            .copied()
            .filter(|key| {
                while remembered.next_if(|held| **held < *key).is_some() {}
                remembered.peek().is_none_or(|held| **held != *key)
            })
            .collect()
    }

    /// Remembers that every key in `keys` held its content at `content` with `halves`, in
    /// addition to the keys already remembered for the same figures.
    pub fn remember(&self, content: u64, halves: Halves, keys: &[BatchKey]) {
        let mut slot = self.0.borrow_mut();
        let mut all: Vec<BatchKey> = match slot.take() {
            Some(checked) if checked.content == content && checked.halves == halves => checked.keys,
            _ => Vec::new(),
        };
        for key in keys {
            if let Err(index) = all.binary_search(key) {
                all.insert(index, *key);
            }
        }
        *slot = Some(Checked {
            content,
            halves,
            keys: all,
        });
    }

    /// Stops remembering `key`, whose range no longer holds what it held.
    pub fn forget(&self, key: BatchKey) {
        let mut slot = self.0.borrow_mut();
        if let Some(checked) = slot.as_mut()
            && let Ok(index) = checked.keys.binary_search(&key)
        {
            checked.keys.remove(index);
        }
    }
}

#[cfg(test)]
#[path = "verified_tests.rs"]
mod tests;
