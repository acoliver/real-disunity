//! Shadow caster selection and shadow map fitting for a light.
//!
//! A render pipeline asks, for each visible light, which renderers cast shadows into the view and
//! what view and projection a shadow map for the light is drawn with. This module answers from
//! plain values: the light, the camera's view volume and the world-space boxes of the renderers a
//! culling kept. It holds no world state and takes the shapes of the managed `UnityEngine` calls
//! (`GetShadowCasterBounds` and the `Compute*ShadowMatricesAndCullingPrimitives` family), so the
//! managed engine and the ABI only carry values in and out.
//!
//! Matrices use the engine's camera convention: a view matrix looks down negative z with the
//! engine's left-handed world, and a projection maps depth to -1 at the near plane and 1 at the
//! far plane. A pipeline converts the projection for the device it draws on.
//! Planes are a unit normal and a distance, and a point is inside a plane when
//! `dot(normal, point) + distance` is not negative.

mod directional;
mod frame;
mod local;

use glam::{Mat4, Vec3, Vec4};

pub use directional::{DirectionalSplit, directional_split};
pub use local::{point_split, spot_split};

/// The shadow casting mode value of a renderer that casts no shadow.
pub const CASTS_NO_SHADOW: u32 = 0;

/// Most culling planes a split carries.
pub const MAX_SPLIT_PLANES: usize = 10;

/// Why a shadow query was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ShadowError {
    #[error("a light or camera value is not finite or has no usable shape")]
    InvalidLight,
    #[error("the split index is outside the split count, which is 1 to 4")]
    InvalidSplit,
    #[error("the split ratios must be above 0, ascending and at most 1")]
    InvalidRatio,
    #[error("the shadow resolution must be at least 4")]
    InvalidResolution,
    #[error("the camera's view volume cannot be fitted")]
    InvalidCamera,
    #[error("the cube face is outside 0 to 5")]
    InvalidFace,
}

/// The kinds of light, with the numbers `UnityEngine.LightType` has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowLightKind {
    Spot,
    Directional,
    Point,
}

impl ShadowLightKind {
    /// The kind of a `LightType` number, or `None` for a number that names none (area lights
    /// among them cast no shadow here).
    #[must_use]
    pub fn from_number(number: u32) -> Option<Self> {
        match number {
            0 => Some(Self::Spot),
            1 => Some(Self::Directional),
            2 => Some(Self::Point),
            _ => None,
        }
    }
}

/// The light shadows are drawn for. A light shines along the z axis of its matrix. `range` is the
/// distance a point or spot light reaches and `spot_angle` the full angle of a spot light, in
/// degrees; a directional light ignores both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLight {
    pub kind: ShadowLightKind,
    pub local_to_world: Mat4,
    pub range: f32,
    pub spot_angle: f32,
}

/// A renderer a culling kept: its world-space box, its shadow casting mode (0 casts none) and its
/// layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowCaster {
    pub min: Vec3,
    pub max: Vec3,
    pub casting_mode: u32,
    pub layer: u32,
}

/// The renderers a culling kept and the layers it drew.
#[derive(Clone, Copy, Debug)]
pub struct Casters<'a> {
    pub list: &'a [ShadowCaster],
    pub culling_mask: u32,
}

/// The camera whose view the shadows serve: its projection times view matrix, its position and
/// the distance shadows reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowCamera {
    pub culling_matrix: Mat4,
    pub origin: Vec3,
    pub shadow_distance: f32,
}

/// What a shadow map for one split is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowSplit {
    pub view: Mat4,
    pub projection: Mat4,
    /// The projection times the view.
    pub culling_matrix: Mat4,
    /// The centre and radius of the sphere around the camera's slice of a directional split, or
    /// zero for a spot or point light.
    pub culling_sphere: Vec4,
    /// The planes a caster must not be wholly outside of, with normals into the volume.
    pub planes: Vec<Vec4>,
}

impl Casters<'_> {
    /// The boxes of the renderers that cast a shadow the light can affect: a renderer on a drawn
    /// layer whose mode is not off, and for a spot or point light one whose box touches the
    /// sphere of the light's range.
    fn affecting(&self, light: &ShadowLight) -> Vec<(Vec3, Vec3)> {
        let position = light.local_to_world.w_axis.truncate();
        self.list
            .iter()
            .filter(|caster| caster.casting_mode != CASTS_NO_SHADOW)
            .filter(|caster| caster.layer < 32 && (self.culling_mask >> caster.layer) & 1 == 1)
            .filter(|caster| match light.kind {
                ShadowLightKind::Directional => true,
                ShadowLightKind::Spot | ShadowLightKind::Point => {
                    let nearest = position.clamp(caster.min, caster.max);
                    nearest.distance_squared(position) <= light.range * light.range
                }
            })
            .map(|caster| (caster.min, caster.max))
            .collect()
    }
}

/// The box around the casters that affect the light, or `None` when none does.
#[must_use]
pub fn caster_bounds(light: &ShadowLight, casters: &Casters<'_>) -> Option<(Vec3, Vec3)> {
    casters
        .affecting(light)
        .into_iter()
        .reduce(|(min, max), (low, high)| (min.min(low), max.max(high)))
}
