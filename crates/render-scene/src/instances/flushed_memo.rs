//! Whether every batch has the buffers a gather reads, remembered per state of the scene.
//!
//! [`InstanceBatch::is_flushed`] reads whether a batch has its instance buffer, its previous
//! buffer and its skin buffer. A buffer is made only by a flush that grows it, which raises the
//! content counter, and a batch starts with none and exists only once a placement was added to
//! it, which raises the edit counter. The answer therefore changes only with those two counters
//! and with whether previous buffers are asked for. A view of a moving camera over a scene at
//! rest asks at the same state every frame, so it asks the scene once instead of every batch it
//! selects.
#[cfg(test)]
mod test_hooks;
use std::cell::Cell;

use super::Instances;

/// The counters and request an answer was found at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct At {
    edits: u64,
    content: u64,
    previous: bool,
}

#[derive(Debug, Default)]
pub(super) struct FlushedMemo {
    /// What the last scan found, and where.
    found: Cell<Option<(At, bool)>>,
    /// How many times the batches were scanned.
    #[cfg(test)]
    scans: Cell<usize>,
}

impl Instances {
    /// Whether every batch can be gathered from: it has an instance buffer, a previous buffer
    /// when `previous`, and a skin buffer when it has a skin stream.
    pub(crate) fn all_flushed(&self, previous: bool) -> bool {
        let at = At {
            edits: self.totals.edits.get(),
            content: self.totals.content.get(),
            previous,
        };
        let scan = || {
            self.batches
                .values()
                .all(|batch| batch.is_flushed(previous))
        };
        if let Some((_, flushed)) = self.flushed.found.get().filter(|(found, _)| *found == at) {
            debug_assert_eq!(flushed, scan());
            return flushed;
        }
        #[cfg(test)]
        self.flushed.scans.set(self.flushed.scans.get() + 1);
        let flushed = scan();
        self.flushed.found.set(Some((at, flushed)));
        flushed
    }
}
