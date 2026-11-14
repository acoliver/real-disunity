//! Per-slot storage for view-independent placement bounds, grouped for culling, and the
//! level each placement draws at for the current eye.

use std::sync::atomic::{AtomicU8, Ordering};

use glam::Vec3;
use worldrunner_render_plan::SceneTransform;

mod tree;
pub(crate) use tree::{Node, Tree};

/// World-space boxes for the allocated slots of one batch, one box per relief scale.
///
/// Visibility selection fills every live slot's boxes before it selects, and every write to
/// a slot clears them. The batch's relief scales depend only on its mesh and materials, so a
/// change of scales discards every entry. The tree groups the live slots by position and is
/// rebuilt after any slot's boxes change or a slot is added or removed.
#[derive(Debug, Default)]
pub(crate) struct SlotBounds {
    /// Distinct relief scales by bit pattern; each slot stores one box per scale.
    reliefs: Vec<u32>,
    boxes: Vec<[Vec3; 2]>,
    valid: Vec<bool>,
    tree: Tree,
    /// Whether `tree` no longer covers exactly the live slots and their boxes.
    stale: bool,
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

    /// Fills the boxes of every live slot that has none and regroups the slots when they
    /// changed. `live` lists the batch's allocated slots in order; `fill` computes one
    /// placement's boxes, one per relief scale.
    pub fn refresh<'a>(
        &mut self,
        reliefs: &[u32],
        live: impl Iterator<Item = (usize, &'a SceneTransform)>,
        mut fill: impl FnMut(&SceneTransform, &mut [[Vec3; 2]]),
    ) {
        if !self.stale && reliefs == self.reliefs {
            return;
        }
        let stride = reliefs.len();
        if reliefs != self.reliefs {
            self.reliefs = reliefs.to_vec();
            self.boxes.clear();
            self.valid.clear();
            self.stale = true;
        }
        let mut slots = Vec::new();
        for (position, transform) in live {
            if position >= self.valid.len() {
                self.valid.resize(position + 1, false);
                self.boxes.resize((position + 1) * stride, [Vec3::ZERO; 2]);
            }
            if !self.valid[position] {
                fill(
                    transform,
                    &mut self.boxes[position * stride..(position + 1) * stride],
                );
                self.valid[position] = true;
                self.stale = true;
            }
            slots.push(position);
        }
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

    /// The live slots grouped by position; current after `refresh`.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Sizes the level memo for the refreshed slots and empties it when `key` differs from
    /// the eye it was filled for.
    pub fn prepare_levels(&mut self, key: LevelKey) {
        self.levels.prepare(key, self.valid.len());
    }

    /// The level memo; current after `prepare_levels`.
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

/// What each slot draws at for one eye, filled during selection by whichever view needs it
/// first. A slot's entry depends only on its placement, its batch's mesh and materials and
/// the eye, so concurrent views may fill it in any order.
#[derive(Debug, Default)]
pub(crate) struct Levels {
    key: Option<LevelKey>,
    values: Vec<AtomicU8>,
}

/// A slot's memoized selection outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    /// Every fragment the placement could draw is faded out.
    Faded,
    /// Drawn at this level of detail.
    Drawn(usize),
}

const UNKNOWN: u8 = 0;
const FADED: u8 = 1;
const DRAWN: u8 = 2;

impl Levels {
    fn prepare(&mut self, key: LevelKey, slots: usize) {
        if self.key != Some(key) {
            self.key = Some(key);
            for value in &mut self.values {
                *value.get_mut() = UNKNOWN;
            }
        }
        if self.values.len() < slots {
            self.values.resize_with(slots, || AtomicU8::new(UNKNOWN));
        }
    }

    fn forget(&mut self, position: usize) {
        if let Some(value) = self.values.get_mut(position) {
            *value.get_mut() = UNKNOWN;
        }
    }

    /// The outcome of `position`, computed by `compute` unless an earlier view stored it.
    pub fn get_or_compute(&self, position: usize, compute: impl FnOnce() -> Level) -> Level {
        let value = &self.values[position];
        match value.load(Ordering::Relaxed) {
            UNKNOWN => {
                let level = compute();
                value.store(
                    match level {
                        Level::Faded => FADED,
                        Level::Drawn(lod) => u8::try_from(lod + usize::from(DRAWN))
                            .expect("levels of detail fit the memo"),
                    },
                    Ordering::Relaxed,
                );
                level
            }
            FADED => Level::Faded,
            stored => Level::Drawn(usize::from(stored - DRAWN)),
        }
    }
}
