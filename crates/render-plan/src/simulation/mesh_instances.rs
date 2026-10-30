//! Extraction of 3D mesh instances from a live world into retained scene objects.
//!
//! The world stores opaque mesh and material ids. A host-supplied [`MeshResolver`] maps them to
//! the handles of the scene's mesh and material tables, so this layer loads no assets and a
//! missing mapping is an error instead of a default handle.

use glam::{Mat4, Quat, Vec3};
use worldrunner_world_simulation::{MeshInstance, ObjectHandle, World, WorldError};

use crate::{
    MaterialHandle, MeshHandle, SceneObject, SceneRotation, SceneTint, SceneTransform,
    SceneValidationError,
};

/// Millimeters per world unit. The world is in meters.
const MILLIMETERS_PER_UNIT: f64 = 1_000.0;
const MICRO: f64 = 1_000_000.0;
/// Relative spread of the three axis scales that still counts as uniform.
const UNIFORM_SCALE_TOLERANCE: f32 = 1.0e-4;
/// Positions beyond this many millimeters have no exact `f64` integer image to round.
const MAX_MILLIMETERS: f64 = 9.0e15;

/// The scene table entries one mesh instance draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedMesh {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
}

/// Maps a world mesh instance to scene table handles. The host owns the tables, so it also
/// decides how the instance's LOD set and extra material slots map onto them.
pub trait MeshResolver {
    /// Returns `None` when the host has no table entry for the instance.
    fn resolve(&self, instance: &MeshInstance) -> Option<ResolvedMesh>;
}

impl<F> MeshResolver for F
where
    F: Fn(&MeshInstance) -> Option<ResolvedMesh>,
{
    fn resolve(&self, instance: &MeshInstance) -> Option<ResolvedMesh> {
        self(instance)
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MeshExtractError {
    #[error("world: {0}")]
    World(#[from] WorldError),
    #[error("the resolver has no scene entry for the mesh instance on {0:?}")]
    Unresolved(ObjectHandle),
    #[error("{object:?} has a world transform a scene object cannot hold: {reason}")]
    Transform {
        object: ObjectHandle,
        reason: &'static str,
    },
    #[error("scene rejected the object for {object:?}: {source}")]
    Scene {
        object: ObjectHandle,
        source: SceneValidationError,
    },
}

/// One world object's scene image.
#[derive(Clone, Copy, Debug)]
pub struct ExtractedMeshObject {
    pub object: ObjectHandle,
    pub scene: SceneObject,
}

/// Extracts every active object that has a mesh instance, in object handle order.
/// This is the initial scene build; later frames use the journal.
pub fn extract_mesh_instances(
    world: &World,
    resolver: &impl MeshResolver,
) -> Result<Vec<ExtractedMeshObject>, MeshExtractError> {
    let mut extracted = Vec::new();
    for object in world.mesh_instance_objects() {
        if let Some(scene) = extract_mesh_object(world, object, resolver)? {
            extracted.push(ExtractedMeshObject { object, scene });
        }
    }
    Ok(extracted)
}

/// The scene object for one world object, or `None` when it should not be drawn: it is gone,
/// inactive in its hierarchy, or has no mesh instance.
pub fn extract_mesh_object(
    world: &World,
    object: ObjectHandle,
    resolver: &impl MeshResolver,
) -> Result<Option<SceneObject>, MeshExtractError> {
    let instance = match world.mesh_instance(object) {
        Ok(Some(instance)) => instance,
        Ok(None) | Err(WorldError::StaleObject(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !world.active_in_hierarchy(object)? {
        return Ok(None);
    }
    let handles = resolver
        .resolve(instance)
        .ok_or(MeshExtractError::Unresolved(object))?;
    let transform = scene_transform(world.world_matrix(object)?, object)?;
    transform
        .validate()
        .map_err(|source| MeshExtractError::Scene { object, source })?;
    let tint =
        scene_tint(instance.tint).map_err(|source| MeshExtractError::Scene { object, source })?;
    Ok(Some(
        SceneObject::new(transform, handles.mesh, handles.material).with_tint(tint),
    ))
}

fn scene_tint(tint: [f32; 3]) -> Result<SceneTint, SceneValidationError> {
    SceneTint::new(tint.map(|channel| micro(f64::from(channel))))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tint channels are validated finite and within 0..=4 before they reach the scene"
)]
fn micro(value: f64) -> u32 {
    (value * MICRO).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

fn scene_transform(matrix: Mat4, object: ObjectHandle) -> Result<SceneTransform, MeshExtractError> {
    let fail = |reason| MeshExtractError::Transform { object, reason };
    let scales = [matrix.x_axis, matrix.y_axis, matrix.z_axis].map(|axis| axis.truncate().length());
    let (low, high) = scales.iter().fold((f32::MAX, 0.0_f32), |(low, high), &s| {
        (low.min(s), high.max(s))
    });
    if !high.is_finite() || low <= 0.0 {
        return Err(fail("scale is zero or not finite"));
    }
    if high - low > UNIFORM_SCALE_TOLERANCE * high {
        return Err(fail("scale is not uniform"));
    }
    if matrix.determinant() <= 0.0 {
        return Err(fail("transform mirrors the object"));
    }
    let (_, rotation, translation) = matrix.to_scale_rotation_translation();
    Ok(SceneTransform {
        position_millimeters: position_millimeters(translation)
            .ok_or(fail("position is out of range"))?,
        rotation: scene_rotation(rotation),
        scale_millionths: scale_millionths(scales[0]).ok_or(fail("scale is out of range"))?,
    })
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is range-checked against MAX_MILLIMETERS before the cast"
)]
fn position_millimeters(translation: Vec3) -> Option<[i64; 3]> {
    let mut out = [0_i64; 3];
    for (slot, axis) in out.iter_mut().zip(translation.to_array()) {
        let millimeters = (f64::from(axis) * MILLIMETERS_PER_UNIT).round();
        if !millimeters.is_finite() || millimeters.abs() > MAX_MILLIMETERS {
            return None;
        }
        *slot = millimeters as i64;
    }
    Some(out)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the scale is positive and bounded by the range check before the cast"
)]
fn scale_millionths(scale: f32) -> Option<u64> {
    let millionths = (f64::from(scale) * MICRO).round();
    (1.0..=MAX_MILLIMETERS)
        .contains(&millionths)
        .then_some(millionths as u64)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "unit quaternion components are within -1..=1, so micro-units fit i64"
)]
fn scene_rotation(rotation: Quat) -> SceneRotation {
    let rotation = rotation.normalize();
    let [x, y, z, w] = rotation
        .to_array()
        .map(|c| (f64::from(c) * MICRO).round() as i64);
    SceneRotation { x, y, z, w }
}
