use std::ops::Range;

use crate::draw::visibility::SelectedRange;
use crate::instances::BatchKey;

/// One gather copy: selected slots of a batch to a position in the view buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Gather {
    pub key: BatchKey,
    pub source: Range<u32>,
    pub destination: u32,
}

/// The layout of a compacted view buffer.
#[derive(Debug, Default)]
pub(super) struct Plan {
    /// One range of the view buffer per (batch, LOD), ordered by batch key then LOD.
    pub groups: Vec<SelectedRange>,
    pub copies: Vec<Gather>,
    /// Instance records in the view buffer.
    pub total: u32,
}

impl Plan {
    /// Orders the ranges by (batch, LOD) and keeps slot order within each group.
    /// Returns `None` when the records would not fit a draw's instance range.
    pub fn new(ranges: &[SelectedRange]) -> Option<Self> {
        let mut order: Vec<&SelectedRange> = ranges.iter().collect();
        order.sort_by_key(|range| (range.key, range.lod));
        let mut plan = Self::default();
        for range in order {
            let start = plan.total;
            let end = start.checked_add(range.slots.end - range.slots.start)?;
            match plan
                .groups
                .last_mut()
                .filter(|group| group.key == range.key && group.lod == range.lod)
            {
                Some(group) => group.slots.end = end,
                None => plan.groups.push(SelectedRange {
                    key: range.key,
                    lod: range.lod,
                    slots: start..end,
                }),
            }
            plan.copies.push(Gather {
                key: range.key,
                source: range.slots.clone(),
                destination: start,
            });
            plan.total = end;
        }
        Some(plan)
    }
}
