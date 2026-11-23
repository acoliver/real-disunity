//! Test-only accessors of this module's types.

use super::*;

impl Instances {
    /// How many flushes looped over the batches rather than skipping a scene at rest.
    pub(crate) fn flush_loops(&self) -> usize {
        self.settled.loops
    }
}
