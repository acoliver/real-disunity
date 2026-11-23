//! One view's object selection, walking each batch's bounds tree.
//!
//! A view visits only the groups of placements its volume and slice may reach, tests each
//! placement in them as before, and emits the admitted slots in ascending order, so ranges,
//! levels and statistics equal those of a scan over every slot. A placement's level and fade
//! outcome depend on the eye and not on the view, so every view of a frame shares them
//! through each batch's level memo, which keeps a level while the eye stays near where it
//! was computed.
use std::cell::RefCell;
use std::ops::Range;

use glam::{Mat4, Vec3};
use worldrunner_render_plan::SceneObject;

use super::fade::FadeReach;
use super::placement::{self, largest_relief};
use super::workers;
use super::{
    Bounds, Frustum, MeshSelection, Passed, Selection, distance_squared, relief_ray_stretch,
};
use crate::draw::slice_hull::LightMap;
use crate::instances::fade::Fade;
use crate::instances::slot_bounds::{Computed, Level, LevelKey, Node, SlotBounds};
use crate::instances::{BatchKey, Instances, LayoutStamp};

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
    /// How far the eye has travelled, which tells whether an entry of a batch's level memo
    /// still holds for this eye.
    travelled: u64,
    /// Live placements in every batch, which every view considers.
    live: usize,
}

/// Slots in a leaf up to which a view tests them one by one instead of classifying the leaf.
pub(super) const SMALL_LEAF: usize = 4;

struct IndexedBatch<'a> {
    key: BatchKey,
    placements: &'a [Option<(SceneObject, bool)>],
    /// What each slot is to a view, in one byte per slot (see `CAMERA` and `CASTER`).
    flags: &'a [u8],
    bounds: &'a SlotBounds,
    largest_relief: f32,
    /// What can fade out a placement of the batch.
    fade_reach: FadeReach,
    /// Whether the batch has so few live placements, up to `SMALL_LEAF`, that its tree is one
    /// small leaf, which a view tests slot by slot without walking the tree.
    small: bool,
}

/// A slot's flag when a camera view may draw it: it is visible and not shadow-only.
const CAMERA: u8 = 1;
/// A slot's flag when a view of casters may draw it: it is visible and casts a shadow.
const CASTER: u8 = 2;

/// The flags of a placement. A view tests one byte of a dense list per slot instead of reading
/// each placement, which is far larger and lies in memory of its own.
fn flags(placement: Option<&(SceneObject, bool)>) -> u8 {
    placement.map_or(0, |(object, visible)| {
        (u8::from(*visible && !object.shadow_only()) * CAMERA)
            | (u8::from(*visible && object.casts_shadow()) * CASTER)
    })
}

/// What a view reads of each batch besides its placements and bounds, in the order of the
/// batches. None of it depends on the eye or the view, so it holds from one frame to the next
/// until a placement or a table changes, and a frame then reads it from one dense list instead
/// of deriving it from every batch.
#[derive(Debug, Default)]
pub(in crate::draw) struct PlacementRows {
    at: Option<LayoutStamp>,
    rows: Vec<Row>,
    live: usize,
    /// The flags of every batch's slots, one batch after another.
    flags: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
struct Row {
    key: BatchKey,
    /// Where this batch's flags start in the list of every batch's, and how many it has.
    flags_at: usize,
    slots: usize,
    largest_relief: f32,
    fade_reach: FadeReach,
    small: bool,
}

impl PlacementRows {
    /// Makes the rows those of `instances` in the state `stamp` names, from the bounds and
    /// placements of its batches in order, which `refresh_all` has made current.
    fn refresh<'b>(
        &mut self,
        stamp: LayoutStamp,
        instances: &Instances,
        meshes: &[MeshSelection],
        batches: impl ExactSizeIterator<Item = (&'b SlotBounds, &'b [Option<(SceneObject, bool)>])>,
    ) {
        if self.at == Some(stamp) && self.rows.len() == batches.len() {
            debug_assert!(
                instances
                    .batches
                    .keys()
                    .eq(self.rows.iter().map(|row| &row.key)),
                "batches are never removed, and a new one changes the stamp or the count"
            );
            return;
        }
        // A mesh's slots fade placements the same whichever material the placement draws with.
        let mut slot_reaches: Vec<Option<FadeReach>> = vec![None; meshes.len()];
        self.rows.clear();
        self.flags.clear();
        self.live = 0;
        for ((&key, _), (bounds, placements)) in instances.batches.iter().zip(batches) {
            let slot_reach = *slot_reaches[key.1]
                .get_or_insert_with(|| meshes[key.1].slot_fade_reach(&instances.fades));
            let live = bounds.tree().len();
            self.live += live;
            self.rows.push(Row {
                key,
                flags_at: self.flags.len(),
                slots: placements.len(),
                largest_relief: largest_relief(bounds.reliefs()),
                fade_reach: slot_reach.with(instances.fades[key.2]),
                small: live <= SMALL_LEAF,
            });
            self.flags
                .extend(placements.iter().map(Option::as_ref).map(flags));
        }
        self.at = Some(stamp);
    }
}

