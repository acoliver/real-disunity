use std::ops::Range;

use glam::{Mat4, Vec3, Vec4};
use worldrunner_render_plan::{BillboardPolicy, SceneMesh, SceneTransform};

use crate::geometry::{millimeters_to_meters, model_matrix};
use crate::instances::{BatchKey, Instances};
pub(super) use casters::{caster_matrix, caster_placement};
pub(super) use select::with_placements;

mod casters;
mod fade;
mod placement;
mod select;

/// Object placement counts for one recorded view, excluding terrain.
/// Hidden live placements count as culled; removed placements are not considered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewStats {
    /// Number of live placements examined.
    pub considered: usize,
    /// Hidden, outside the view volume, or guaranteed to reject every faded fragment.
    pub culled: usize,
    /// Submitted placements by zero-based LOD index, across all meshes.
    pub drawn_per_lod: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    pub(super) fn expanded(self, extent: Vec3) -> Self {
        Self {
            min: self.min - extent,
            max: self.max + extent,
        }
    }

    pub(super) fn transformed(self, matrix: Mat4) -> Self {
        let center = matrix.transform_point3((self.min + self.max) * 0.5);
        let half = (self.max - self.min) * 0.5;
        let extent = matrix.x_axis.truncate().abs() * half.x
            + matrix.y_axis.truncate().abs() * half.y
            + matrix.z_axis.truncate().abs() * half.z;
        Self {
            min: center - extent,
            max: center + extent,
        }
    }

    /// `transformed`, grown by a margin that covers its rounding for this box and for every
    /// box inside it. A point outside the result is outside the `transformed` box of every
    /// box this one contains, so group culling can reject a group from its box alone.
    pub(super) fn transformed_enclosing(self, matrix: Mat4) -> Self {
        let center = (self.min + self.max) * 0.5;
        let reach = center.abs() + (self.max - self.min) * 0.5;
        let magnitude = matrix.x_axis.truncate().abs() * reach.x
            + matrix.y_axis.truncate().abs() * reach.y
            + matrix.z_axis.truncate().abs() * reach.z
            + matrix.w_axis.truncate().abs();
        self.transformed(matrix).expanded(magnitude * GROUP_MARGIN)
    }

    fn support(self, plane: Vec4) -> f32 {
        let center = (self.min + self.max) * 0.5;
        let half = (self.max - self.min) * 0.5;
        plane.truncate().dot(center) + plane.w + plane.truncate().abs().dot(half)
    }
}

#[derive(Debug)]
pub(super) struct MeshSelection {
    pub(super) material_slots: Vec<Option<Vec<worldrunner_render_plan::SceneLodMaterialSlot>>>,
    bounds: Bounds,
    billboard_radius: Option<f32>,
    ceilings_squared: Vec<u128>,
    transitions: Vec<bool>,
}

