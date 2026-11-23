//! Test-only accessors of this module's types.

use super::*;

impl ReadyMemo {
    /// How many times a check has scanned the batches.
    pub fn scans(&self) -> usize {
        self.scans.get()
    }
}
