//! Light-space footprint of the camera slice a shadow cascade serves.
//!
//! A cascade map is a stable square around the bounding sphere of its camera slice, so the
//! square stays put while the camera turns. Receivers sample the map only where they lie, and
//! every receiver lies inside the slice, so only the slice's projection along the light is ever
//! read. A caster whose light-space footprint misses that projection cannot darken a sampled
//! texel, and caster selection skips it. The map matrix is unchanged, so edges do not swim.
use std::cell::{Cell, RefCell};

use glam::{Mat4, Vec2, Vec3};

use super::SceneGpu;
use super::visibility::Bounds;

/// Convex polygon in a cascade's light clip-space XY, counter-clockwise.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SliceHull {
    points: Vec<Vec2>,
}

impl SliceHull {
    /// Hull of world points projected by `matrix`, grown by `margin` clip units along x and y.
    pub fn new(corners: &[Vec3], matrix: Mat4, margin: f32) -> Self {
        let projected = corners
            .iter()
            .map(|&corner| matrix.project_point3(corner).truncate());
        Self {
            points: convex_hull(grow(projected, margin)),
        }
    }

    /// This hull grown by `margin` clip units along x and y.
    pub fn grown(&self, margin: f32) -> Self {
        Self {
            points: convex_hull(grow(self.points.iter().copied(), margin)),
        }
    }

    /// Whether every point of `other` lies inside this hull.
    pub fn contains(&self, other: &Self) -> bool {
        other.points.iter().all(|&point| {
            self.edges()
                .all(|(a, b)| (b - a).perp_dot(point - a) >= 0.0)
        })
    }

    /// Whether the light-space footprint of world `bounds` overlaps this hull.
    pub(super) fn overlaps(&self, bounds: Bounds, matrix: Mat4) -> bool {
        let light = bounds.transformed(matrix);
        let (low, high) = (light.min.truncate(), light.max.truncate());
        let (hull_low, hull_high) = self.extent();
        if hull_high.x < low.x || hull_low.x > high.x || hull_high.y < low.y || hull_low.y > high.y
        {
            return false;
        }
        let corners = [
            low,
            Vec2::new(high.x, low.y),
            high,
            Vec2::new(low.x, high.y),
        ];
        // Separating axes: the rectangle's were tested above, the hull's edges remain.
        !self.edges().any(|(a, b)| {
            corners
                .iter()
                .all(|&corner| (b - a).perp_dot(corner - a) < 0.0)
        })
    }

    fn extent(&self) -> (Vec2, Vec2) {
        self.points.iter().fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(low, high), &point| (low.min(point), high.max(point)),
        )
    }

    fn edges(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        let count = self.points.len();
        (0..count).map(move |index| (self.points[index], self.points[(index + 1) % count]))
    }
}

fn grow(points: impl Iterator<Item = Vec2>, margin: f32) -> Vec<Vec2> {
    points
        .flat_map(|point| {
            [
                Vec2::new(-margin, -margin),
                Vec2::new(margin, -margin),
                Vec2::new(margin, margin),
                Vec2::new(-margin, margin),
            ]
            .map(|offset| point + offset)
        })
        .collect()
}

/// Andrew's monotone chain; counter-clockwise without collinear points.
fn convex_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut hull: Vec<Vec2> = Vec::with_capacity(points.len() * 2);
    for pass in [false, true] {
        let floor = hull.len();
        let ordered: Box<dyn Iterator<Item = &Vec2>> = if pass {
            Box::new(points.iter().rev())
        } else {
            Box::new(points.iter())
        };
        for &point in ordered {
            while hull.len() >= floor + 2
                && (hull[hull.len() - 1] - hull[hull.len() - 2])
                    .perp_dot(point - hull[hull.len() - 2])
                    <= 0.0
            {
                hull.pop();
            }
            hull.push(point);
        }
        hull.pop();
    }
    hull
}

