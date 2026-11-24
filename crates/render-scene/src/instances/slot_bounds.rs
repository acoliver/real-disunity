//! Per-slot storage for view-independent placement bounds, grouped for culling, the
//! eye-independent inputs of each placement's level, and the level it draws at, kept while the
//! eye stays near where it was computed.

#[cfg(test)]
mod test_hooks;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::Vec3;

mod inputs;
mod tree;
use inputs::is_distinct_of;
pub(crate) use inputs::{Shape, Slot, SlotFade, SlotMut, distinct};
pub(crate) use tree::{Node, Tree};
use worldrunner_render_plan::SceneFadeReference;

/// World-space boxes for the allocated slots of one batch, one box per relief scale, and
/// each slot's level inputs.
///
/// Visibility selection fills every live slot's boxes and inputs before it selects, and every
/// write to a slot clears them. The batch's relief scales and input shape depend only on its
/// mesh and materials, so a change of either discards every entry. The tree groups the live
/// slots by position and follows the slots whose boxes change and those added or removed,
/// until its grouping has worn enough to build it afresh.
#[derive(Debug, Default)]
pub(crate) struct SlotBounds {
    /// Distinct relief scales by bit pattern; each slot stores one box per scale.
    reliefs: Vec<u32>,
    boxes: Vec<[Vec3; 2]>,
    valid: Vec<bool>,
    tree: Tree,
    /// Whether `tree` no longer covers exactly the live slots and their boxes.
    stale: bool,
    inputs: inputs::Inputs,
    levels: Levels,
}

impl SlotBounds {
    pub fn invalidate(&mut self, position: usize) {
        if let Some(valid) = self.valid.get_mut(position) {
            *valid = false;
        }
        self.levels.forget(position);
        self.tree.remove(position);
        self.stale = true;
    }

    /// Whether `refresh` would find nothing to do for a batch whose materials make these relief
    /// scales and fade references over a mesh of `levels` levels: no slot changed and the stored
    /// layout is the one they make. Each iterator lists its values per material and may repeat
    /// them; only their distinct values in order of first appearance are compared, and nothing
    /// is allocated.
    pub fn is_current(
        &self,
        reliefs: impl Iterator<Item = u32>,
        levels: usize,
        references: impl Iterator<Item = SceneFadeReference>,
    ) -> bool {
        let shape = self.inputs.shape();
        !self.stale
            && shape.levels == levels
            && is_distinct_of(reliefs, &self.reliefs)
            && is_distinct_of(references, &shape.references)
    }

    /// Fills the boxes and level inputs of every live slot that has none and brings the grouping
    /// of the slots up to date: slots that changed rejoin it, and a grouping that its changes
    /// have worn, or that a new layout invalidates, is built afresh. `live` lists the batch's
    /// allocated slots in order with their placements; `fill` computes one placement's boxes,
    /// one per relief scale, and its inputs laid out by `shape`.
    pub fn refresh<'a, P: 'a>(
        &mut self,
        (reliefs, shape): (&[u32], &Shape),
        live: impl Iterator<Item = (usize, &'a P)>,
        mut fill: impl FnMut(&P, &mut [[Vec3; 2]], SlotMut<'_>),
    ) {
        if !self.stale && reliefs == self.reliefs && shape == self.inputs.shape() {
            return;
        }
        let stride = reliefs.len();
        let new_layout = reliefs != self.reliefs || shape != self.inputs.shape();
        if new_layout {
            self.reliefs = reliefs.to_vec();
            self.inputs.reset(shape);
            self.boxes.clear();
            self.valid.clear();
            self.levels.clear();
        }
        let rebuild = new_layout || self.tree.is_degraded();
        // The slots to group afresh, or those that changed and are not in the grouping.
        let mut slots = Vec::new();
        for (position, placement) in live {
            if position >= self.valid.len() {
                self.valid.resize(position + 1, false);
                self.boxes.resize((position + 1) * stride, [Vec3::ZERO; 2]);
                self.inputs.grow(position + 1);
            }
            let changed = !self.valid[position];
            if changed {
                fill(
                    placement,
                    &mut self.boxes[position * stride..(position + 1) * stride],
                    self.inputs.slot_mut(position),
                );
                self.valid[position] = true;
            }
            if rebuild || changed {
                slots.push(position);
            }
        }
        self.levels.grow(self.valid.len());
        if rebuild {
            self.tree = Tree::build(slots, &self.boxes, stride);
        } else {
            for position in slots {
                self.tree.insert(position, &self.boxes);
            }
        }
        self.stale = false;
    }

    /// The boxes of a live slot, one per relief scale; call after `refresh`.
    pub fn boxes(&self, position: usize) -> &[[Vec3; 2]] {
        debug_assert!(self.valid[position], "refreshed slot bounds");
        let stride = self.reliefs.len();
        &self.boxes[position * stride..(position + 1) * stride]
    }

    /// Relief scales by bit pattern, in the order of each slot's boxes.
    pub fn reliefs(&self) -> &[u32] {
        &self.reliefs
    }

    /// The level inputs of a live slot; call after `refresh`.
    pub fn inputs(&self, position: usize) -> Slot<'_> {
        debug_assert!(self.valid[position], "refreshed slot inputs");
        self.inputs.slot(position)
    }

    /// The live slots grouped by position; current after `refresh`.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Empties the level memo of every eye, as when the epochs that tell eyes apart wrap.
    pub fn forget_levels(&mut self) {
        self.levels.clear();
    }

    /// The level memo, sized for the refreshed slots by `refresh`.
    pub fn levels(&self) -> &Levels {
        &self.levels
    }
}

/// The eye a level memo belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LevelKey {
    pub eye_millimeters: [i64; 3],
    pub fade_eye: [u32; 3],
    /// Whether fade culling was disabled, which only tests do.
    pub fade_cull_disabled: bool,
}