impl MeshSelection {
    pub fn new(mesh: &SceneMesh) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut radius_squared = 0.0_f32;
        for vertex in mesh.lods().iter().flat_map(|lod| {
            crate::geometry::card_vertices(lod).expect("mesh upload validated card extents")
        }) {
            let point = millimeters_to_meters(vertex.position_millimeters);
            min = min.min(point);
            max = max.max(point);
            radius_squared = radius_squared.max(point.length_squared());
        }
        let billboard = mesh.lods().iter().any(|lod| {
            matches!(
                lod.billboard(),
                Some(BillboardPolicy::YawLocked | BillboardPolicy::Spherical)
            )
        });
        Self {
            transitions: mesh
                .lods()
                .iter()
                .enumerate()
                .map(|(level, lod)| level > 0 && lod.card().is_some())
                .collect(),
            material_slots: mesh
                .lods()
                .iter()
                .map(|lod| lod.material_slots().map(<[_]>::to_vec))
                .collect(),
            bounds: Bounds { min, max },
            billboard_radius: billboard.then(|| radius_squared.sqrt()),
            ceilings_squared: mesh
                .max_distances_millimeters()
                .iter()
                .map(|&value| u128::from(value) * u128::from(value))
                .collect(),
        }
    }

    pub fn lod(&self, distance_squared: u128, scale_millionths: Option<u32>) -> usize {
        self.ceilings_squared
            .iter()
            .position(|&ceiling| within_ceiling(distance_squared, ceiling, scale_millionths))
            .unwrap_or_else(|| self.ceilings_squared.len().saturating_sub(1))
    }

    pub(super) fn wind_bounds(
        &self,
        transform: &SceneTransform,
        extent: Vec3,
        curved: Option<Vec3>,
        shape: [f32; 2],
    ) -> Bounds {
        let [shaped_radius, capture_radius] = shape;
        let mut bounds = self.world_bounds(transform);
        let model = model_matrix(transform);
        let scale = model.x_axis.truncate().length();
        if shaped_radius > 0.0 {
            let center = model.w_axis.truncate();
            let extent = Vec3::splat(shaped_radius * scale * 1.000_01);
            bounds.min = bounds.min.min(center - extent);
            bounds.max = bounds.max.max(center + extent);
        }
        let Some(origin) = curved else {
            return bounds.expanded(extent);
        };
        // Each arc is bounded by twice its input length, including arbitrary
        // force directions. Projection and attachment together fit six radii.
        let radius = self
            .bounds
            .min
            .abs()
            .max(self.bounds.max.abs())
            .length()
            .max(self.billboard_radius.unwrap_or(0.0))
            .max(shaped_radius)
            + capture_radius;
        bounds.expanded(Vec3::splat((6.0 * radius + 7.0 * origin.length()) * scale))
    }

    /// Reference for the cached path in `placement`; tests compare the two bit for bit.
    #[cfg(test)]
    fn material_bounds(
        &self,
        transform: &SceneTransform,
        wind: Vec3,
        curved: Option<Vec3>,
        impostor: [f32; 3],
        relief_stretch: f32,
    ) -> Bounds {
        self.wind_bounds(transform, wind, curved, [impostor[0], impostor[2]])
            .expanded(Vec3::splat(
                impostor[2] * model_matrix(transform).x_axis.truncate().length(),
            ))
            .expanded(impostor_extent(
                [0.0, impostor[1]],
                transform,
                relief_stretch,
            ))
    }

    #[cfg(test)]
    fn placement_bounds(
        &self,
        instances: &Instances,
        transform: &SceneTransform,
        material: usize,
        stretch: f32,
    ) -> Bounds {
        let bounds_for = |material: usize| {
            self.material_bounds(
                transform,
                instances.wind_extents[material],
                instances.curved_wind[material],
                instances.impostor_extents[material],
                stretch,
            )
        };
        let mut bounds = bounds_for(material);
        for slot in self.material_slots.iter().flatten().flatten() {
            let other = bounds_for(slot.material().raw() as usize);
            bounds.min = bounds.min.min(other.min);
            bounds.max = bounds.max.max(other.max);
        }
        bounds
    }

    pub fn world_bounds(&self, transform: &SceneTransform) -> Bounds {
        let model = model_matrix(transform);
        let bounds = if let Some(radius) = self.billboard_radius {
            let center = model.w_axis.truncate();
            let extent = Vec3::splat(radius * model.x_axis.truncate().length());
            Bounds {
                min: center - extent,
                max: center + extent,
            }
        } else {
            self.bounds.transformed(model)
        };
        // Cover float rounding in the model and clip transforms, including flat meshes.
        let pad = bounds.min.abs().max(bounds.max.abs()).max(Vec3::ONE) * 0.000_01;
        Bounds {
            min: bounds.min - pad,
            max: bounds.max + pad,
        }
    }
}

