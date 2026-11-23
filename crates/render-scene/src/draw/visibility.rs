use std::ops::Range;

use glam::{Mat4, Vec3};
use worldrunner_render_plan::{BillboardPolicy, SceneMesh, SceneTransform};

use super::slice_hull::LightMap;
use crate::geometry::{millimeters_to_meters, model_matrix};
use crate::instances::{BatchKey, Instances};
#[cfg(test)]
pub(super) use casters::caster_matrix;
pub(super) use casters::{CasterScan, FitScratch, caster_placement};
pub(super) use frustum::{Frustum, Region};
pub(super) use select::{PlacementRows, with_placements};
pub(super) use workers::available as selection_threads;

mod casters;
mod fade;
mod frustum;
mod placement;
mod select;
mod workers;

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
    pub(super) fn outward_rounded(self) -> Self {
        // Cover float rounding in model and clip transforms, including flat bounds.
        let pad = self.min.abs().max(self.max.abs()).max(Vec3::ONE) * 0.000_01;
        self.expanded(pad)
    }

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

    /// The box's centre and half extent.
    pub(super) fn center_half(self) -> (Vec3, Vec3) {
        ((self.min + self.max) * 0.5, (self.max - self.min) * 0.5)
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

    /// Levels of detail of the mesh.
    pub(super) fn levels(&self) -> usize {
        self.ceilings_squared.len()
    }

    /// Fills `ceilings` with each level's squared distance ceiling for a placement whose LOD
    /// distances scale by `scale_millionths`.
    pub(super) fn scaled_ceilings(&self, scale_millionths: Option<u32>, ceilings: &mut [u128]) {
        for (scaled, &ceiling) in ceilings.iter_mut().zip(&self.ceilings_squared) {
            *scaled = scaled_ceiling(ceiling, scale_millionths);
        }
    }

    /// The level a placement draws at from a squared distance, given its `scaled_ceilings`:
    /// the first whose ceiling the distance does not exceed, else the last.
    pub(super) fn level(distance_squared: u128, ceilings: &[u128]) -> usize {
        ceilings
            .iter()
            .position(|&ceiling| distance_squared <= ceiling)
            .unwrap_or_else(|| ceilings.len().saturating_sub(1))
    }

    /// How far, in millimetres, an eye at `distance_squared` from a placement may move before
    /// the level the placement draws at, `level` from its `scaled_ceilings`, can differ.
    ///
    /// The level is `level` while the squared distance stays above every ceiling of the levels
    /// before it and, unless `level` is the last, within its own ceiling. A move of the eye of
    /// at most the margin changes the distance by at most the margin, and the margin leaves
    /// room for the rounding down of each integer square root, so the distance cannot reach a
    /// ceiling.
    pub(super) fn level_margin(distance_squared: u128, ceilings: &[u128], level: usize) -> u64 {
        let distance = distance_squared.isqrt();
        let below = ceilings[..level]
            .iter()
            .max()
            .map(|&ceiling| distance.saturating_sub(ceiling.isqrt() + 1));
        let above = (level + 1 < ceilings.len())
            .then(|| ceilings[level].isqrt().saturating_sub(distance + 1));
        below
            .into_iter()
            .chain(above)
            .min()
            .map_or(u64::MAX, |margin| u64::try_from(margin).unwrap_or(u64::MAX))
    }

    #[cfg(test)]
    pub fn lod(&self, distance_squared: u128, scale_millionths: Option<u32>) -> usize {
        let mut ceilings = vec![0; self.levels()];
        self.scaled_ceilings(scale_millionths, &mut ceilings);
        Self::level(distance_squared, &ceilings)
    }

    /// The distance from the mesh origin to the farthest point of its bounding box, in metres.
    pub(super) fn radius(&self) -> f32 {
        self.bounds.min.abs().max(self.bounds.max.abs()).length()
    }

    /// The bounds of a placement over the vertices of every level. `reach` is how far, in the
    /// mesh's own space, a pose can carry a vertex from its bind position.
    pub(super) fn wind_bounds(
        &self,
        transform: &SceneTransform,
        extent: Vec3,
        curved: Option<Vec3>,
        shape: [f32; 2],
        reach: f32,
    ) -> Bounds {
        let [shaped_radius, capture_radius] = shape;
        let model = model_matrix(transform);
        let scale = model.x_axis.truncate().length();
        let mut bounds = self
            .world_bounds(transform)
            .expanded(Vec3::splat(reach * scale));
        if shaped_radius > 0.0 {
            let center = model.w_axis.truncate();
            let extent = Vec3::splat((shaped_radius + reach) * scale * 1.000_01);
            bounds.min = bounds.min.min(center - extent);
            bounds.max = bounds.max.max(center + extent);
        }
        let Some(origin) = curved else {
            return bounds.expanded(extent);
        };
        // Each arc is bounded by twice its input length, including arbitrary
        // force directions. Projection and attachment together fit six radii.
        let radius = (self.radius() + reach)
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
        self.wind_bounds(transform, wind, curved, [impostor[0], impostor[2]], 0.0)
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
        bounds.outward_rounded()
    }
}

