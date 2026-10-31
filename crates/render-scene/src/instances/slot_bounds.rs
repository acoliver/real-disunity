//! Per-slot storage for view-independent placement bounds.

use glam::Vec3;

/// World-space boxes for the allocated slots of one batch, `stride` boxes per slot.
///
/// Visibility selection fills a slot's boxes the first time it needs them, and every
/// write to the slot clears them. The batch's box count per slot depends only on its
/// mesh and material, so a change of stride discards every entry.
#[derive(Debug, Default)]
pub(crate) struct SlotBounds {
    stride: usize,
    boxes: Vec<[Vec3; 2]>,
    valid: Vec<bool>,
}

impl SlotBounds {
    pub fn invalidate(&mut self, position: usize) {
        if let Some(valid) = self.valid.get_mut(position) {
            *valid = false;
        }
    }

    /// The boxes stored for `position`, computed by `fill` when the slot has none.
    pub fn get_or_fill(
        &mut self,
        position: usize,
        stride: usize,
        fill: impl FnOnce(&mut [[Vec3; 2]]),
    ) -> &[[Vec3; 2]] {
        if stride != self.stride {
            self.stride = stride;
            self.boxes.clear();
            self.valid.clear();
        }
        if position >= self.valid.len() {
            self.valid.resize(position + 1, false);
            self.boxes.resize((position + 1) * stride, [Vec3::ZERO; 2]);
        }
        let range = position * stride..(position + 1) * stride;
        if !self.valid[position] {
            fill(&mut self.boxes[range.clone()]);
            self.valid[position] = true;
        }
        &self.boxes[range]
    }

    #[cfg(test)]
    pub fn is_cached(&self, position: usize) -> bool {
        self.valid.get(position).copied().unwrap_or(false)
    }
}
