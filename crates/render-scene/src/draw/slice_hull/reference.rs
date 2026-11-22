//! The footprint tests of a hull as plain loops over its points, the oracles the prepared tests
//! and the group regions are checked against.
use glam::{Mat4, Vec2};

use super::{Bounds, GROUP_MARGIN, SliceHull};

fn extent(hull: &SliceHull) -> (Vec2, Vec2) {
    hull.points.iter().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(low, high), &point| (low.min(point), high.max(point)),
    )
}

fn edges(hull: &SliceHull) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    let count = hull.points.len();
    (0..count).map(move |index| (hull.points[index], hull.points[(index + 1) % count]))
}

/// Whether the light-space footprint of world `bounds` overlaps `hull`.
pub(in crate::draw) fn overlaps(hull: &SliceHull, bounds: Bounds, matrix: Mat4) -> bool {
    let light = bounds.transformed(matrix);
    let (low, high) = (light.min.truncate(), light.max.truncate());
    let (hull_low, hull_high) = extent(hull);
    if hull_high.x < low.x || hull_low.x > high.x || hull_high.y < low.y || hull_low.y > high.y {
        return false;
    }
    let corners = [
        low,
        Vec2::new(high.x, low.y),
        high,
        Vec2::new(low.x, high.y),
    ];
    !edges(hull).any(|(a, b)| {
        corners
            .iter()
            .all(|&corner| (b - a).perp_dot(corner - a) < 0.0)
    })
}

/// Whether `overlaps` is false for every box inside `bounds`, with margins that cover the
/// rounding of those tests.
pub(in crate::draw) fn excludes(hull: &SliceHull, bounds: Bounds, matrix: Mat4) -> bool {
    let light = bounds.transformed_enclosing(matrix);
    let (low, high) = (light.min.truncate(), light.max.truncate());
    let (hull_low, hull_high) = extent(hull);
    if hull_high.x < low.x || hull_low.x > high.x || hull_high.y < low.y || hull_low.y > high.y {
        return true;
    }
    let corners = [
        low,
        Vec2::new(high.x, low.y),
        high,
        Vec2::new(low.x, high.y),
    ];
    let reach = low.abs().max(high.abs());
    edges(hull).any(|(a, b)| {
        let edge = b - a;
        let magnitude = edge.x.abs() * (reach.y + a.y.abs()) + edge.y.abs() * (reach.x + a.x.abs());
        corners
            .iter()
            .all(|&corner| edge.perp_dot(corner - a) < -GROUP_MARGIN * magnitude)
    })
}

/// Whether `overlaps` is true for every box inside `bounds`: the enclosing light-space
/// rectangle lies inside every edge by more than the rounding of those tests.
pub(in crate::draw) fn covers(hull: &SliceHull, bounds: Bounds, matrix: Mat4) -> bool {
    if hull.points.len() < 3 {
        return false;
    }
    let light = bounds.transformed_enclosing(matrix);
    let (low, high) = (light.min.truncate(), light.max.truncate());
    let corners = [
        low,
        Vec2::new(high.x, low.y),
        high,
        Vec2::new(low.x, high.y),
    ];
    let reach = low.abs().max(high.abs());
    edges(hull).all(|(a, b)| {
        let edge = b - a;
        let magnitude = edge.x.abs() * (reach.y + a.y.abs()) + edge.y.abs() * (reach.x + a.x.abs());
        corners
            .iter()
            .all(|&corner| edge.perp_dot(corner - a) > GROUP_MARGIN * magnitude)
    })
}
