//! Per-slot storage for view-independent placement bounds, grouped for culling, the
//! eye-independent inputs of each placement's level, and the level it draws at for the
//! current eye.

use std::sync::atomic::{AtomicU16, Ordering};

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
/// slots by position and is rebuilt after any slot's boxes change or a slot is added or
/// removed.
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

    /// Fills the boxes and level inputs of every live slot that has none and regroups the
    /// slots when they changed. `live` lists the batch's allocated slots in order with their
    /// placements; `fill` computes one placement's boxes, one per relief scale, and its inputs
    /// laid out by `shape`.
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
        if reliefs != self.reliefs || shape != self.inputs.shape() {
            self.reliefs = reliefs.to_vec();
            self.inputs.reset(shape);
            self.boxes.clear();
            self.valid.clear();
            self.stale = true;
        }
        let mut slots = Vec::new();
        for (position, placement) in live {
            if position >= self.valid.len() {
                self.valid.resize(position + 1, false);
                self.boxes.resize((position + 1) * stride, [Vec3::ZERO; 2]);
                self.inputs.grow(position + 1);
            }
            if !self.valid[position] {
                fill(
                    placement,
                    &mut self.boxes[position * stride..(position + 1) * stride],
                    self.inputs.slot_mut(position),
                );
                self.valid[position] = true;
                self.stale = true;
            }
            slots.push(position);
        }
        self.levels.grow(self.valid.len());
        if self.stale {
            self.tree = Tree::build(
                slots
                    .into_iter()
                    .map(|position| (position, self.union(position)))
                    .collect(),
            );
            self.stale = false;
        }
    }

    /// The boxes of a live slot, one per relief scale; call after `refresh`.
    pub fn boxes(&self, position: usize) -> &[[Vec3; 2]] {
        debug_assert!(self.valid[position], "refreshed slot bounds");
        let stride = self.reliefs.len();
        &self.boxes[position * stride..(position + 1) * stride]
    }

    fn union(&self, position: usize) -> [Vec3; 2] {
        self.boxes(position)
            .iter()
            .copied()
            .reduce(|[min, max], [low, high]| [min.min(low), max.max(high)])
            .expect("a batch has at least one material")
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

    #[cfg(test)]
    pub fn is_cached(&self, position: usize) -> bool {
        self.valid.get(position).copied().unwrap_or(false)
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

/// Tells eyes apart for every batch's level memo: each new eye takes the next epoch, so an
/// entry stored for an earlier eye reads as empty without anything being written to it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LevelEpochs {
    key: Option<LevelKey>,
    /// The epoch of `key`, from 1; no entry holds epoch 0.
    epoch: u8,
}

impl LevelEpochs {
    /// The epoch of `key`, and whether the epochs wrapped to reach it, in which case entries of
    /// every earlier epoch may still be stored and the caller must empty every memo.
    pub fn advance(&mut self, key: LevelKey) -> (u8, bool) {
        if self.key == Some(key) {
            return (self.epoch, false);
        }
        self.key = Some(key);
        let (epoch, wrapped) = match self.epoch.checked_add(1) {
            Some(epoch) => (epoch, false),
            None => (1, true),
        };
        self.epoch = epoch;
        (epoch, wrapped)
    }
}

/// What each slot draws at for one eye, filled during selection by whichever view needs it
/// first. A slot's entry depends only on its placement, its batch's mesh and materials and
/// the eye, so concurrent views may fill it in any order.
///
/// Each entry holds the epoch of the eye it was filled for above its outcome (see
/// [`LevelEpochs`]), so a new eye empties every entry without a write to any of them.
#[derive(Debug, Default)]
pub(crate) struct Levels {
    values: Vec<AtomicU16>,
}

/// A slot's memoized selection outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    /// Every fragment the placement could draw is faded out.
    Faded,
    /// Drawn at this level of detail.
    Drawn(usize),
}

const FADED: u8 = 1;
const DRAWN: u8 = 2;

impl Levels {
    /// Makes room for `slots` slots.
    fn grow(&mut self, slots: usize) {
        if self.values.len() < slots {
            self.values.resize_with(slots, || AtomicU16::new(0));
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

    /// The outcome of `position` for the eye of `epoch`, computed by `compute` unless an
    /// earlier view stored it.
    pub fn get_or_compute(
        &self,
        epoch: u8,
        position: usize,
        compute: impl FnOnce() -> Level,
    ) -> Level {
        let value = &self.values[position];
        let [stored_epoch, outcome] = value.load(Ordering::Relaxed).to_be_bytes();
        if stored_epoch != epoch {
            let level = compute();
            let outcome = match level {
                Level::Faded => FADED,
                Level::Drawn(lod) => {
                    u8::try_from(lod + usize::from(DRAWN)).expect("levels of detail fit the memo")
                }
            };
            value.store(u16::from_be_bytes([epoch, outcome]), Ordering::Relaxed);
            return level;
        }
        match outcome {
            FADED => Level::Faded,
            stored => Level::Drawn(usize::from(stored - DRAWN)),
        }
    }
}

#[cfg(test)]
mod levels_tests;
