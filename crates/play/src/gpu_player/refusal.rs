//! The work of a run that the GPU would not take, which ends the run: what the player drew after
//! it, or kept drawing from before it, is not the game's frame.

use super::PlayerGpu;
use crate::offscreen::{Report, WorkTrace};

/// The name that a cause of a failure has when the storage budget refused the allocation.
const OVER_BUDGET: &str = "OverBudget";

/// The kinds of failure for stored textures and meshes. A refused upload is not final: the engine
/// gives the stored texture or mesh again with the next snapshot that names it, so it can go up
/// once other storage is free.
const UPLOADS: [&str; 2] = ["texture:", "mesh:"];

/// How a frame or flush the GPU refused whole is worded, with the error it gave.
pub(super) fn whole(trace: &WorkTrace) -> Option<String> {
    let error = trace.error.as_ref()?;
    Some(format!("{} refused by the GPU: {error}", trace.kind))
}

/// How the first allocation of the frame's own work that the storage budget refused in `report`
/// is worded, for a frame or flush that `source` names: a render texture or a record the work
/// went without.
pub(super) fn over_budget(report: &Report, source: &str) -> Option<String> {
    let cause = report.causes.keys().find(|cause| {
        cause.split('/').any(|name| name == OVER_BUDGET)
            && !UPLOADS.iter().any(|kind| cause.starts_with(kind))
    })?;
    let example = report.examples.get(cause).map_or("", String::as_str);
    Some(format!("{source} refused by the GPU: {cause}: {example}"))
}

impl PlayerGpu {
    /// The first refusal since the last call, for the run to end at: a frame or flush the GPU
    /// refused whole, or work that went without an allocation the storage budget refused. A stored
    /// texture or mesh whose upload the budget refuses is asked for again, so it is not one.
    pub(crate) fn take_refusal(&self) -> Option<String> {
        self.report.borrow_mut().refusal.take()
    }
}
