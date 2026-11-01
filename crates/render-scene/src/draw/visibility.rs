use std::ops::Range;

use glam::{Mat4, Vec3, Vec4};
use worldrunner_render_plan::{BillboardPolicy, SceneMesh, SceneTransform};

use crate::geometry::{millimeters_to_meters, model_matrix};
use crate::instances::{BatchKey, Instances};
use placement::BatchBounds;

mod placement;

/// Object placement counts for one recorded view, excluding terrain.
/// Hidden live placements count as culled; removed placements are not considered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewStats {
    /// Number of live placements examined.
    pub considered: usize,
    /// Hidden or outside the view volume.
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
    fn expanded(self, extent: Vec3) -> Self {
        Self {
            min: self.min - extent,
            max: self.max + extent,
        }
    }

    fn transformed(self, matrix: Mat4) -> Self {
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
fn relief_ray_stretch(matrix: Mat4) -> f32 {
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

    pub fn rebuild(
        &mut self,
        instances: &Instances,
        meshes: &[MeshSelection],
        matrix: Mat4,
        eye_millimeters: [i64; 3],
    ) {
        self.reset(meshes);
        let frustum = Frustum::new(matrix);
        let relief_stretch = if instances
            .impostor_extents
            .iter()
            .any(|extent| extent[1] != 0.0)
        {
            relief_ray_stretch(matrix)
        } else {
            1.0
        };
        for (&key, batch) in &instances.batches {
            let mesh = &meshes[key.1];
            let mut bounds = BatchBounds::new(mesh, instances, key.2, batch);
            // Traversal scans allocated slots; removed slots contribute neither
            // selection work nor statistics.
            for (slot, object, visible) in batch.placements() {
                self.stats.considered += 1;
                if !visible
                    || (self.casters_only && !object.casts_shadow())
                    || !frustum.intersects(bounds.placement(
                        slot,
                        object.transform(),
                        relief_stretch,
                    ))
                {
                    self.stats.culled += 1;
                    continue;
                }
                let distance_squared =
                    distance_squared(object.transform().position_millimeters, eye_millimeters);
                let lod = mesh.lod(distance_squared, object.lod_distance_scale_millionths());
                self.push(key, lod, slot);
            }
        }
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

/// Extend only the toward-light depth end of the authored receiver volume.
/// XY coverage and its far end stay fixed. Casters beyond the original near
/// plane must also fit the depth projection, not merely survive CPU culling.
pub(super) fn caster_matrix(
    instances: &Instances,
    meshes: &[MeshSelection],
    terrain: Option<Bounds>,
    receiver: Mat4,
) -> Mat4 {
    let mut near = terrain.map_or(0.0, |bounds| caster_near(bounds, receiver));
    for (&(_, mesh, material), batch) in &instances.batches {
        let mut bounds = BatchBounds::new(&meshes[mesh], instances, material, batch);
        // Scan allocated slots, excluding removed placements from caster fitting.
        for (slot, object, visible) in batch
            .placements()
            .filter(|(_, object, visible)| *visible && object.casts_shadow())
        {
            debug_assert!(visible);
            near = near.min(caster_near(
                bounds.placement(slot, object.transform(), 0.0),
                receiver,
            ));
        }
    }
    let scale = (1.0 - near).recip();
    let mut remap = Mat4::IDENTITY;
    remap.z_axis.z = scale;
    remap.w_axis.z = -near * scale;
    remap * receiver
}

fn caster_near(bounds: Bounds, receiver: Mat4) -> f32 {
    let bounds = bounds.transformed(receiver);
    if bounds.min.x <= 1.0
        && bounds.max.x >= -1.0
        && bounds.min.y <= 1.0
        && bounds.max.y >= -1.0
        && bounds.min.z <= 1.0
    {
        bounds.min.z.min(0.0)
    } else {
        0.0
    }
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
