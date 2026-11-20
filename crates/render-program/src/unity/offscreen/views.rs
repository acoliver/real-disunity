//! The texture views one build shares.
//!
//! A draw binds a view of every texture it samples, and a frame's draws mostly sample the same
//! textures at the same versions. Making each view anew costs a wgpu view object for every
//! binding, which the build keeps until its operations are submitted. A build makes the view of
//! a range at a version once and gives every later draw a clone of it.
//!
//! The cache belongs to one [`Tracker`](super::tracker::Tracker) and goes with it, so it never
//! outlives the allocations the tracker imported. An allocation the registry replaces takes a new
//! generation in its key, so a view of the old storage cannot be found under the new key.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use worldrunner_render_post::contracts::{AllocationKey, CheckedView, ContractError, TextureRange};

/// How a build's draws got their texture views.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewCounts {
    /// Views made on the device.
    pub created: usize,
    /// Views a draw took from one an earlier draw made.
    pub reused: usize,
}

/// A view as the registry would make it: the range and dimension of a version of an allocation.
/// The dimension is `None` where the registry picks it from the number of layers.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct ViewKey {
    pub(super) allocation: AllocationKey,
    pub(super) range: TextureRange,
    pub(super) version: u32,
    pub(super) dimension: Option<wgpu::TextureViewDimension>,
}

#[derive(Default)]
struct Shared {
    views: HashMap<ViewKey, CheckedView>,
    counts: ViewCounts,
}

#[derive(Default)]
pub(super) struct ViewCache {
    shared: Mutex<Shared>,
}

impl ViewCache {
    /// The view under `key`, made by `make` the first time it is asked for.
    pub(super) fn get_or_make(
        &self,
        key: ViewKey,
        make: impl FnOnce() -> Result<CheckedView, ContractError>,
    ) -> Result<CheckedView, ContractError> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(view) = shared.views.get(&key) {
            let view = view.clone();
            shared.counts.reused += 1;
            return Ok(view);
        }
        let view = make()?;
        shared.counts.created += 1;
        shared.views.insert(key, view.clone());
        Ok(view)
    }

    pub(super) fn counts(&self) -> ViewCounts {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .counts
    }
}
