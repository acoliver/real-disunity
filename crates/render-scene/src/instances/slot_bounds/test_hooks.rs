//! Test-only accessors of this module's types.

use super::*;

impl SlotBounds {
    pub fn is_cached(&self, position: usize) -> bool {
        self.valid.get(position).copied().unwrap_or(false)
    }
}
