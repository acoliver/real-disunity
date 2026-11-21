//! One view's object selection, walking each batch's bounds tree.
//!
//! A view visits only the groups of placements its volume and slice may reach, tests each
//! placement in them as before, and emits the admitted slots in ascending order, so ranges,
//! levels and statistics equal those of a scan over every slot. A placement's level and fade
//! outcome depend on the eye and not on the view, so every view of a frame shares them
//! through each batch's level memo.
use std::cell::Ref;

use glam::{Mat4, Vec3};
use worldrunner_render_plan::SceneObject;

use super::fade::FadeReach;
use super::placement::{self, largest_relief};
use super::{
    Bounds, Frustum, MeshSelection, Passed, Selection, distance_squared, relief_ray_stretch,
};
use crate::draw::slice_hull::LightMap;
use crate::instances::fade::Fade;
use crate::instances::slot_bounds::{Level, LevelKey, Node, SlotBounds};
use crate::instances::{BatchKey, InstanceBatch, Instances};

/// What every view of one frame selects from. It holds only shared references to plain data
/// and atomics, so views may select on several threads at once.
pub(in crate::draw) struct Placements<'a> {
    batches: Vec<IndexedBatch<'a>>,
    meshes: &'a [MeshSelection],
    fades: &'a [Fade],
    fade_cull_disabled: bool,
    /// Whether any material has impostor relief, which makes bounds depend on the view.
    relief: bool,
    eye_millimeters: [i64; 3],
    fade_eye: Vec3,
}

/// Slots in a leaf up to which a view tests them one by one instead of classifying the leaf.
pub(super) const SMALL_LEAF: usize = 4;

struct IndexedBatch<'a> {
    key: BatchKey,
    placements: &'a [Option<(SceneObject, bool)>],
    bounds: &'a SlotBounds,
    largest_relief: f32,
    /// What can fade out a placement of the batch.
    fade_reach: FadeReach,
}

#[cfg(test)]
fn fade_cull_disabled(instances: &Instances) -> bool {
    instances.fade_cull_disabled
}

#[cfg(not(test))]
fn fade_cull_disabled(_: &Instances) -> bool {
    false
}

