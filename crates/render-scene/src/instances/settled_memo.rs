//! The flush of every batch before a frame's views, which skips a scene at rest.
//!
//! [`InstanceBatch::is_settled`] reads a batch's dirty slots, buffer flag, pending history and
//! buffers. Those change only through the counters in [`Stamp`] (see `ready_memo`), and through
//! `track_previous`, which decides whether the previous-transform buffer is needed. A flush that
//! finds every batch settled has written, copied and published nothing, so it remembers the
//! stamp and `track_previous` it saw, and the next flush at the same pair has nothing to find.
//! A flush that did anything remembers nothing; the scene is checked again by the next one.
#[cfg(test)]
mod test_hooks;
use worldrunner_render_post::PrepareContext;

use super::ready_memo::Stamp;
use super::{InstanceWrite, Instances, history};

#[derive(Debug, Default)]
pub(super) struct SettledMemo {
    /// The counters and `track_previous` at which a flush found every batch settled.
    at: Option<(Stamp, bool)>,
    /// How many flushes looped over the batches.
    #[cfg(test)]
    loops: usize,
}

impl SettledMemo {
    fn holds(&self, stamp: Stamp, track_previous: bool) -> bool {
        self.at == Some((stamp, track_previous))
    }
}

impl Instances {
    /// Stages the uploads of every batch that has work and returns the slot ranges written. The
    /// history candidates of this flush are appended to `candidates`.
    pub(crate) fn flush_all(
        &mut self,
        context: &mut PrepareContext<'_>,
        candidates: &mut Vec<history::Candidate>,
    ) -> Vec<InstanceWrite> {
        let track = self.track_previous;
        let stamp = Stamp::of_totals(&self.totals);
        if self.settled.holds(stamp, track) {
            debug_assert!(self.batches.values().all(|batch| batch.is_settled(track)));
            return Vec::new();
        }
        #[cfg(test)]
        {
            self.settled.loops += 1;
        }
        let mut writes = Vec::new();
        let mut settled = true;
        for (&(_, mesh, material), batch) in &mut self.batches {
            if batch.is_settled(track) {
                continue;
            }
            settled = false;
            let (ranges, candidate) =
                super::batch::flush_prepared(batch, context, &self.census, track);
            candidates.extend(candidate);
            for instances in ranges {
                writes.push(InstanceWrite {
                    mesh,
                    material,
                    instances,
                });
            }
        }
        if settled {
            self.settled.at = Some((stamp, track));
        }
        writes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::totals::Totals;

    fn stamp(totals: &Totals) -> Stamp {
        Stamp::of_totals(totals)
    }

    #[test]
    fn the_memo_holds_only_for_the_pair_it_was_set_at() {
        let totals = Totals::default();
        let memo = SettledMemo {
            at: Some((stamp(&totals), false)),
            ..SettledMemo::default()
        };
        assert!(memo.holds(stamp(&totals), false));
        assert!(!memo.holds(stamp(&totals), true), "track_previous");
        assert!(!SettledMemo::default().holds(stamp(&totals), false));
    }

    #[test]
    fn each_counter_of_the_stamp_ends_the_memo_alone() {
        for moved in 0..3 {
            let totals = Totals::default();
            let memo = SettledMemo {
                at: Some((stamp(&totals), true)),
                ..SettledMemo::default()
            };
            [&totals.edits, &totals.content, &totals.history][moved].add(1);
            assert!(!memo.holds(stamp(&totals), true), "counter {moved}");
        }
    }
}
