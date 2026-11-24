//! Test-only accessors of this module's types.

use super::*;

impl InstanceBatch {
    pub fn visible_ranges(&self) -> Vec<Range<u32>> {
        ranges(
            self.placements
                .iter()
                .enumerate()
                .filter(|(_, placement)| placement.is_some_and(|(_, visible)| visible))
                .map(|(position, _)| position),
        )
    }

    pub fn pending_ranges(&self) -> Vec<Range<u32>> {
        self.pending_ranges_for(Publications::current().as_ref())
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }
}
