//! Where shadow casters changed since a retained cascade last looked, so a cascade
//! refreshes only when a change can reach its map.
use glam::{Mat4, Vec3};

use super::SceneGpu;
use super::visibility::{Bounds, Frustum, caster_placement, relief_ray_stretch};
use crate::instances::InstanceHandle;

// Past this many unseen changes they become one generation change, which refreshes
// every cascade, so frames without a shadow consumer cannot grow the list.
const PENDING_LIMIT: usize = 4096;

/// World bounds a caster covered, before relief, and its largest relief scale.
#[derive(Clone, Copy, Debug)]
pub(super) struct CasterBounds {
    pub bounds: Bounds,
    pub relief: f32,
}

#[derive(Debug, Default)]
pub(super) struct CasterEdits {
    /// Changes that can reach any cascade: resources, LOD scales, overflow.
    pub generation: u64,
    sequence: u64,
    pending: Vec<(u64, CasterBounds)>,
}

impl CasterEdits {
    fn push(&mut self, change: CasterBounds) {
        if self.pending.len() == PENDING_LIMIT {
            self.pending.clear();
            self.generation = self.generation.wrapping_add(1);
        }
        self.sequence += 1;
        self.pending.push((self.sequence, change));
    }

    pub fn global(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

impl SceneGpu {
    /// The caster bounds of a visible shadow-casting placement, or `None`.
    pub(super) fn caster_bounds(&self, handle: InstanceHandle) -> Option<CasterBounds> {
        caster_placement(&self.instances, &self.mesh_selection, handle)
    }

    /// Records the bounds a caster left and the bounds it now covers.
    pub(super) fn record_caster_change(
        &mut self,
        before: Option<CasterBounds>,
        after: Option<CasterBounds>,
    ) {
        for change in before.into_iter().chain(after) {
            self.casters.push(change);
        }
    }

    /// Sequence of the latest localized caster change.
    pub(crate) fn caster_sequence(&self) -> u64 {
        self.casters.sequence
    }

    /// Whether a change after `seen` can reach a map rendered with `matrix`.
    pub(crate) fn casters_changed_within(&self, seen: u64, matrix: Mat4) -> bool {
        let frustum = Frustum::new(matrix);
        let stretch = relief_ray_stretch(matrix);
        self.casters
            .pending
            .iter()
            .filter(|(sequence, _)| *sequence > seen)
            .any(|(_, change)| {
                frustum.intersects(change.bounds.expanded(Vec3::splat(change.relief * stretch)))
            })
    }

    /// Drops changes every consumer has examined.
    pub(crate) fn forget_caster_changes_through(&mut self, seen: u64) {
        self.casters
            .pending
            .retain(|(sequence, _)| *sequence > seen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(x: f32) -> CasterBounds {
        CasterBounds {
            bounds: Bounds {
                min: Vec3::new(x, -0.5, -0.5),
                max: Vec3::new(x + 1.0, 0.5, 0.5),
            },
            relief: 0.0,
        }
    }

    #[test]
    fn overflow_becomes_one_generation_change() {
        let mut edits = CasterEdits::default();
        for _ in 0..PENDING_LIMIT {
            edits.push(change(0.0));
        }
        assert_eq!((edits.generation, edits.pending.len()), (0, PENDING_LIMIT));
        edits.push(change(0.0));
        assert_eq!((edits.generation, edits.pending.len()), (1, 1));
        assert_eq!(edits.sequence, PENDING_LIMIT as u64 + 1);
    }
}
