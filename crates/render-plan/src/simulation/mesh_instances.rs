//! Extraction of 3D mesh instances from a live world into retained scene objects.
//!
//! The world stores opaque mesh and material ids. A host-supplied [`MeshResolver`] maps them to
//! the handles of the scene's mesh and material tables, so this layer loads no assets and a
//! missing mapping is an error instead of a default handle.
//!
//! A resolver can also return the full scene objects a host draws for an instance, with every
//! field the host sets. Extraction then replaces only each object's transform. One instance may
//! resolve to several scene objects, such as LOD levels or impostor cards drawn as separate
//! retained objects.

use glam::{Mat4, Quat, Vec3};
use worldrunner_world_simulation::{MeshInstance, ObjectHandle, World, WorldError};

use crate::{
    MaterialHandle, MeshHandle, SceneObject, SceneRotation, SceneTint, SceneTransform,
    SceneValidationError,
};

/// Millimeters per world unit when the world is in meters.
pub const METERS: f64 = 1_000.0;
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

/// What a host draws for one mesh instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshTemplate {
    /// One object built from the handles and the instance's tint, with default policies.
    Handles(ResolvedMesh),
    /// The scene objects the host draws for the instance, in its scene order. Extraction keeps
    /// every field and replaces each object's transform with the instance's world transform.
    /// The transforms the host puts here are placeholders. An empty list draws nothing.
    Objects(Vec<SceneObject>),
    /// The scene objects the host draws for the instance, with transforms the host already
    /// computed. Extraction returns them unchanged, transform included, and does not read the
    /// world transform, so a world transform a scene object cannot hold is not an error here.
    ///
    /// A host uses this when its own placement policy produces the scene transform, such as a
    /// quantized saved orientation, axis conventions or a per-entity scale, which a unit factor
    /// on the world transform cannot express. The resolver runs that policy on the object's
    /// current state. The world still decides which objects are drawn and which changed, so the
    /// host must write every input of the placement (transform, parent, active state, mesh
    /// instance) to the world for the journal to report it. An empty list draws nothing.
    Placed(Vec<SceneObject>),
}

/// Maps a world mesh instance to scene table handles. The host owns the tables, so it also
/// decides how the instance's LOD set and extra material slots map onto them.
pub trait MeshResolver {
    /// Returns `None` when the host has no table entry for the instance.
    fn resolve(&self, instance: &MeshInstance) -> Option<ResolvedMesh>;

    /// Returns the scene objects to draw for the instance, or `None` when the host has no entry
    /// for it. The default wraps [`resolve`](Self::resolve), so resolvers that only map handles
    /// keep their behavior. Override it to supply full scene objects.
    fn resolve_template(&self, instance: &MeshInstance) -> Option<MeshTemplate> {
        self.resolve(instance).map(MeshTemplate::Handles)
    }
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
    #[error("{0} is not a positive finite number of millimeters per world unit")]
    InvalidUnit(f64),
    #[error("{0:?} resolves to several scene objects; extract it with extract_mesh_objects")]
    SeveralObjects(ObjectHandle),
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

/// How extraction reads world positions.
///
/// The world stores translations as `f32` in a host-chosen unit. Extraction multiplies each
/// translation component by `millimeters_per_unit` in `f64` and rounds to whole millimeters.
/// Scale is a ratio and is never multiplied. An `f32` holds every integer up to 2^24 exactly, so
/// whole world units round-trip exactly up to 16,777,216 units: 16,777 km at the default
/// [`METERS`] and 16.7 km when the world unit is one millimeter. Fractions of a unit are exact
/// only where `f32` can store them: at a magnitude `m` the spacing is about `m * 2^-23`, so a
/// world in millimeters (`1.0`) keeps whole millimeters exact to 2^24 mm and a world in meters
/// keeps millimeters to within half a millimeter up to about 2^13 m, then rounds to the nearest
/// representable value. A unit finer than a millimeter rounds to the nearest whole millimeter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshExtractOptions {
    millimeters_per_unit: f64,
}

impl Default for MeshExtractOptions {
    fn default() -> Self {
        Self {
            millimeters_per_unit: METERS,
        }
    }
}

impl MeshExtractOptions {
    /// Options for a world whose unit is `millimeters_per_unit` millimeters.
    ///
    /// # Errors
    ///
    /// Returns [`MeshExtractError::InvalidUnit`] unless the value is positive and finite.
    pub fn with_millimeters_per_unit(millimeters_per_unit: f64) -> Result<Self, MeshExtractError> {
        if millimeters_per_unit.is_finite() && millimeters_per_unit > 0.0 {
            Ok(Self {
                millimeters_per_unit,
            })
        } else {
            Err(MeshExtractError::InvalidUnit(millimeters_per_unit))
        }
    }

