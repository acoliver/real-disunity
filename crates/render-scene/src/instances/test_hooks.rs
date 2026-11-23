//! Test-only accessors of this module's types.

use super::*;

impl Instances {
    /// Makes the next view check every batch's cached bounds, as every view once did.
    pub(crate) fn recheck_bounds(&self) {
        self.bounds_current.set(None);
    }

    /// How many times `ready_for` has scanned the batches rather than reusing a passing scan.
    pub fn ready_scans(&self) -> usize {
        self.ready.scans()
    }

    /// The sum of every batch's history version, which rises with each change `ready_for` reads
    /// of the history.
    pub fn history_versions(&self) -> u64 {
        self.sum(&self.totals.history, InstanceBatch::history_version)
    }

    /// Makes the batch at `key` with no placement in it, as a batch left empty by removals is.
    pub(crate) fn add_empty_batch(&mut self, key: BatchKey) {
        self.batch_for(key);
    }
}
