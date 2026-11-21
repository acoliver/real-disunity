//! Sums of the batches' change counters, kept as the batches change.
//!
//! A frame asks for them many times: every view's selection and compaction key reads the edit
//! and content sums, and every readiness check reads all three. Adding up a few hundred batches
//! for each ask costs more than the rest of an unchanged frame's check, so each batch also
//! raises the scene's tally by the amount it raises its own counter, and the tally equals the
//! sum at every moment.
use std::{cell::Cell, rc::Rc};

/// One counter shared by every batch of a scene, which each batch raises with its own.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tally(Rc<Cell<u64>>);

impl Tally {
    pub fn add(&self, amount: u64) {
        self.0.set(self.0.get() + amount);
    }

    pub fn get(&self) -> u64 {
        self.0.get()
    }
}

/// The scene's tallies of its batches' placement edits, staged content and history changes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Totals {
    pub edits: Tally,
    pub content: Tally,
    pub history: Tally,
}
