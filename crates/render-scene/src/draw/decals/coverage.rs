//! A coarse world-plane grid of which selected decal volumes can contain a point.
//!
//! Every fragment that projects decals otherwise transforms its position into each
//! selected volume to learn that it lies in none of them. The grid lets the shader visit
//! only the volumes whose bounds reach the fragment's cell, in the same ascending order.
//! A skipped volume is one the fragment lies outside of, which the volume loop would
//! have skipped before writing anything, so the projected surface is unchanged.

use super::super::visibility::Bounds;

/// Cells along each world-plane axis.
const AXIS_CELLS: u16 = 16;
pub(super) const CELLS: usize = AXIS_CELLS as usize;
/// Grid frame (origin and inverse cell size) followed by one volume mask per cell.
pub(super) const BYTES: usize = 16 + CELLS * CELLS * 4;

/// Absolute widening, in metres, applied to every volume's bounds before marking cells.
const MARGIN_METERS: f32 = 0.01;
/// Widening proportional to coordinate magnitude, covering f32 rounding at large offsets.
const MARGIN_PER_METER: f32 = 1.0e-4;
/// Smallest cell edge, so a grid around a single thin volume keeps a finite scale.
const MIN_CELL_METERS: f32 = 1.0e-3;

#[cfg(test)]
thread_local! {
    /// Test reference: every point visits every selected volume, as before the grid.
    pub(crate) static EVERY_VOLUME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Packs the grid for `bounds`, where bit `i` of a cell marks selected volume `i`.
pub(super) fn bytes(bounds: &[Bounds]) -> Vec<u8> {
    assert!(bounds.len() <= 32, "one mask bit per selected volume");
    let mut out = Vec::with_capacity(BYTES);
    #[cfg(test)]
    if EVERY_VOLUME.get() && !bounds.is_empty() {
        let all = u32::MAX >> (32 - bounds.len());
        for value in [-1.0e9_f32, -1.0e9, 8.0e-9, 8.0e-9] {
            out.extend(value.to_le_bytes());
        }
        for _ in 0..CELLS * CELLS {
            out.extend(all.to_le_bytes());
        }
        return out;
    }
    let Some(widened) = widen(bounds) else {
        out.resize(BYTES, 0);
        return out;
    };
    let origin = widened.iter().fold([f32::INFINITY; 2], |min, b| {
        [min[0].min(b[0]), min[1].min(b[1])]
    });
    let corner = widened.iter().fold([f32::NEG_INFINITY; 2], |max, b| {
        [max[0].max(b[2]), max[1].max(b[3])]
    });
    let inverse = [0, 1].map(|axis| {
        let edge = (corner[axis] - origin[axis]) / f32::from(AXIS_CELLS);
        1.0 / edge.max(MIN_CELL_METERS)
    });
    let mut masks = [0u32; CELLS * CELLS];
    for (index, b) in widened.iter().enumerate() {
        let low = [0, 1].map(|axis| cell(b[axis], origin[axis], inverse[axis]));
        let high = [0, 1].map(|axis| cell(b[axis + 2], origin[axis], inverse[axis]));
        for y in low[1]..=high[1] {
            for x in low[0]..=high[0] {
                masks[y * CELLS + x] |= 1 << index;
            }
        }
    }
    for value in [origin[0], origin[1], inverse[0], inverse[1]] {
        out.extend(value.to_le_bytes());
    }
    for mask in masks {
        out.extend(mask.to_le_bytes());
    }
    out
}

/// Each volume's world-plane bounds as `[min x, min y, max x, max y]`, widened.
fn widen(bounds: &[Bounds]) -> Option<Vec<[f32; 4]>> {
    if bounds.is_empty() {
        return None;
    }
    Some(
        bounds
            .iter()
            .map(|b| {
                let reach = b.min.abs().max(b.max.abs());
                let margin = MARGIN_METERS + MARGIN_PER_METER * reach.x.max(reach.y);
                [
                    b.min.x - margin,
                    b.min.y - margin,
                    b.max.x + margin,
                    b.max.y + margin,
                ]
            })
            .collect(),
    )
}

/// The cell a world coordinate falls in, clamped to the grid, by the shader's formula.
fn cell(value: f32, origin: f32, inverse: f32) -> usize {
    let scaled = ((value - origin) * inverse).floor();
    (0..AXIS_CELLS)
        .rev()
        .find(|&cell| scaled >= f32::from(cell))
        .map_or(0, usize::from)
}

#[cfg(test)]
mod tests;
