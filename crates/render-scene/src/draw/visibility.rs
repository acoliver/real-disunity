use std::ops::Range;

use glam::{Mat4, Vec3, Vec4};
use worldrunner_render_plan::{BillboardPolicy, SceneMesh, SceneTransform};

use crate::geometry::{millimeters_to_meters, model_matrix};
use crate::instances::{BatchKey, Instances};

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
            bounds: Bounds { min, max },
            billboard_radius: billboard.then(|| radius_squared.sqrt()),
            ceilings_squared: mesh
                .max_distances_millimeters()
                .iter()
                .map(|&value| u128::from(value) * u128::from(value))
                .collect(),
        }
    }

    pub fn lod(&self, distance_squared: u128) -> usize {
        self.ceilings_squared
            .iter()
            .position(|&ceiling| distance_squared <= ceiling)
            .unwrap_or_else(|| self.ceilings_squared.len().saturating_sub(1))
    }

    fn wind_bounds(
        &self,
        transform: &SceneTransform,
        extent: Vec3,
        curved: Option<Vec3>,
    ) -> Bounds {
        let bounds = self.world_bounds(transform);
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
            .max(self.billboard_radius.unwrap_or(0.0));
        let scale = model_matrix(transform).x_axis.truncate().length();
        bounds.expanded(Vec3::splat((6.0 * radius + 7.0 * origin.length()) * scale))
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
    pub ranges: Vec<SelectedRange>,
    pub stats: ViewStats,
}

impl Selection {
    pub fn rebuild(
        &mut self,
        instances: &Instances,
        meshes: &[MeshSelection],
        matrix: Mat4,
        eye_millimeters: [i64; 3],
    ) {
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
        let frustum = Frustum::new(matrix);
        for (&key, batch) in &instances.batches {
            let mesh = &meshes[key.1];
            // Traversal scans allocated slots; removed slots contribute neither
            // selection work nor statistics.
            for (slot, object, visible) in batch.placements() {
                self.stats.considered += 1;
                if !visible
                    || !frustum.intersects(mesh.wind_bounds(
                        object.transform(),
                        instances.wind_extents[key.2],
                        instances.curved_wind[key.2],
                    ))
                {
                    self.stats.culled += 1;
                    continue;
                }
                let distance_squared = object
                    .transform()
                    .position_millimeters
                    .into_iter()
                    .zip(eye_millimeters)
                    .try_fold(0_u128, |sum, (position, eye)| {
                        let delta = (i128::from(position) - i128::from(eye)).unsigned_abs();
                        sum.checked_add(delta * delta)
                    })
                    .expect("squared placement distance exceeds u128 millimetres");
                let lod = mesh.lod(distance_squared);
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
    }
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
        // Scan allocated slots, excluding removed placements from caster fitting.
        for (_, object, visible) in batch.placements().filter(|(_, _, visible)| *visible) {
            debug_assert!(visible);
            near = near.min(caster_near(
                meshes[mesh].wind_bounds(
                    object.transform(),
                    instances.wind_extents[material],
                    instances.curved_wind[material],
                ),
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
