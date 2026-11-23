//! Test-only accessors of this module's types.

use super::*;

impl Instances {
    /// How many times `all_flushed` scanned the batches rather than reusing an answer.
    pub(crate) fn flushed_scans(&self) -> usize {
        self.flushed.scans.get()
    }
}
