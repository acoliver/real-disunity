use std::ops::Range;

use crate::draw::visibility::SelectedRange;
use crate::instances::BatchKey;

/// One selected range: slots of a batch gathered to a position in the view buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Gather {
    pub key: BatchKey,
    pub source: Range<u32>,
    pub destination: u32,
}

/// Records one gather workgroup copies at most; longer runs are split so no workgroup
/// serializes a whole large batch.
pub(super) const RUN_RECORDS: u32 = 64;

/// One run of the gather dispatch: `count` records from mirror record `source` to view
/// record `destination`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Run {
    pub source: u32,
    pub destination: u32,
    pub count: u32,
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

    /// The gather runs that read every selected range from a mirror holding batch `key` from
    /// record `base(key)`. Ranges contiguous in both the mirror and the view buffer merge, and
    /// runs longer than [`RUN_RECORDS`] split.
    pub fn runs(&self, base: impl Fn(BatchKey) -> u32) -> Vec<Run> {
        let mut merged: Vec<Run> = Vec::with_capacity(self.copies.len());
        // The copies of one batch are adjacent, so its base is looked up once.
        let mut known: Option<(BatchKey, u32)> = None;
        for copy in &self.copies {
            let origin = match known {
                Some((key, origin)) if key == copy.key => origin,
                _ => {
                    let origin = base(copy.key);
                    known = Some((copy.key, origin));
                    origin
                }
            };
            let source = origin
                .checked_add(copy.source.start)
                .expect("mirror records fit u32");
            let count = copy.source.end - copy.source.start;
            match merged.last_mut().filter(|run| {
                run.source + run.count == source && run.destination + run.count == copy.destination
            }) {
                Some(run) => run.count += count,
                None => merged.push(Run {
                    source,
                    destination: copy.destination,
                    count,
                }),
            }
        }
        merged
            .into_iter()
            .flat_map(|run| {
                (0..run.count)
                    .step_by(RUN_RECORDS as usize)
                    .map(move |offset| Run {
                        source: run.source + offset,
                        destination: run.destination + offset,
                        count: (run.count - offset).min(RUN_RECORDS),
                    })
            })
            .collect()
    }
}
