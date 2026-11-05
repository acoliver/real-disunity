//! CPU queries against resident, undeformed LOD-zero triangle geometry.
use crate::{RenderScene, SceneMeshLod, SceneObject, SceneTransform};
use glam::{DVec3, Mat4, Quat, Vec3};
use worldrunner_camera::ScreenRay;

/// Caller-selected identity. Retained initial objects use IDs `0..object_count`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

/// Nearest geometric intersection, measured from the supplied ray origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickHit {
    pub object: ObjectId,
    pub distance_meters: f64,
    pub point_meters: Vec3,
    /// Zero-based triangle in the resident mesh's first LOD.
    pub triangle: usize,
    /// Weights of the triangle's three indexed vertices.
    pub barycentric: [f64; 3],
}

impl RenderScene {
    /// Picks resident LOD-zero triangles using scene-space metres, without GPU work.
    /// Objects need unique IDs, validated transforms and handles into this scene's
    /// resource tables. Filtering precedes bounds tests. Exact ties choose the lowest `ObjectId`,
    /// then the lowest triangle index, independent of iteration order.
    ///
    /// Material/LOD-slot double-sided flags are respected; texture alpha, shader
    /// deformation, fade dithering, billboard replacement and terrain are excluded.
    /// The ray direction is normalized here; a nonfinite or zero ray returns None.
    pub fn pick(
        &self,
        ray: ScreenRay,
        objects: impl IntoIterator<Item = (ObjectId, SceneObject)>,
        mut include: impl FnMut(ObjectId) -> bool,
    ) -> Option<PickHit> {
        let origin = ray.origin_meters.as_dvec3();
        let direction = ray.direction.as_dvec3().try_normalize()?;
        if !origin.is_finite() || !direction.is_finite() {
            return None;
        }
        let mut nearest = None;
        for (id, object) in objects {
            if include(id) {
                self.pick_object((origin, direction), id, object, &mut nearest);
            }
        }
        nearest
    }

    fn pick_object(
        &self,
        ray: (DVec3, DVec3),
        id: ObjectId,
        object: SceneObject,
        nearest: &mut Option<PickHit>,
    ) {
        let lod = &self.geometry().meshes()[object.mesh().raw() as usize].lods()[0];
        let model = model_matrix(object.transform()).as_dmat4();
        let (minimum, maximum) = lod.bounds_meters();
        let center = model.transform_point3((minimum + maximum) * 0.5);
        let extent = (maximum - minimum) * 0.5;
        let axes = [
            model.x_axis.truncate(),
            model.y_axis.truncate(),
            model.z_axis.truncate(),
        ];
        let extent = axes[0].abs() * extent.x + axes[1].abs() * extent.y + axes[2].abs() * extent.z;
        if !intersects_bounds(ray, center - extent, center + extent) {
            return;
        }
        let inverse = model.inverse();
        let local = (
            inverse.transform_point3(ray.0),
            inverse.transform_vector3(ray.1),
        );
        let mut material_slot = 0;
        for (triangle, indices) in lod.indices().chunks_exact(3).enumerate() {
            let vertices = [indices[0], indices[1], indices[2]]
                .map(|index| vertex_meters(lod.vertices()[index as usize].position_millimeters));
            let double_sided =
                self.triangle_double_sided(lod, object, triangle, &mut material_slot);
            if let Some((distance, barycentric)) = intersect_triangle(local, vertices, double_sided)
            {
                let hit = PickHit {
                    object: id,
                    distance_meters: distance.abs(),
                    point_meters: (ray.0 + ray.1 * distance).as_vec3(),
                    triangle,
                    barycentric,
                };
                if nearest.is_none_or(|old| precedes(hit, old)) {
                    *nearest = Some(hit);
                }
            }
        }
    }

    fn triangle_double_sided(
        &self,
        lod: &SceneMeshLod,
        object: SceneObject,
        triangle: usize,
        material_slot: &mut usize,
    ) -> bool {
        let handle = lod.material_slots().map_or(object.material(), |slots| {
            if slots[*material_slot].indices().end as usize <= triangle * 3 {
                *material_slot += 1;
            }
            slots[*material_slot].material()
        });
        self.resources().materials()[handle.raw() as usize].double_sided()
    }
}

fn precedes(hit: PickHit, old: PickHit) -> bool {
    hit.distance_meters
        .total_cmp(&old.distance_meters)
        .then_with(|| hit.object.cmp(&old.object))
        .then_with(|| hit.triangle.cmp(&old.triangle))
        .is_lt()
}

fn intersects_bounds(ray: (DVec3, DVec3), minimum: DVec3, maximum: DVec3) -> bool {
    let mut near = 0.0_f64;
    let mut far = f64::INFINITY;
    for axis in 0..3 {
        if ray.1[axis] == 0.0 {
            if ray.0[axis] < minimum[axis] || ray.0[axis] > maximum[axis] {
                return false;
            }
        } else {
            let a = (minimum[axis] - ray.0[axis]) / ray.1[axis];
            let b = (maximum[axis] - ray.0[axis]) / ray.1[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    near <= far
}

fn intersect_triangle(
    ray: (DVec3, DVec3),
    vertices: [DVec3; 3],
    double_sided: bool,
) -> Option<(f64, [f64; 3])> {
    let edge1 = vertices[1] - vertices[0];
    let edge2 = vertices[2] - vertices[0];
    let p = ray.1.cross(edge2);
    let determinant = edge1.dot(p);
    if determinant == 0.0 || (!double_sided && determinant < 0.0) {
        return None;
    }
    let offset = ray.0 - vertices[0];
    let u = offset.dot(p) / determinant;
    let q = offset.cross(edge1);
    let v = ray.1.dot(q) / determinant;
    let distance = edge2.dot(q) / determinant;
    if u < 0.0 || v < 0.0 || u + v > 1.0 || distance < 0.0 {
        return None;
    }
    Some((distance, [1.0 - u - v, u, v]))
}

/// Model matrix in the scene's unchanged Z-up metre basis, shared with rasterization.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "converts authored integers to GPU float precision"
)]
pub fn model_matrix(transform: &SceneTransform) -> Mat4 {
    let r = transform.rotation;
    let q = [r.x, r.y, r.z, r.w].map(|value| (value as f64 / 1_000_000.0) as f32);
    let rotation = Quat::from_array(q).normalize();
    Mat4::from_scale_rotation_translation(
        Vec3::splat((transform.scale_millionths as f64 / 1_000_000.0) as f32),
        rotation,
        vertex_meters(transform.position_millimeters).as_vec3(),
    )
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "converts authored integers to GPU float precision"
)]
fn vertex_meters(position: [i64; 3]) -> DVec3 {
    Vec3::from_array(position.map(|value| (value as f64 / 1000.0) as f32)).as_dvec3()
}

impl SceneMeshLod {
    pub(crate) fn bounds_meters(&self) -> (DVec3, DVec3) {
        let (minimum, maximum) = self.bounds_millimeters();
        (vertex_meters(minimum), vertex_meters(maximum))
    }
}
