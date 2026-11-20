//! The written layers of a persistent backing as the frame in preparation found them.
//!
//! A fitted capture submits a frame in two segments. The first records what it wrote when it is
//! submitted, which is before the second plans, so the published flags no longer describe the
//! frame the second segment belongs to. A snapshot taken when the frame first stages an effect on
//! a backing lets every segment of that frame answer with one state.
//!
//! The snapshot is kept per thread because a frame's write set ([`Publications`]) is not `Send`,
//! and the backing, which is shared across generations and threads, must stay `Send` and `Sync`.
//! A snapshot answers only for the frame it was taken in. Once that frame is submitted,
//! cancelled or its capture retired, no later frame matches it and it is dropped.
use super::targets::Backing;
use std::cell::RefCell;
use std::sync::{Arc, Weak};
use worldrunner_render_gpu::submission::Publications;

struct Snapshot {
    backing: Weak<Backing>,
    frame: Publications,
    layers: Vec<bool>,
}

impl Snapshot {
    /// Whether the frame the snapshot was taken in can still be in preparation.
    fn live(&self) -> bool {
        self.backing.strong_count() > 0 && self.frame.same_frame(&self.frame)
    }

    fn describes(&self, backing: *const Backing, frame: &Publications) -> bool {
        std::ptr::eq(self.backing.as_ptr(), backing) && self.frame.same_frame(frame)
    }
}

thread_local! {
    static SNAPSHOTS: RefCell<Vec<Snapshot>> = const { RefCell::new(Vec::new()) };
}

/// Keeps `layers`, the published flags of `backing`, as its state at the start of `frame`,
/// unless an earlier segment of the same frame already did.
pub(super) fn remember(backing: &Arc<Backing>, frame: &Publications, layers: Vec<bool>) {
    SNAPSHOTS.with_borrow_mut(|all| {
        all.retain(Snapshot::live);
        if !all
            .iter()
            .any(|snapshot| snapshot.describes(Arc::as_ptr(backing), frame))
        {
            all.push(Snapshot {
                backing: Arc::downgrade(backing),
                frame: frame.clone(),
                layers,
            });
        }
    });
}

/// The flags of `backing` at the start of the frame in preparation, when that frame has kept
/// them: `None` outside a frame, and in a frame that has not yet staged an effect on `backing`,
/// where the published flags are the frame's start.
pub(super) fn recall(backing: &Backing) -> Option<Vec<bool>> {
    let current = Publications::current()?;
    SNAPSHOTS.with_borrow(|all| {
        all.iter()
            .find(|snapshot| snapshot.describes(backing, &current))
            .map(|snapshot| snapshot.layers.clone())
    })
}

/// How many snapshots this thread keeps.
#[cfg(test)]
pub(super) fn kept() -> usize {
    SNAPSHOTS.with_borrow(Vec::len)
}