fn within_ceiling(distance: u128, ceiling: u128, scale: Option<u32>) -> bool {
    const UNIT_SQUARED: u128 = 1_000_000_000_000;
    let Some(scale) = scale else {
        return distance <= ceiling;
    };
    let scale_squared = u128::from(scale) * u128::from(scale);
    // Compare the rational quotients first. Only the remainders need cross
    // multiplication, bounded by 10^28 even when the squared distances fill u128.
    match (distance / scale_squared).cmp(&(ceiling / UNIT_SQUARED)) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => {
            (distance % scale_squared) * UNIT_SQUARED <= (ceiling % UNIT_SQUARED) * scale_squared
        }
    }
}

#[cfg(test)]
fn impostor_extent(extents: [f32; 2], transform: &SceneTransform, stretch: f32) -> Vec3 {
    let scale = model_matrix(transform).x_axis.truncate().length();
    Vec3::splat(extents[0] * scale + extents[1] * stretch)
}

// The longest ray per unit axial depth occurs at a frustum corner.
pub(super) fn relief_ray_stretch(matrix: Mat4) -> f32 {
    let rows = matrix.transpose();
    if rows.w_axis.truncate().length_squared() == 0.0 {
        return 1.0;
    }
    let axis = rows.w_axis.truncate().normalize();
    let inverse = matrix.inverse();
    let mut stretch = 1.0_f32;
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            let near = inverse.project_point3(Vec3::new(x, y, 0.0));
            let middle = inverse.project_point3(Vec3::new(x, y, 0.5));
            let ray = middle - near;
            stretch = stretch.max(ray.length() / ray.dot(axis).abs());
        }
    }
    stretch * 1.000_01
}

