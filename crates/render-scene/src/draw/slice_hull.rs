//! Light-space footprint of the camera slice a shadow cascade serves.
//!
//! A cascade map is a stable square around the bounding sphere of its camera slice, so the
//! square stays put while the camera turns. Receivers sample the map only where they lie, and
//! every receiver lies inside the slice, so only the slice's projection along the light is ever
//! read. A caster whose light-space footprint misses that projection cannot darken a sampled
//! texel, and caster selection skips it. The map matrix is unchanged, so edges do not swim.
use std::cell::{Cell, RefCell};

use glam::{Mat4, Vec2, Vec3};

use super::visibility::{Bounds, Frustum, GROUP_MARGIN, Region};
use super::{DrawCount, SceneGpu};
use worldrunner_render_program::runtime::{PassCommands, PreparedPipeline};

/// Convex polygon in a cascade's light clip-space XY, counter-clockwise.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SliceHull {
    points: Vec<Vec2>,
    /// Each point with the vector to the next, which every box test reads.
    edges: Vec<(Vec2, Vec2)>,
    /// Lowest and highest coordinates of `points`.
    extent: (Vec2, Vec2),
}

/// A cascade map's matrix with the absolute values its box tests read from every column, so
/// that testing many boxes against one map computes them once.
#[derive(Clone, Copy, Debug)]
pub(super) struct LightMap {
    matrix: Mat4,
    reaches: [Vec3; 3],
    translation_reach: Vec3,
}

impl LightMap {
    pub fn new(matrix: Mat4) -> Self {
        Self {
            matrix,
            reaches: [matrix.x_axis, matrix.y_axis, matrix.z_axis]
                .map(|axis| axis.truncate().abs()),
            translation_reach: matrix.w_axis.truncate().abs(),
        }
    }

    /// `bounds.transformed(matrix)`.
    pub(super) fn transformed(&self, bounds: Bounds) -> Bounds {
        let (center, half) = bounds.center_half();
        self.about(self.matrix.transform_point3(center), half)
    }

    fn about(&self, center: Vec3, half: Vec3) -> Bounds {
        let extent = self.reaches[0] * half.x + self.reaches[1] * half.y + self.reaches[2] * half.z;
        Bounds {
            min: center - extent,
            max: center + extent,
        }
    }

    /// `bounds.transformed_enclosing(matrix)`.
    pub(super) fn enclosing(&self, bounds: Bounds) -> Bounds {
        let (center, half) = bounds.center_half();
        let reach = center.abs() + half;
        let magnitude = self.reaches[0] * reach.x
            + self.reaches[1] * reach.y
            + self.reaches[2] * reach.z
            + self.translation_reach;
        self.about(self.matrix.transform_point3(center), half)
            .expanded(magnitude * GROUP_MARGIN)
    }
}

impl SliceHull {
    /// Hull of world points projected by `matrix`, grown by `margin` clip units along x and y.
    pub fn new(corners: &[Vec3], matrix: Mat4, margin: f32) -> Self {
        let projected = corners
            .iter()
            .map(|&corner| matrix.project_point3(corner).truncate());
        Self::from_points(convex_hull(grow(projected, margin)))
    }

    fn from_points(points: Vec<Vec2>) -> Self {
        let count = points.len();
        let edges = (0..count)
            .map(|index| {
                let (a, b) = (points[index], points[(index + 1) % count]);
                (a, b - a)
            })
            .collect();
        let extent = points.iter().fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(low, high), &point| (low.min(point), high.max(point)),
        );
        Self {
            points,
            edges,
            extent,
        }
    }

    /// This hull grown by `margin` clip units along x and y.
    pub fn grown(&self, margin: f32) -> Self {
        Self::from_points(convex_hull(grow(self.points.iter().copied(), margin)))
    }

    /// Whether every point of `other` lies inside this hull.
    pub fn contains(&self, other: &Self) -> bool {
        other.points.iter().all(|&point| {
            self.edges
                .iter()
                .all(|&(a, edge)| edge.perp_dot(point - a) >= 0.0)
        })
    }

    /// Whether the light-space footprint of world `bounds` overlaps this hull, for a map whose
    /// matrix columns are already prepared.
    pub(super) fn overlaps(&self, bounds: Bounds, light: &LightMap) -> bool {
        let light = light.transformed(bounds);
        let (low, high) = (light.min.truncate(), light.max.truncate());
        let (hull_low, hull_high) = self.extent;
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
        !self.edges.iter().any(|&(a, edge)| {
            corners
                .iter()
                .all(|&corner| edge.perp_dot(corner - a) < 0.0)
        })
    }

    /// Whether `overlaps` is false for every box inside `bounds`, true for every box inside
    /// it, or neither, with margins that cover the rounding of those tests, so a cascade can
    /// reject a group of casters from its box, or skip testing the casters of one.
    pub(super) fn region(&self, bounds: Bounds, light: &LightMap) -> Region {
        let light = light.enclosing(bounds);
        let (low, high) = (light.min.truncate(), light.max.truncate());
        let (hull_low, hull_high) = self.extent;
        if hull_high.x < low.x || hull_low.x > high.x || hull_high.y < low.y || hull_low.y > high.y
        {
            return Region::Outside;
        }
        let corners = [
            low,
            Vec2::new(high.x, low.y),
            high,
            Vec2::new(low.x, high.y),
        ];
        let reach = low.abs().max(high.abs());
        let mut covered = self.points.len() >= 3;
        for &(a, edge) in &self.edges {
            let magnitude =
                edge.x.abs() * (reach.y + a.y.abs()) + edge.y.abs() * (reach.x + a.x.abs());
            let margin = GROUP_MARGIN * magnitude;
            let (mut behind, mut ahead) = (true, true);
            for corner in corners {
                let side = edge.perp_dot(corner - a);
                behind &= side < -margin;
                ahead &= side > margin;
            }
            if behind {
                return Region::Outside;
            }
            covered &= ahead;
        }
        if covered {
            Region::Inside
        } else {
            Region::Straddles
        }
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

    /// The generation `for_matrix` returns with its hull, without copying the hull.
    pub fn generation_for(&self, matrix: Mat4) -> Option<u64> {
        let bits = matrix.to_cols_array().map(f32::to_bits);
        self.current
            .borrow()
            .as_ref()
            .filter(|(stored, _)| *stored == bits)
            .map(|_| self.generation.get())
    }
}