    pub const fn millimeters_per_unit(&self) -> f64 {
        self.millimeters_per_unit
    }
}

/// One world object's scene image. An object drawn as several scene objects appears once per
/// scene object, consecutively and in the resolver's order.
#[derive(Clone, Copy, Debug)]
pub struct ExtractedMeshObject {
    pub object: ObjectHandle,
    pub scene: SceneObject,
}

/// Extracts every active object that has a mesh instance, in object handle order, with
/// [`MeshExtractOptions::default`]. This is the initial scene build; later frames use the
/// journal.
pub fn extract_mesh_instances(
    world: &World,
    resolver: &impl MeshResolver,
) -> Result<Vec<ExtractedMeshObject>, MeshExtractError> {
    extract_mesh_instances_with(world, resolver, MeshExtractOptions::default())
}

/// [`extract_mesh_instances`] with explicit options.
pub fn extract_mesh_instances_with(
    world: &World,
    resolver: &impl MeshResolver,
    options: MeshExtractOptions,
) -> Result<Vec<ExtractedMeshObject>, MeshExtractError> {
    let mut extracted = Vec::new();
    for object in world.mesh_instance_objects() {
        for scene in extract_mesh_objects(world, object, resolver, options)? {
            extracted.push(ExtractedMeshObject { object, scene });
        }
    }
    Ok(extracted)
}

/// The scene object for one world object, or `None` when it should not be drawn: it is gone,
/// inactive in its hierarchy, or has no mesh instance. Fails with
/// [`MeshExtractError::SeveralObjects`] when the resolver returns more than one object.
pub fn extract_mesh_object(
    world: &World,
    object: ObjectHandle,
    resolver: &impl MeshResolver,
) -> Result<Option<SceneObject>, MeshExtractError> {
    let mut scenes = extract_mesh_objects(world, object, resolver, MeshExtractOptions::default())?;
    if scenes.len() > 1 {
        return Err(MeshExtractError::SeveralObjects(object));
    }
    Ok(scenes.pop())
}

/// Every scene object for one world object, in the resolver's order. The list is empty when the
/// object should not be drawn: it is gone, inactive in its hierarchy, has no mesh instance, or
/// the resolver returned no objects.
pub fn extract_mesh_objects(
    world: &World,
    object: ObjectHandle,
    resolver: &impl MeshResolver,
    options: MeshExtractOptions,
) -> Result<Vec<SceneObject>, MeshExtractError> {
    let instance = match world.mesh_instance(object) {
        Ok(Some(instance)) => instance,
        Ok(None) | Err(WorldError::StaleObject(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if !world.active_in_hierarchy(object)? {
        return Ok(Vec::new());
    }
    let template = resolver
        .resolve_template(instance)
        .ok_or(MeshExtractError::Unresolved(object))?;
    match template {
        MeshTemplate::Placed(placed) => Ok(placed),
        MeshTemplate::Handles(handles) => {
            let transform = world_scene_transform(world, object, options)?;
            let tint = scene_tint(instance.tint)
                .map_err(|source| MeshExtractError::Scene { object, source })?;
            Ok(vec![
                SceneObject::new(transform, handles.mesh, handles.material).with_tint(tint),
            ])
        }
        MeshTemplate::Objects(templates) => {
            let transform = world_scene_transform(world, object, options)?;
            Ok(templates
                .into_iter()
                .map(|template| template.with_transform(transform))
                .collect())
        }
    }
}

fn world_scene_transform(
    world: &World,
    object: ObjectHandle,
    options: MeshExtractOptions,
) -> Result<SceneTransform, MeshExtractError> {
    let transform = scene_transform(world.world_matrix(object)?, object, options)?;
    transform
        .validate()
        .map_err(|source| MeshExtractError::Scene { object, source })?;
    Ok(transform)
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

fn scene_transform(
    matrix: Mat4,
    object: ObjectHandle,
    options: MeshExtractOptions,
) -> Result<SceneTransform, MeshExtractError> {
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
        position_millimeters: position_millimeters(translation, options.millimeters_per_unit)
            .ok_or(fail("position is out of range"))?,
        rotation: scene_rotation(rotation),
        scale_millionths: scale_millionths(scales[0]).ok_or(fail("scale is out of range"))?,
    })
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is range-checked against MAX_MILLIMETERS before the cast"
)]
fn position_millimeters(translation: Vec3, millimeters_per_unit: f64) -> Option<[i64; 3]> {
    let mut out = [0_i64; 3];
    for (slot, axis) in out.iter_mut().zip(translation.to_array()) {
        let millimeters = (f64::from(axis) * millimeters_per_unit).round();
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
