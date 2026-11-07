//! Counts of what a runtime has built, for tests that prove a steady state builds nothing.

use std::sync::atomic::{AtomicU64, Ordering};

/// How many objects a [`ProgramRuntime`](crate::runtime::ProgramRuntime) has created since it was
/// made.
///
/// Every field only rises. A cost test takes a snapshot, runs a frame and compares: a field that
/// did not move means the frame built none of that object. A cache hit builds nothing, and a call
/// that fails before it creates the object does not count it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeStats {
    /// Shader modules compiled. Stages with the same source, entry point and kind share one.
    pub modules_built: u64,
    /// Render and compute pipelines created. Equal programs, state and targets share one.
    pub pipelines_built: u64,
    /// Bind groups created. A set that a lender binds (an external set) never adds to this.
    pub bind_groups_built: u64,
    /// Reusable bind group sets prepared ahead of any draw. No call prepares one yet, so this is
    /// always zero; it is part of the counter set so that a cost test written now stays valid
    /// when sets can be prepared.
    pub sets_prepared: u64,
    /// Draws prepared, whether by `prepare_draw`, a material, an admission or
    /// `prepare_imported_draw`.
    pub draws_prepared: u64,
}

/// The runtime's counters. They are atomic so that counting needs only a shared reference, as
/// bind groups are built under one.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    modules: AtomicU64,
    pipelines: AtomicU64,
    bind_groups: AtomicU64,
    draws: AtomicU64,
}

impl Counters {
    pub(crate) fn module_built(&self) {
        self.modules.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn pipeline_built(&self) {
        self.pipelines.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn bind_group_built(&self) {
        self.bind_groups.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn draw_prepared(&self) {
        self.draws.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> RuntimeStats {
        RuntimeStats {
            modules_built: self.modules.load(Ordering::Relaxed),
            pipelines_built: self.pipelines.load(Ordering::Relaxed),
            bind_groups_built: self.bind_groups.load(Ordering::Relaxed),
            sets_prepared: 0,
            draws_prepared: self.draws.load(Ordering::Relaxed),
        }
    }
}
