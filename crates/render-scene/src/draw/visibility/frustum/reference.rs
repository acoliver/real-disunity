//! The view-volume tests as plain per-plane loops, the oracles the prepared tests and the group
//! regions are checked against.
use glam::{Vec3, Vec4};

use super::{Bounds, Frustum, GROUP_MARGIN};

fn planes(frustum: &Frustum) -> impl Iterator<Item = Vec4> + '_ {
    (0..frustum.offsets.len()).map(|plane| {
        let normals = &frustum.normals;
        Vec3::new(normals.x[plane], normals.y[plane], normals.z[plane])
            .extend(frustum.offsets[plane])
    })
}

/// The distance in front of `plane` of the corner of `bounds` that lies furthest in front.
fn support(bounds: Bounds, plane: Vec4) -> f32 {
    let (center, half) = bounds.center_half();
    plane.truncate().dot(center) + plane.w + plane.truncate().abs().dot(half)
}

/// Whether `bounds` lies in front of every plane at its furthest corner.
pub(in crate::draw) fn intersects(frustum: &Frustum, bounds: Bounds) -> bool {
    planes(frustum).all(|plane| support(bounds, plane) >= 0.0)
}

/// Whether `intersects` is true for every box inside `bounds`: `bounds` lies in front of every
/// plane by more than the rounding of any of those tests.
pub(in crate::draw) fn contains(frustum: &Frustum, bounds: Bounds) -> bool {
    let center = (bounds.min + bounds.max) * 0.5;
    let half = (bounds.max - bounds.min) * 0.5;
    let reach = center.abs() + half;
    planes(frustum).all(|plane| {
        let magnitude = plane.truncate().abs().dot(reach) + plane.w.abs();
        let nearest = plane.truncate().dot(center) + plane.w - plane.truncate().abs().dot(half);
        nearest > GROUP_MARGIN * magnitude
    })
}

/// Whether `intersects` is false for every box inside `bounds`: some plane leaves `bounds`
/// behind it by more than the rounding of any of those tests.
pub(in crate::draw) fn excludes(frustum: &Frustum, bounds: Bounds) -> bool {
    let center = (bounds.min + bounds.max) * 0.5;
    let reach = center.abs() + (bounds.max - bounds.min) * 0.5;
    planes(frustum).any(|plane| {
        let magnitude = plane.truncate().abs().dot(reach) + plane.w.abs();
        support(bounds, plane) < -GROUP_MARGIN * magnitude
    })
}
