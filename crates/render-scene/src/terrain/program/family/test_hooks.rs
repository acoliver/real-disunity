//! Test-only accessors of this module's types.

use super::*;

impl Family {
    /// What the objects' runtime has built so far.
    pub(crate) fn object_stats(&self) -> RuntimeStats {
        self.0
            .objects
            .borrow()
            .as_ref()
            .map_or_else(RuntimeStats::default, ProgramRuntime::stats)
    }
}