#[cfg(test)]
pub(super) mod reference;

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

impl SceneGpu {
    /// What shadow cascade `index` draws of the terrain when its map is drawn with `matrix`:
    /// `None` when the cascade's volume misses the terrain altogether, otherwise the test of one
    /// streaming chunk's world bounds, `[min, max]`. Chunks outside the cascade's volume or its
    /// slice hull cannot shadow a sampled texel and are skipped.
    pub(crate) fn terrain_caster_selection(
        &self,
        index: u32,
        matrix: Mat4,
    ) -> Option<impl Fn([Vec3; 2]) -> bool> {
        let frustum = Frustum::new(matrix);
        self.terrain
            .as_ref()
            .filter(|t| frustum.intersects(t.bounds))?;
        let slice = self.views[index as usize + 1].slice.for_matrix(matrix);
        let light = LightMap::new(matrix);
        Some(move |[min, max]: [Vec3; 2]| {
            #[cfg(test)]
            if UNCULLED.get() {
                return true;
            }
            let bounds = Bounds { min, max };
            frustum.intersects(bounds)
                && slice
                    .as_ref()
                    .is_none_or(|(hull, _)| hull.overlaps(bounds, &light))
        })
    }

    /// Draws the terrain into shadow cascade `index` through the caster's prepared pipeline, with
    /// the cascade's light frame group lent at set 0.
    pub(crate) fn draw_terrain_depth<P: PassCommands>(
        &self,
        pass: &mut P,
        (pipeline, frame): (&PreparedPipeline, &wgpu::BindGroup),
        (index, matrix): (u32, Mat4),
    ) -> DrawCount {
        let Some(keep) = self.terrain_caster_selection(index, matrix) else {
            return DrawCount::default();
        };
        let terrain = self.terrain.as_ref().expect("a selection has terrain");
        pipeline.bind(pass);
        pass.set_bind_group(0, frame);
        let Some(streaming) = &terrain.streaming else {
            return Self::draw_terrain_mesh(pass, pipeline, terrain);
        };
        if let Some(count) = streaming.draw_caster_chunks(pass, pipeline, (terrain, index), &keep) {
            return count;
        }
        streaming.draw_prepared_where(pass, pipeline, terrain, keep)
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
    fn the_generation_alone_is_that_of_the_hull_for_the_same_matrix_only() {
        let slice = CascadeSlice::default();
        let matrix = Mat4::from_scale(Vec3::splat(0.5));
        assert_eq!(slice.generation_for(matrix), None);
        *slice.current.borrow_mut() = Some((matrix.to_cols_array().map(f32::to_bits), square(0.5)));
        slice.generation.set(7);
        for probe in [matrix, Mat4::IDENTITY] {
            assert_eq!(
                slice.generation_for(probe),
                slice.for_matrix(probe).map(|(_, generation)| generation)
            );
        }
        assert_eq!(slice.generation_for(matrix), Some(7));
        assert_eq!(slice.generation_for(Mat4::IDENTITY), None);
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
        let overlaps = |bounds, matrix| diamond.overlaps(bounds, &LightMap::new(matrix));
        assert!(overlaps(bounds([-0.1, -0.1], [0.1, 0.1]), Mat4::IDENTITY));
        assert!(overlaps(bounds([0.4, 0.4], [0.9, 0.9]), Mat4::IDENTITY));
        assert!(!overlaps(bounds([0.6, 0.6], [0.9, 0.9]), Mat4::IDENTITY));
        assert!(!overlaps(bounds([1.1, -0.1], [1.5, 0.1]), Mat4::IDENTITY));
        let shifted = Mat4::from_translation(Vec3::new(-0.7, -0.7, 0.0));
        assert!(overlaps(bounds([0.6, 0.6], [0.9, 0.9]), shifted));
    }

    #[test]
    fn generation_for_answers_exactly_when_for_matrix_does() {
        let slice = CascadeSlice::default();
        let matrix = Mat4::from_translation(Vec3::new(0.25, -0.5, 2.0));
        let other = Mat4::from_translation(Vec3::new(0.25, -0.5, 2.5));
        let agree = |slice: &CascadeSlice, matrix: Mat4| {
            assert_eq!(
                slice.generation_for(matrix),
                slice.for_matrix(matrix).map(|(_, generation)| generation)
            );
            slice.generation_for(matrix)
        };
        assert_eq!(agree(&slice, matrix), None);
        *slice.current.borrow_mut() = Some((matrix.to_cols_array().map(f32::to_bits), square(0.5)));
        slice.generation.set(3);
        assert_eq!(agree(&slice, matrix), Some(3));
        assert_eq!(agree(&slice, other), None);
        // One bit of one element is a different matrix.
        let mut nudged = matrix.to_cols_array();
        nudged[12] = nudged[12].next_up();
        assert_eq!(agree(&slice, Mat4::from_cols_array(&nudged)), None);
        slice.generation.set(4);
        assert_eq!(agree(&slice, matrix), Some(4));
    }
}