/// The slice hull a cascade's selection culls against, for one map matrix.
#[derive(Debug, Default)]
pub(super) struct CascadeSlice {
    current: RefCell<Option<([u32; 16], SliceHull)>>,
    generation: Cell<u64>,
}

impl CascadeSlice {
    /// The hull for `matrix`, with its generation; none when the view has no slice for it.
    pub fn for_matrix(&self, matrix: Mat4) -> Option<(SliceHull, u64)> {
        let bits = matrix.to_cols_array().map(f32::to_bits);
        self.current
            .borrow()
            .as_ref()
            .filter(|(stored, _)| *stored == bits)
            .map(|(_, hull)| (hull.clone(), self.generation.get()))
    }
}

#[cfg(test)]
thread_local! {
    /// Draws every caster in each cascade map, for comparison against slice culling. Refresh
    /// decisions still track the slices, so both paths retain and refresh the same maps.
    pub(crate) static UNCULLED: Cell<bool> = const { Cell::new(false) };
}

impl SceneGpu {
    /// Sets the hull cascade `index` culls casters against while drawn with `matrix`, and
    /// returns it. The stored hull carries `slack` beyond `required`, and is kept while it
    /// still contains `required` for the same matrix, so small camera motion and projection
    /// jitter reuse the cascade's selection. Without `required` the cascade draws every
    /// caster in its map.
    pub(crate) fn cascade_slice(
        &self,
        index: usize,
        matrix: Mat4,
        required: Option<&SliceHull>,
        slack: f32,
    ) -> Option<SliceHull> {
        let slice = &self.views[index + 1].slice;
        let Some(required) = required else {
            *slice.current.borrow_mut() = None;
            return None;
        };
        if let Some((stored, _)) = slice
            .for_matrix(matrix)
            .filter(|(stored, _)| stored.contains(required))
        {
            return Some(stored);
        }
        let hull = required.grown(slack);
        *slice.current.borrow_mut() =
            Some((matrix.to_cols_array().map(f32::to_bits), hull.clone()));
        slice.generation.set(slice.generation.get() + 1);
        Some(hull)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(half: f32) -> SliceHull {
        SliceHull::new(
            &[
                Vec3::new(-half, -half, 0.0),
                Vec3::new(half, -half, 0.0),
                Vec3::new(half, half, 0.0),
                Vec3::new(-half, half, 0.0),
                Vec3::ZERO,
            ],
            Mat4::IDENTITY,
            0.0,
        )
    }

    fn bounds(low: [f32; 2], high: [f32; 2]) -> Bounds {
        Bounds {
            min: Vec3::new(low[0], low[1], 0.0),
            max: Vec3::new(high[0], high[1], 1.0),
        }
    }

    #[test]
    fn hull_drops_interior_points_and_grows_by_the_margin() {
        let hull = square(0.5);
        assert_eq!(hull.points.len(), 4);
        assert!(square(0.6).contains(&hull));
        assert!(!hull.contains(&square(0.6)));
        assert!(hull.grown(0.1).contains(&square(0.6)));
    }

    #[test]
    fn footprints_overlap_unless_an_axis_separates_them() {
        // A diamond: its edges, not its bounding box, separate the corner rectangles.
        let diamond = SliceHull::new(
            &[
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),
            ],
            Mat4::IDENTITY,
            0.0,
        );
        assert!(diamond.overlaps(bounds([-0.1, -0.1], [0.1, 0.1]), Mat4::IDENTITY));
        assert!(diamond.overlaps(bounds([0.4, 0.4], [0.9, 0.9]), Mat4::IDENTITY));
        assert!(!diamond.overlaps(bounds([0.6, 0.6], [0.9, 0.9]), Mat4::IDENTITY));
        assert!(!diamond.overlaps(bounds([1.1, -0.1], [1.5, 0.1]), Mat4::IDENTITY));
        let shifted = Mat4::from_translation(Vec3::new(-0.7, -0.7, 0.0));
        assert!(diamond.overlaps(bounds([0.6, 0.6], [0.9, 0.9]), shifted));
    }
}