/// Refreshes every batch's bounds, tree and level memo for this eye, then lends the
/// placements to `select`.
pub(in crate::draw) fn with_placements<R>(
    instances: &Instances,
    meshes: &[MeshSelection],
    (eye_millimeters, fade_eye): ([i64; 3], Vec3),
    select: impl FnOnce(&Placements<'_>) -> R,
) -> R {
    let fade_cull_disabled = fade_cull_disabled(instances);
    let key = LevelKey {
        eye_millimeters,
        fade_eye: fade_eye.to_array().map(f32::to_bits),
        fade_cull_disabled,
    };
    placement::refresh_all(instances, meshes);
    for batch in instances.batches.values() {
        batch.bounds.borrow_mut().prepare_levels(key);
    }
    let borrowed: Vec<(BatchKey, &InstanceBatch, Ref<'_, SlotBounds>)> = instances
        .batches
        .iter()
        .map(|(&key, batch)| (key, batch, batch.bounds.borrow()))
        .collect();
    // A mesh's slots fade placements the same whichever material the placement draws with.
    let mut slot_reaches: Vec<Option<FadeReach>> = vec![None; meshes.len()];
    let placements = Placements {
        batches: borrowed
            .iter()
            .map(|(key, batch, bounds)| {
                let slot_reach = *slot_reaches[key.1]
                    .get_or_insert_with(|| meshes[key.1].slot_fade_reach(&instances.fades));
                IndexedBatch {
                    key: *key,
                    placements: batch.slots(),
                    bounds,
                    largest_relief: largest_relief(bounds.reliefs()),
                    fade_reach: slot_reach.with(instances.fades[key.2]),
                }
            })
            .collect(),
        meshes,
        fades: &instances.fades,
        fade_cull_disabled,
        relief: instances
            .impostor_extents
            .iter()
            .any(|extent| extent[1] != 0.0),
        eye_millimeters,
        fade_eye,
    };
    select(&placements)
}

impl Selection {
    /// Selects the placements this view's `matrix`, slice and caster filter admit.
    pub fn select(&mut self, placements: &Placements<'_>, matrix: Mat4) {
        self.reset(placements.meshes);
        let frustum = Frustum::new(matrix);
        let light = LightMap::new(matrix);
        let stretch = if placements.relief {
            relief_ray_stretch(matrix)
        } else {
            1.0
        };
        let view = ViewTests {
            frustum: &frustum,
            light: &light,
            stretch,
        };
        let mut admitted = Vec::new();
        let mut stack = Vec::new();
        for batch in &placements.batches {
            let tree = batch.bounds.tree();
            // Traversal covers allocated slots; removed slots contribute neither
            // selection work nor statistics.
            self.stats.considered += tree.len();
            if let Some(Node::Leaf { slots, .. }) = tree.root().map(|root| tree.node(root))
                && slots.len() <= SMALL_LEAF
            {
                // The slots of a lone small leaf ascend, so they are tested and emitted in
                // place, without the list and sort of a walk.
                for &slot in slots {
                    if self.admits_slot(batch, slot, view, Passed::default()) {
                        self.push_level(placements, batch, slot);
                    }
                }
                continue;
            }
            admitted.clear();
            self.admit(batch, view, &mut stack, &mut admitted);
            admitted.sort_unstable();
            for &slot in &admitted {
                self.push_level(placements, batch, slot);
            }
        }
        self.stats.culled = self.stats.considered - self.stats.drawn_per_lod.iter().sum::<usize>();
    }

    /// Emits `slot` at the level it draws at, unless every fragment of it is faded out.
    fn push_level(&mut self, placements: &Placements<'_>, batch: &IndexedBatch<'_>, slot: u32) {
        let slot = slot as usize;
        if let Level::Drawn(lod) = placements.level(batch, slot) {
            self.push(batch.key, lod, slot);
        }
    }

    /// Whether `slot` is visible, casts when only casters count, is not shadow-only when the
    /// view is a camera, and has bounds the view admits, given the tests its group `passed`.
    fn admits_slot(
        &self,
        batch: &IndexedBatch<'_>,
        slot: u32,
        view: ViewTests<'_>,
        passed: Passed,
    ) -> bool {
        let (object, visible) = batch.placements[slot as usize]
            .as_ref()
            .expect("live instance");
        *visible
            && if self.casters_only {
                object.casts_shadow()
            } else {
                !object.shadow_only()
            }
            && self.admits(view.frustum, view.light, passed, || {
                placement::placement(
                    batch.bounds.boxes(slot as usize),
                    batch.bounds.reliefs(),
                    view.stretch,
                )
            })
    }

    /// Collects the slots of `batch` that are visible, cast when only casters count, are not
    /// shadow-only when the view is a camera, and whose bounds the view admits, skipping groups
    /// no placement of which it can admit.
    fn admit(
        &self,
        batch: &IndexedBatch<'_>,
        view: ViewTests<'_>,
        stack: &mut Vec<(usize, Passed)>,
        admitted: &mut Vec<u32>,
    ) {
        let tree = batch.bounds.tree();
        // Each placement grows by at most the largest relief, so the grown group box holds
        // every grown placement box: rounding of subtraction and addition is monotone.
        let growth = Vec3::splat(batch.largest_relief * view.stretch);
        stack.clear();
        stack.extend(tree.root().map(|root| (root, Passed::default())));
        while let Some((index, passed)) = stack.pop() {
            let node = tree.node(index);
            let passed = match node {
                // A few slots cost less to test one by one than their group costs to classify.
                Node::Leaf { slots, .. } if slots.len() <= SMALL_LEAF => passed,
                _ => {
                    let [min, max] = node.bounds();
                    let group = Bounds { min, max }.expanded(growth);
                    let Some(passed) = self.classify(view.frustum, view.light, group, passed)
                    else {
                        continue;
                    };
                    passed
                }
            };
            match node {
                Node::Inner { children, .. } => {
                    stack.extend(children.map(|child| (child, passed)));
                }
                Node::Leaf { slots, .. } => {
                    admitted.extend(
                        slots
                            .iter()
                            .copied()
                            .filter(|&slot| self.admits_slot(batch, slot, view, passed)),
                    );
                }
            }
        }
    }
}

/// What one view tests placements against.
#[derive(Clone, Copy)]
struct ViewTests<'a> {
    frustum: &'a Frustum,
    light: &'a LightMap,
    /// The view's relief ray stretch.
    stretch: f32,
}

impl Placements<'_> {
    /// Selects each view with its matrix, every view after the first on a thread of its own.
    pub fn select_each(&self, views: &mut [(Mat4, &mut Selection)]) {
        let Some(((matrix, first), rest)) = views.split_first_mut() else {
            return;
        };
        std::thread::scope(|scope| {
            for (matrix, selection) in rest.iter_mut() {
                let matrix = *matrix;
                scope.spawn(move || selection.select(self, matrix));
            }
            first.select(self, *matrix);
        });
    }

    /// The level `slot` draws at, or whether every fragment it could draw is faded out.
    fn level(&self, batch: &IndexedBatch<'_>, slot: usize) -> Level {
        batch.bounds.levels().get_or_compute(slot, || {
            let (object, _) = batch.placements[slot].as_ref().expect("live instance");
            let inputs = batch.bounds.inputs(slot);
            let distance = distance_squared(
                object.transform().position_millimeters,
                self.eye_millimeters,
            );
            let lod = MeshSelection::level(distance, inputs.ceilings);
            if !self.fade_cull_disabled
                && batch.fade_reach.applies(inputs.fade.band)
                && self.meshes[batch.key.1].fade_empty(
                    self.fades,
                    object.material(),
                    &inputs,
                    lod,
                    self.fade_eye,
                )
            {
                Level::Faded
            } else {
                Level::Drawn(lod)
            }
        })
    }
}

#[cfg(test)]
impl Selection {
    pub fn rebuild(
        &mut self,
        instances: &Instances,
        meshes: &[MeshSelection],
        matrix: Mat4,
        eye_millimeters: [i64; 3],
    ) {
        self.rebuild_with_eye(
            instances,
            meshes,
            matrix,
            eye_millimeters,
            crate::geometry::millimeters_to_meters(eye_millimeters),
        );
    }

    pub fn rebuild_with_eye(
        &mut self,
        instances: &Instances,
        meshes: &[MeshSelection],
        matrix: Mat4,
        eye_millimeters: [i64; 3],
        fade_eye: Vec3,
    ) {
        with_placements(
            instances,
            meshes,
            (eye_millimeters, fade_eye),
            |placements| self.select(placements, matrix),
        );
    }
}