#[derive(Debug)]
pub(super) struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn new(matrix: Mat4) -> Self {
        let rows = matrix.transpose();
        Self {
            planes: [
                rows.w_axis + rows.x_axis,
                rows.w_axis - rows.x_axis,
                rows.w_axis + rows.y_axis,
                rows.w_axis - rows.y_axis,
                rows.z_axis,
                rows.w_axis - rows.z_axis,
            ],
        }
    }

    pub fn intersects(&self, bounds: Bounds) -> bool {
        self.planes
            .iter()
            .all(|&plane| bounds.support(plane) >= 0.0)
    }

    /// Whether `intersects` is true for every box inside `bounds`: `bounds` lies in front of
    /// every plane by more than the rounding of any of those tests.
    pub fn contains(&self, bounds: Bounds) -> bool {
        let center = (bounds.min + bounds.max) * 0.5;
        let half = (bounds.max - bounds.min) * 0.5;
        let reach = center.abs() + half;
        self.planes.iter().all(|&plane| {
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
        self.planes.iter().any(|&plane| {
            let magnitude = plane.truncate().abs().dot(reach) + plane.w.abs();
            bounds.support(plane) < -GROUP_MARGIN * magnitude
        })
    }
}

/// Relative margin of group culling. A box test rounds by a few units in the last place of
/// its operands, about 1e-6 of their magnitude in f32, so a group rejected by more than this
/// margin holds no placement the exact per-placement test could keep.
pub(super) const GROUP_MARGIN: f32 = 1e-5;

/// The view tests a group of placements passes for every placement inside it.
#[derive(Clone, Copy, Debug, Default)]
struct Passed {
    frustum: bool,
    slice: bool,
}

#[derive(Clone, Debug)]
pub(super) struct SelectedRange {
    pub key: BatchKey,
    pub lod: usize,
    pub slots: Range<u32>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Selection {
    pub casters_only: bool,
    /// Light-space slice a cascade's receivers sample; casters missing it are culled.
    pub slice: Option<super::slice_hull::SliceHull>,
    pub ranges: Vec<SelectedRange>,
    pub stats: ViewStats,
}

impl Selection {
    fn reset(&mut self, meshes: &[MeshSelection]) {
        self.ranges.clear();
        self.stats.considered = 0;
        self.stats.culled = 0;
        self.stats.drawn_per_lod.resize(
            meshes
                .iter()
                .map(|mesh| mesh.ceilings_squared.len().max(1))
                .max()
                .unwrap_or(0),
            0,
        );
        self.stats.drawn_per_lod.fill(0);
    }

    /// The view tests every placement inside `group` passes, given those its parent passed;
    /// none when the view admits no placement inside it.
    fn classify(
        &self,
        frustum: &Frustum,
        matrix: Mat4,
        group: Bounds,
        passed: Passed,
    ) -> Option<Passed> {
        if !passed.frustum && frustum.excludes(group) {
            return None;
        }
        let slice = match &self.slice {
            None => true,
            Some(_) if passed.slice => true,
            Some(slice) if slice.excludes(group, matrix) => return None,
            Some(slice) => slice.covers(group, matrix),
        };
        Some(Passed {
            frustum: passed.frustum || frustum.contains(group),
            slice,
        })
    }

    /// Whether the view volume and slice admit a placement inside a group that `passed`
    /// those tests already; `placement` computes its bounds when a test remains.
    fn admits(
        &self,
        frustum: &Frustum,
        matrix: Mat4,
        passed: Passed,
        placement: impl FnOnce() -> Bounds,
    ) -> bool {
        if passed.frustum && passed.slice {
            return true;
        }
        let placement = placement();
        (passed.frustum || frustum.intersects(placement))
            && (passed.slice
                || self
                    .slice
                    .as_ref()
                    .is_none_or(|slice| slice.overlaps(placement, matrix)))
    }

    fn push(&mut self, key: BatchKey, lod: usize, slot: usize) {
        self.stats.drawn_per_lod[lod] += 1;
        let slot = u32::try_from(slot).expect("validated instance slot");
        if let Some(last) = self
            .ranges
            .last_mut()
            .filter(|last| last.key == key && last.lod == lod && last.slots.end == slot)
        {
            last.slots.end += 1;
        } else {
            self.ranges.push(SelectedRange {
                key,
                lod,
                slots: slot..slot + 1,
            });
        }
    }
}

fn distance_squared(position: [i64; 3], eye: [i64; 3]) -> u128 {
    position
        .into_iter()
        .zip(eye)
        .try_fold(0_u128, |sum, (position, eye)| {
            let delta = (i128::from(position) - i128::from(eye)).unsigned_abs();
            sum.checked_add(delta * delta)
        })
        .expect("squared placement distance exceeds u128 millimetres")
}

pub(super) fn terrain_bounds(vertices: &[u8]) -> Bounds {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for vertex in vertices.chunks_exact(crate::geometry::VERTEX_BYTES) {
        let point = Vec3::from_array(std::array::from_fn(|axis| {
            f32::from_le_bytes(
                vertex[axis * 4..axis * 4 + 4]
                    .try_into()
                    .expect("packed position"),
            )
        }));
        min = min.min(point);
        max = max.max(point);
    }
    // Enclose the uploaded vertices, including rounding at flat terrain boundaries.
    let pad = min.abs().max(max.abs()).max(Vec3::ONE) * 0.000_01;
    Bounds {
        min: min - pad,
        max: max + pad,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod policy_tests;

#[cfg(test)]
mod scale_tests;

#[cfg(test)]
mod caster_tests;

#[cfg(test)]
mod cache_tests;

#[cfg(test)]
mod fade_tests;

#[cfg(test)]
mod large_scene;

#[cfg(test)]
mod equality_tests;

#[cfg(test)]
mod group_tests;

#[cfg(test)]
mod timing_probe;

pub(super) fn has_relief(instances: &Instances, meshes: &[MeshSelection]) -> bool {
    instances.batches.iter().any(|(key, batch)| {
        batch.has_instances()
            && (crate::pipeline::PipelineKey::ALL[key.0].relief
                || meshes[key.1]
                    .material_slots
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|slot| {
                        let pipeline = instances.pipelines[slot.material().raw() as usize];
                        crate::pipeline::PipelineKey::ALL[pipeline].relief
                    }))
    })
}
