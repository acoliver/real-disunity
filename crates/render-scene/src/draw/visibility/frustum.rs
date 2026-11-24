//! A view volume as six planes, tested against world boxes.
use glam::{Mat4, Vec3};

use super::{Bounds, GROUP_MARGIN};

const PLANES: usize = 6;

/// One vector per plane, kept as one list per axis so that a box test works through all six
/// planes at once.
#[derive(Clone, Copy, Debug)]
struct Lanes {
    x: [f32; PLANES],
    y: [f32; PLANES],
    z: [f32; PLANES],
}

impl Lanes {
    fn new(vectors: [Vec3; PLANES]) -> Self {
        Self {
            x: vectors.map(|vector| vector.x),
            y: vectors.map(|vector| vector.y),
            z: vectors.map(|vector| vector.z),
        }
    }

    /// The dot product of every plane's vector with `vector`, each as `Vec3::dot` computes it.
    fn dots(&self, vector: Vec3) -> [f32; PLANES] {
        std::array::from_fn(|plane| {
            self.x[plane] * vector.x + self.y[plane] * vector.y + self.z[plane] * vector.z
        })
    }
}

/// The six clip planes of a view matrix, with what a box test reads of each beyond its
/// normal: the normal's absolute value.
#[derive(Debug)]
pub(in crate::draw) struct Frustum {
    normals: Lanes,
    offsets: [f32; PLANES],
    reaches: Lanes,
}

/// Where a group's box lies against a view volume, as far as the margins of group culling
/// allow it to be said for every box inside the group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::draw) enum Region {
    /// No box inside the group intersects the volume.
    Outside,
    /// Every box inside the group intersects the volume.
    Inside,
    /// Boxes inside the group need their own test.
    Straddles,
}

impl Frustum {
    pub fn new(matrix: Mat4) -> Self {
        let rows = matrix.transpose();
        let planes = [
            rows.w_axis + rows.x_axis,
            rows.w_axis - rows.x_axis,
            rows.w_axis + rows.y_axis,
            rows.w_axis - rows.y_axis,
            rows.z_axis,
            rows.w_axis - rows.z_axis,
        ];
        Self {
            normals: Lanes::new(planes.map(glam::Vec4::truncate)),
            offsets: planes.map(|plane| plane.w),
            reaches: Lanes::new(planes.map(|plane| plane.truncate().abs())),
        }
    }

    #[inline]
    pub fn intersects(&self, bounds: Bounds) -> bool {
        let (center, half) = bounds.center_half();
        let (fronts, spreads) = (self.normals.dots(center), self.reaches.dots(half));
        // Counting the planes it lies in front of tests all six without a branch for each.
        let in_front: usize = (0..PLANES)
            .map(|plane| usize::from(fronts[plane] + self.offsets[plane] + spreads[plane] >= 0.0))
            .sum();
        in_front == PLANES
    }

    /// Whether the volume misses every box inside `bounds`, holds every box inside it, or
    /// neither. `Outside` when some plane leaves `bounds` behind it by more than the rounding
    /// of any per-box test, `Inside` when `bounds` lies in front of every plane by more than
    /// that rounding.
    pub fn region(&self, bounds: Bounds) -> Region {
        let (center, half) = bounds.center_half();
        let reach = center.abs() + half;
        let fronts = self.normals.dots(center);
        let (spreads, magnitudes) = (self.reaches.dots(half), self.reaches.dots(reach));
        let (mut outside, mut inside) = (false, true);
        for plane in 0..PLANES {
            let offset = self.offsets[plane];
            let margin = GROUP_MARGIN * (magnitudes[plane] + offset.abs());
            let front = fronts[plane] + offset;
            outside |= front + spreads[plane] < -margin;
            inside &= front - spreads[plane] > margin;
        }
        if outside {
            Region::Outside
        } else if inside {
            Region::Inside
        } else {
            Region::Straddles
        }
    }
}

#[cfg(test)]
pub(super) mod reference;
