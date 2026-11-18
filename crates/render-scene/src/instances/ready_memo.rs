//! Remembers the last readiness scan of every batch that passed, so the views of one
//! transaction do not repeat it while nothing it reads has changed.
//!
//! [`InstanceBatch::ready_for`] reads the batch's bytes, dirty slots, staged and uploaded copies,
//! encoded marker, buffers and history, all of which change only through the three counters in
//! [`Stamp`]: `edits` for placement, byte, skin and dirty-slot changes, `content` for staged
//! uploads and buffer allocation, and the history version for everything the history and the
//! publications that follow a frame change. The scan also reads the asking transaction and its
//! status. The stamp sums counters that never fall and batches are never removed, so an equal
//! stamp means no counter moved.

use std::cell::RefCell;
use std::collections::BTreeMap;

use worldrunner_render_gpu::submission::{Publications, TransactionStatus};

use super::{BatchKey, InstanceBatch};

#[derive(Debug, Default)]
pub(super) struct ReadyMemo {
    passed: RefCell<Option<Passed>>,
    #[cfg(test)]
    scans: std::cell::Cell<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    edits: u64,
    content: u64,
    history: u64,
}

impl Stamp {
    fn of(batches: &BTreeMap<BatchKey, InstanceBatch>) -> Self {
        batches.values().fold(
            Self {
                edits: 0,
                content: 0,
                history: 0,
            },
            |stamp, batch| Self {
                edits: stamp.edits + batch.edits(),
                content: stamp.content + batch.content(),
                history: stamp.history + batch.history_version(),
            },
        )
    }
}

/// A scan that passed: who asked, in what state their transaction was, and the batches' stamp.
#[derive(Debug)]
struct Passed {
    owner: Option<(Publications, TransactionStatus)>,
    stamp: Stamp,
}

impl Passed {
    fn answers(&self, owner: Option<&Publications>, stamp: Stamp) -> bool {
        self.stamp == stamp
            && match (&self.owner, owner) {
                (None, None) => true,
                (Some((held, status)), Some(owner)) => {
                    held.same_transaction(owner) && *status == owner.status()
                }
                _ => false,
            }
    }
}

impl ReadyMemo {
    /// Whether every batch is ready for `owner`. A failing scan is never remembered.
    pub fn check(
        &self,
        batches: &BTreeMap<BatchKey, InstanceBatch>,
        owner: Option<&Publications>,
    ) -> bool {
        let stamp = Stamp::of(batches);
        if self
            .passed
            .borrow()
            .as_ref()
            .is_some_and(|passed| passed.answers(owner, stamp))
        {
            return true;
        }
        #[cfg(test)]
        self.scans.set(self.scans.get() + 1);
        let ready = batches.values().all(|batch| batch.ready_for(owner));
        if ready {
            *self.passed.borrow_mut() = Some(Passed {
                owner: owner.map(|owner| (owner.clone(), owner.status())),
                stamp,
            });
        }
        ready
    }

    /// How many times a check has scanned the batches.
    #[cfg(test)]
    pub fn scans(&self) -> usize {
        self.scans.get()
    }
}
