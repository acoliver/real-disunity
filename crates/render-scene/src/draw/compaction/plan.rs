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
    /// Orders the ranges by (batch, LOD) and keeps slot order within each group, replacing what
    /// the plan held. `order` is scratch for the ordering and holds indices into `ranges`.
    /// Returns false when the records would not fit a draw's instance range.
    fn fill(&mut self, ranges: &[SelectedRange], order: &mut Vec<usize>) -> bool {
        self.groups.clear();
        self.copies.clear();
        self.total = 0;
        self.groups.reserve(ranges.len());
        self.copies.reserve(ranges.len());
        order.clear();
        order.extend(0..ranges.len());
        // Ties in the sort keys break by position, which a stable sort would keep.
        if ranges.is_sorted_by_key(|range| range.key) {
            // A selection lists its batches in key order, so only the levels within each
            // batch need ordering; the order of each run by (LOD, position) is the (batch, LOD)
            // order of a stable sort.
            for run in order.chunk_by_mut(|&a, &b| ranges[a].key == ranges[b].key) {
                if !run.is_sorted_by_key(|&index| ranges[index].lod) {
                    run.sort_unstable_by_key(|&index| (ranges[index].lod, index));
                }
            }
        } else {
            order.sort_unstable_by_key(|&index| (ranges[index].key, ranges[index].lod, index));
        }
        for &index in order.iter() {
            let range = &ranges[index];
            let start = self.total;
            let Some(end) = start.checked_add(range.slots.end - range.slots.start) else {
                return false;
            };
            match self
                .groups
                .last_mut()
                .filter(|group| group.key == range.key && group.lod == range.lod)
            {
                Some(group) => group.slots.end = end,
                None => self.groups.push(SelectedRange {
                    key: range.key,
                    lod: range.lod,
                    slots: start..end,
                }),
            }
            self.copies.push(Gather {
                key: range.key,
                source: range.slots.clone(),
                destination: start,
            });
            self.total = end;
        }
        true
    }

    /// A plan of `ranges` held by nothing but itself.
    #[cfg(test)]
    pub fn new(ranges: &[SelectedRange]) -> Option<Self> {
        let mut plan = Self::default();
        plan.fill(ranges, &mut Vec::new()).then_some(plan)
    }

    /// Replaces `runs` with the gather runs that read every selected range from a mirror holding
    /// batch `key` from record `base(key)`, which is asked for once per batch, in ascending key
    /// order. Ranges contiguous in both the mirror and the view buffer merge, and runs longer
    /// than [`RUN_RECORDS`] split. `merged` is scratch.
    fn runs_into(
        &self,
        mut base: impl FnMut(BatchKey) -> u32,
        merged: &mut Vec<Run>,
        runs: &mut Vec<Run>,
    ) {
        merged.clear();
        merged.reserve(self.copies.len());
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
        runs.clear();
        runs.reserve(merged.len());
        for run in merged.iter() {
            runs.extend(
                (0..run.count)
                    .step_by(RUN_RECORDS as usize)
                    .map(|offset| Run {
                        source: run.source + offset,
                        destination: run.destination + offset,
                        count: (run.count - offset).min(RUN_RECORDS),
                    }),
            );
        }
    }

    /// The runs of [`Plan::runs_into`] in a new list.
    #[cfg(test)]
    pub fn runs(&self, base: impl FnMut(BatchKey) -> u32) -> Vec<Run> {
        let mut runs = Vec::new();
        self.runs_into(base, &mut Vec::new(), &mut runs);
        runs
    }
}

/// A view's plan and the lists built from it, kept from one frame to the next. Each is cleared
/// and filled again by the next selection, and reserved from its range count, so a view whose
/// selection stops growing stops allocating for them.
#[derive(Debug, Default)]
pub(super) struct Workspace {
    order: Vec<usize>,
    plan: Plan,
    keys: Vec<BatchKey>,
    merged: Vec<Run>,
    runs: Vec<Run>,
}

impl Workspace {
    /// Plans `ranges`, replacing the earlier plan and key list. Returns false when the records
    /// would not fit a draw's instance range.
    pub fn arrange(&mut self, ranges: &[SelectedRange]) -> bool {
        if !self.plan.fill(ranges, &mut self.order) {
            return false;
        }
        self.keys.clear();
        self.keys.reserve(ranges.len());
        for copy in &self.plan.copies {
            if self.keys.last() != Some(&copy.key) {
                self.keys.push(copy.key);
            }
        }
        true
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// The batches of the plan, once each in key order.
    pub fn keys(&self) -> &[BatchKey] {
        &self.keys
    }

    /// The gather runs of the plan, with each batch's base asked for once in key order.
    pub fn runs(&mut self, base: impl FnMut(BatchKey) -> u32) -> &[Run] {
        self.plan.runs_into(base, &mut self.merged, &mut self.runs);
        &self.runs
    }

    /// Where each list lives and how much it can hold: the order, groups, copies, keys, merged
    /// runs and runs.
    #[cfg(test)]
    pub fn buffers(&self) -> [(usize, usize); 6] {
        fn of<T>(list: &Vec<T>) -> (usize, usize) {
            (list.as_ptr() as usize, list.capacity())
        }
        [
            of(&self.order),
            of(&self.plan.groups),
            of(&self.plan.copies),
            of(&self.keys),
            of(&self.merged),
            of(&self.runs),
        ]
    }
}