/// The largest squared distance within `ceiling` once LOD distances scale by `scale`
/// millionths: `distance / scale^2 <= ceiling / 10^12` holds exactly for the squared distances
/// up to it, so a placement compares one integer per level instead of dividing.
fn scaled_ceiling(ceiling: u128, scale: Option<u32>) -> u128 {
    const UNIT_SQUARED: u128 = 1_000_000_000_000;
    let Some(scale) = scale else {
        return ceiling;
    };
    let scale_squared = u128::from(scale) * u128::from(scale);
    // floor(ceiling * scale^2 / 10^12), split so the remainder product stays below 10^28. A
    // ceiling beyond u128 admits every squared distance.
    (ceiling / UNIT_SQUARED)
        .checked_mul(scale_squared)
        .and_then(|whole| whole.checked_add(ceiling % UNIT_SQUARED * scale_squared / UNIT_SQUARED))
        .unwrap_or(u128::MAX)
}

/// The scaled ceiling test before `scaled_ceiling`, kept as its reference.
#[cfg(test)]
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
    /// An empty selection that will hold `ranges` ranges without growing.
    pub fn with_capacity(ranges: usize) -> Self {
        Self {
            ranges: Vec::with_capacity(ranges),
            ..Self::default()
        }
    }

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
        light: &LightMap,
        group: Bounds,
        passed: Passed,
    ) -> Option<Passed> {
        let in_frustum = passed.frustum
            || match frustum.region(group) {
                Region::Outside => return None,
                Region::Inside => true,
                Region::Straddles => false,
            };
        let in_slice = passed.slice
            || match &self.slice {
                None => true,
                Some(slice) => match slice.region(group, light) {
                    Region::Outside => return None,
                    Region::Inside => true,
                    Region::Straddles => false,
                },
            };
        Some(Passed {
            frustum: in_frustum,
            slice: in_slice,
        })
    }

    /// Whether the view volume and slice admit a placement inside a group that `passed`
    /// those tests already; `placement` computes its bounds when a test remains.
    fn admits(
        &self,
        frustum: &Frustum,
        light: &LightMap,
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
                    .is_none_or(|slice| slice.overlaps(placement, light)))
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
    let deltas = [0, 1, 2].map(|axis| position[axis].abs_diff(eye[axis]));
    // Squares of deltas below 2^31 sum to less than 2^64, as a placement's always do.
    if deltas.iter().all(|&delta| delta < 1 << 31) {
        return u128::from(deltas.iter().map(|&delta| delta * delta).sum::<u64>());
    }
    deltas
        .into_iter()
        .try_fold(0_u128, |sum, delta| {
            sum.checked_add(u128::from(delta) * u128::from(delta))
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
    Bounds { min, max }.outward_rounded()
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
mod caster_scan_tests;

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
mod pin_tests;

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

#[cfg(test)]
mod frame_cost_tests;