/// The farthest an entry's margin or the eye's travel may reach, in millimetres, so that an
/// entry's limit fits the bits `Levels` keeps for it.
const MAX_TRAVEL: u64 = 1 << 54;

/// How far the eye has travelled across the eyes the memos have seen. The eye is no farther
/// from where it was when an entry was computed than it has travelled since, however it
/// wandered in between, so an entry whose margin covers that travel still holds.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EyePath {
    key: Option<LevelKey>,
    /// Millimetres travelled in all: every move of the eye adds at least its length, rounded
    /// up, and at least one, so a new eye never shares the travel of an earlier one.
    travelled: u64,
}

impl EyePath {
    /// The travel at the eye `key` names, and whether it restarted to stay in range, in which
    /// case entries stored under the earlier travel may still be stored and the caller must
    /// empty every memo.
    pub fn advance(&mut self, key: LevelKey) -> (u64, bool) {
        let Some(previous) = self.key.replace(key) else {
            return (self.travelled, false);
        };
        if previous == key {
            return (self.travelled, false);
        }
        let step = (0..3)
            .map(|axis| previous.eye_millimeters[axis].abs_diff(key.eye_millimeters[axis]))
            .try_fold(0_u128, |sum, delta| {
                sum.checked_add(u128::from(delta) * u128::from(delta))
            })
            .map(|squared| {
                let floor = squared.isqrt();
                floor + u128::from(floor * floor < squared)
            })
            .and_then(|length| u64::try_from(length.max(1)).ok());
        match step.and_then(|step| self.travelled.checked_add(step)) {
            Some(travelled) if travelled <= MAX_TRAVEL => {
                self.travelled = travelled;
                (travelled, false)
            }
            _ => {
                self.travelled = 0;
                (0, true)
            }
        }
    }
}

/// What each slot draws at, kept from the eye it was computed for until the eye has moved too
/// far for the outcome to be sure. A slot's entry depends only on its placement, its batch's
/// mesh and materials and the eye, so concurrent views may fill it in any order.
///
/// Each entry holds its outcome above the travel of the eye (see [`EyePath`]) up to which it
/// holds, so a new eye empties no entry that its margin still covers, and an entry whose margin
/// is nothing holds for the eye it was computed for alone.
#[derive(Debug, Default)]
pub(crate) struct Levels {
    values: Vec<AtomicU64>,
}

/// A slot's selection outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    /// Every fragment the placement could draw is faded out.
    Faded,
    /// Drawn at this level of detail.
    Drawn(usize),
}

/// An outcome and how far the eye may move from where it was computed, in millimetres, without
/// the outcome changing for any eye in reach.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Computed {
    pub level: Level,
    pub margin: u64,
}

const LIMIT_BITS: u32 = 56;
const LIMIT_MASK: u64 = (1 << LIMIT_BITS) - 1;
const FADED: u8 = 1;
const DRAWN: u8 = 2;

impl Levels {
    /// Makes room for `slots` slots.
    fn grow(&mut self, slots: usize) {
        if self.values.len() < slots {
            self.values.resize_with(slots, || AtomicU64::new(0));
        }
    }

    fn clear(&mut self) {
        for value in &mut self.values {
            *value.get_mut() = 0;
        }
    }

    fn forget(&mut self, position: usize) {
        if let Some(value) = self.values.get_mut(position) {
            *value.get_mut() = 0;
        }
    }

    /// The outcome of `position` for the eye that has `travelled` so far, computed by `compute`
    /// unless an earlier eye's entry still holds.
    pub fn get_or_compute(
        &self,
        travelled: u64,
        position: usize,
        compute: impl FnOnce() -> Computed,
    ) -> Level {
        let value = &self.values[position];
        let stored = value.load(Ordering::Relaxed);
        let (outcome, limit) = ((stored >> LIMIT_BITS) as u8, stored & LIMIT_MASK);
        if outcome != 0 && travelled <= limit {
            return match outcome {
                FADED => Level::Faded,
                drawn => Level::Drawn(usize::from(drawn - DRAWN)),
            };
        }
        let Computed { level, margin } = compute();
        let outcome = match level {
            Level::Faded => FADED,
            Level::Drawn(lod) => {
                u8::try_from(lod + usize::from(DRAWN)).expect("levels of detail fit the memo")
            }
        };
        let limit = travelled + margin.min(MAX_TRAVEL);
        value.store(u64::from(outcome) << LIMIT_BITS | limit, Ordering::Relaxed);
        level
    }
}

#[cfg(test)]
mod levels_tests;
