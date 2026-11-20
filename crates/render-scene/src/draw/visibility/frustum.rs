//! A view volume as six planes, tested against world boxes.
use glam::{Mat4, Vec3};

use super::{Bounds, GROUP_MARGIN};

/// The six clip planes of a view matrix, with what a box test reads of each beyond its
/// normal: the normal's absolute value.
#[derive(Debug)]
pub(in crate::draw) struct Frustum {
    normals: [Vec3; 6],
    offsets: [f32; 6],
    reaches: [Vec3; 6],
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
            normals: planes.map(glam::Vec4::truncate),
            offsets: planes.map(|plane| plane.w),
            reaches: planes.map(|plane| plane.truncate().abs()),
        }
    }

    pub fn intersects(&self, bounds: Bounds) -> bool {
        let (center, half) = bounds.center_half();
        self.normals
            .iter()
            .zip(&self.offsets)
            .zip(&self.reaches)
            .all(|((normal, &offset), reach)| normal.dot(center) + offset + reach.dot(half) >= 0.0)
    }

    /// Whether the volume misses every box inside `bounds`, holds every box inside it, or
    /// neither. `Outside` when some plane leaves `bounds` behind it by more than the rounding
    /// of any per-box test, `Inside` when `bounds` lies in front of every plane by more than
    /// that rounding.
    pub fn region(&self, bounds: Bounds) -> Region {
        let (center, half) = bounds.center_half();
        let reach = center.abs() + half;
        let mut inside = true;
        for ((normal, &offset), plane_reach) in
            self.normals.iter().zip(&self.offsets).zip(&self.reaches)
        {
            let magnitude = plane_reach.dot(reach) + offset.abs();
            let margin = GROUP_MARGIN * magnitude;
            let front = normal.dot(center) + offset;
            let spread = plane_reach.dot(half);
            if front + spread < -margin {
                return Region::Outside;
            }
            inside &= front - spread > margin;
        }
        if inside {
            Region::Inside
        } else {
            Region::Straddles
        }
    }
}

#[cfg(test)]
impl Frustum {
    fn planes(&self) -> impl Iterator<Item = glam::Vec4> + '_ {
        self.normals
            .iter()
            .zip(&self.offsets)
            .map(|(normal, &offset)| normal.extend(offset))
    }

    /// `intersects` before it shared one centre and half extent between planes.
    pub fn intersects_reference(&self, bounds: Bounds) -> bool {
        self.planes().all(|plane| bounds.support(plane) >= 0.0)
    }

    /// Whether `intersects` is true for every box inside `bounds`: `bounds` lies in front of
    /// every plane by more than the rounding of any of those tests.
    pub fn contains(&self, bounds: Bounds) -> bool {
        let center = (bounds.min + bounds.max) * 0.5;
        let half = (bounds.max - bounds.min) * 0.5;
        let reach = center.abs() + half;
        self.planes().all(|plane| {
            let magnitude = plane.truncate().abs().dot(reach) + plane.w.abs();
            let nearest = plane.truncate().dot(center) + plane.w - plane.truncate().abs().dot(half);
            nearest > GROUP_MARGIN * magnitude
        })
    }

    /// Whether `intersects` is false for every box inside `bounds`: some plane leaves
    /// `bounds` behind it by more than the rounding of any of those tests.
    pub fn excludes(&self, bounds: Bounds) -> bool {
        let center = (bounds.min + bounds.max) * 0.5;
        let reach = center.abs() + (bounds.max - bounds.min) * 0.5;
        self.planes().any(|plane| {
            let magnitude = plane.truncate().abs().dot(reach) + plane.w.abs();
            bounds.support(plane) < -GROUP_MARGIN * magnitude
        })
    }
}
