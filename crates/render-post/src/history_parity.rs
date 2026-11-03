//! Which of the two temporal histories the next frame writes.
//!
//! The temporal upscale pass reads the history the previous frame wrote and writes the other one
//! next to its output, so no copy back is needed. The host decides the parity because only the
//! host knows which frames were really submitted: a frame that was encoded and then cancelled
//! must not change which history the next frame reads. The default is the even parity, which
//! reads the second history and writes the first.
use std::{cell::Cell, rc::Rc};

/// A shared flag: `false` is even (write the first history), `true` is odd. Clones share it.
#[derive(Clone, Debug, Default)]
pub struct HistoryParity(Rc<Cell<bool>>);

impl HistoryParity {
    pub fn select(&self, odd: bool) {
        self.0.set(odd);
    }

    pub fn odd(&self) -> bool {
        self.0.get()
    }
}

/// A test host: counts the frames it has submitted since a frame without history, as the
/// compositor counts submitted frames since a reset, and selects the parity of the next frame.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct SubmittedFrames(Cell<u64>);

#[cfg(test)]
impl SubmittedFrames {
    /// Selects the parity of a frame about to be encoded. A frame without history starts the count
    /// again, like a reset.
    pub(crate) fn select(&self, factory: &crate::ToneMapOutput, history_valid: bool) {
        if !history_valid {
            self.0.set(0);
        }
        factory.select_history_parity(self.0.get() % 2 == 1);
    }

    /// Records that the encoded frame was submitted.
    pub(crate) fn submitted(&self) {
        self.0.set(self.0.get() + 1);
    }

    /// The index of the history the last submitted frame wrote.
    pub(crate) fn written(&self) -> usize {
        usize::try_from(self.0.get().saturating_sub(1) % 2).expect("0 or 1")
    }
}