#[cfg(test)]
fn fade_cull_disabled(instances: &Instances) -> bool {
    instances.fade_cull_disabled
}

#[cfg(not(test))]
fn fade_cull_disabled(_: &Instances) -> bool {
    false
}

/// Refreshes every batch's bounds and tree and the memo of levels for this eye, then lends the
/// placements to `select`.
pub(in crate::draw) fn with_placements<R>(
    instances: &Instances,
    meshes: &[MeshSelection],
    (eye_millimeters, fade_eye): ([i64; 3], Vec3),
    rows: &RefCell<PlacementRows>,
    select: impl FnOnce(&Placements<'_>) -> R,
) -> R {
    let fade_cull_disabled = fade_cull_disabled(instances);
    let key = LevelKey {
        eye_millimeters,
        fade_eye: fade_eye.to_array().map(f32::to_bits),
        fade_cull_disabled,
    };
    placement::refresh_all(instances, meshes);
    let travelled = instances.eye_travel(key);
    let borrowed: Vec<_> = instances
        .batches
        .values()
        .map(|batch| (batch.bounds.borrow(), batch.slots()))
        .collect();
    let mut rows = rows.borrow_mut();
    rows.refresh(
        instances.layout_stamp(meshes.len()),
        instances,
        meshes,
        borrowed.iter().map(|(bounds, slots)| (&**bounds, *slots)),
    );
    let placements = Placements {
        batches: borrowed
            .iter()
            .zip(&rows.rows)
            .map(|((bounds, placements), row)| IndexedBatch {
                key: row.key,
                placements,
                flags: &rows.flags[row.flags_at..row.flags_at + row.slots],
                bounds,
                largest_relief: row.largest_relief,
                fade_reach: row.fade_reach,
                small: row.small,
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
        travelled,
        live: rows.live,
    };
    select(&placements)
}

impl Selection {
    /// Selects from `batches` the placements `view`, with this selection's slice and caster
    /// filter, admits, into a selection of its own that holds just those.
    fn select_chunk(
        &self,
        placements: &Placements<'_>,
        view: ViewTests<'_>,
        batches: Range<usize>,
    ) -> Selection {
        let mut chosen = Selection::default();
        chosen.stats.drawn_per_lod = vec![0; self.stats.drawn_per_lod.len()];
        let mut admitted = Admitted::default();
        let mut stack = Vec::new();
        for batch in &placements.batches[batches] {
            if batch.small {
                // The slots of a lone small leaf ascend, so they are tested and emitted in
                // place, without the list and sort of a walk.
                let tree = batch.bounds.tree();
                let members = tree
                    .slots()
                    .iter()
                    .zip(tree.boxes().chunks_exact(tree.stride()));
                for (&slot, boxes) in members {
                    if self.admits_slot(batch, (slot, boxes), view, Passed::default()) {
                        chosen.push_level(placements, batch, slot);
                    }
                }
                continue;
            }
            admitted.reset(batch.flags.len());
            self.admit(batch, view, &mut stack, &mut admitted);
            for slot in admitted.slots() {
                chosen.push_level(placements, batch, slot);
            }
        }
        chosen
    }

    /// Appends what the next chunk of this view's batches selected.
    fn append(&mut self, chunk: Selection) {
        self.ranges.extend(chunk.ranges);
        for (total, drawn) in self
            .stats
            .drawn_per_lod
            .iter_mut()
            .zip(chunk.stats.drawn_per_lod)
        {
            *total += drawn;
        }
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
        (slot, boxes): (u32, &[[Vec3; 2]]),
        view: ViewTests<'_>,
        passed: Passed,
    ) -> bool {
        batch.flags[slot as usize] & view.flag != 0
            && self.admits(view.frustum, view.light, passed, || {
                placement::placement(boxes, batch.bounds.reliefs(), view.stretch)
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
        admitted: &mut Admitted,
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
                Node::Leaf { slots, boxes, .. } => {
                    for (&slot, boxes) in slots.iter().zip(boxes.chunks_exact(tree.stride())) {
                        if self.admits_slot(batch, (slot, boxes), view, passed) {
                            admitted.insert(slot);
                        }
                    }
                }
            }
        }
    }
}

/// The slots a view admits from one batch, one bit each, which come out in ascending order
/// without a sort however the walk of the tree met them.
#[derive(Default)]
struct Admitted {
    words: Vec<u64>,
}

impl Admitted {
    /// Empties the set and makes room for slots below `slots`.
    fn reset(&mut self, slots: usize) {
        self.words.clear();
        self.words.resize(slots.div_ceil(64), 0);
    }

    fn insert(&mut self, slot: u32) {
        self.words[slot as usize / 64] |= 1 << (slot % 64);
    }

    fn slots(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(index, &word)| {
            let mut bits = word;
            let first = u32::try_from(index * 64).expect("validated instance slot");
            std::iter::from_fn(move || {
                (bits != 0).then(|| {
                    let slot = first + bits.trailing_zeros();
                    bits &= bits - 1;
                    slot
                })
            })
        })
    }
}

/// What one view tests placements against.
#[derive(Clone, Copy)]
struct ViewTests<'a> {
    frustum: &'a Frustum,
    light: &'a LightMap,
    /// The view's relief ray stretch.
    stretch: f32,
    /// The flag a slot must have for the view to draw it.
    flag: u8,
}

impl Placements<'_> {
    /// How many of `available` threads selecting `views` views is worth their cost, at least one.
    pub fn threads_for(&self, views: usize, available: usize) -> usize {
        workers::threads_for(self.live * views, available)
    }

    /// Selects each view with its matrix on `threads` threads, the caller's among them.
    ///
    /// The views' batches are divided into chunks of about equal work, which the threads take
    /// as they finish the last. A chunk only reads the placements and fills the shared memo of
    /// levels, which every view of the frame computes the same, and returns what it selected,
    /// so the selections equal those of one view after another whatever the threads do.
    pub fn select_each(&self, views: &mut [(Mat4, &mut Selection)], threads: usize) {
        let tests: Vec<_> = views
            .iter_mut()
            .map(|(matrix, selection)| {
                selection.reset(self.meshes);
                // Traversal covers allocated slots; removed slots contribute neither selection
                // work nor statistics.
                selection.stats.considered = self.live;
                let stretch = if self.relief {
                    relief_ray_stretch(*matrix)
                } else {
                    1.0
                };
                (Frustum::new(*matrix), LightMap::new(*matrix), stretch)
            })
            .collect();
        let work: Vec<usize> = self
            .batches
            .iter()
            .map(|batch| batch.bounds.tree().len())
            .collect();
        let chunks = workers::chunks(&work, views.len(), threads);
        let selected = workers::run(chunks.len(), threads, |index| {
            let chunk = &chunks[index];
            let selection: &Selection = &*views[chunk.view].1;
            let (frustum, light, stretch) = &tests[chunk.view];
            let view = ViewTests {
                frustum,
                light,
                stretch: *stretch,
                flag: if selection.casters_only {
                    CASTER
                } else {
                    CAMERA
                },
            };
            selection.select_chunk(self, view, chunk.batches.clone())
        });
        for (chunk, selected) in chunks.iter().zip(selected) {
            views[chunk.view].1.append(selected);
        }
        for (_, selection) in views.iter_mut() {
            selection.stats.culled =
                selection.stats.considered - selection.stats.drawn_per_lod.iter().sum::<usize>();
        }
    }

    /// The level `slot` draws at, or whether every fragment it could draw is faded out.
    fn level(&self, batch: &IndexedBatch<'_>, slot: usize) -> Level {
        batch
            .bounds
            .levels()
            .get_or_compute(self.travelled, slot, || {
                let (object, _) = batch.placements[slot].as_ref().expect("live instance");
                let inputs = batch.bounds.inputs(slot);
                let distance = distance_squared(
                    object.transform().position_millimeters,
                    self.eye_millimeters,
                );
                let lod = MeshSelection::level(distance, inputs.ceilings);
                if batch.fade_reach.applies(inputs.fade.band) {
                    // A fade can change with any step of the eye, so it holds for this eye alone.
                    let faded = !self.fade_cull_disabled
                        && self.meshes[batch.key.1].fade_empty(
                            self.fades,
                            object.material(),
                            &inputs,
                            lod,
                            self.fade_eye,
                        );
                    return Computed {
                        level: if faded {
                            Level::Faded
                        } else {
                            Level::Drawn(lod)
                        },
                        margin: 0,
                    };
                }
                Computed {
                    level: Level::Drawn(lod),
                    margin: MeshSelection::level_margin(distance, inputs.ceilings, lod),
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
            &RefCell::default(),
            |placements| placements.select_each(&mut [(matrix, self)], 1),
        );
    }
}
