//! Perspective shadow maps of spot and point lights.
//!
//! A spot light looks along its own axis with its cone as the field of view; a point light looks
//! along one of the six world axes with a quarter turn for a field of view (plus a bias a
//! pipeline asks for, which widens each face so the filtering at its edges has texels to read).

use super::frame::{LightBasis, perspective, side_and_far_planes};
use super::{Casters, ShadowError, ShadowLight, ShadowSplit};
use glam::Vec4;

/// The face field of view of a point light shadow map, in degrees.
const FACE_FOV: f32 = 90.0;
/// The largest field of view a perspective shadow map takes, in degrees.
const MAX_FOV: f32 = 170.0;
/// The near plane of a shadow map is this fraction of the light's range, up to [`NEAR_LIMIT`].
const NEAR_FRACTION: f32 = 0.01;
const NEAR_LIMIT: f32 = 0.1;

/// The view, projection and planes of a spot light's shadow map, or `None` when no renderer in
/// the light's range casts a shadow.
///
/// # Errors
/// Refuses a light with no range, no direction or a cone of 0 or 170 degrees and more.
pub fn spot_split(
    light: &ShadowLight,
    casters: &Casters<'_>,
) -> Result<Option<ShadowSplit>, ShadowError> {
    let basis = LightBasis::of(&light.local_to_world)?;
    fitted(light, casters, &basis, light.spot_angle)
}

/// The view, projection and planes of one face of a point light's shadow map, with `fov_bias`
/// degrees added to the face's quarter turn, or `None` when no renderer in the light's range casts
/// a shadow.
///
/// # Errors
/// Refuses a face outside 0 to 5 (the cube's faces in the order +x, -x, +y, -y, +z, -z), a light
/// with no range and a bias that makes the field of view 0 or 170 degrees and more.
pub fn point_split(
    light: &ShadowLight,
    face: u32,
    fov_bias: f32,
    casters: &Casters<'_>,
) -> Result<Option<ShadowSplit>, ShadowError> {
    let basis = LightBasis::of_face(face)?;
    fitted(light, casters, &basis, FACE_FOV + fov_bias)
}

fn fitted(
    light: &ShadowLight,
    casters: &Casters<'_>,
    basis: &LightBasis,
    fov: f32,
) -> Result<Option<ShadowSplit>, ShadowError> {
    let valid = light.range.is_finite() && light.range > 0.0 && fov.is_finite();
    if !valid || fov <= 0.0 || fov >= MAX_FOV {
        return Err(ShadowError::InvalidLight);
    }
    if casters.affecting(light).is_empty() {
        return Ok(None);
    }
    let eye = light.local_to_world.w_axis.truncate();
    let near = (light.range * NEAR_FRACTION).min(NEAR_LIMIT);
    let view = basis.view(eye);
    let projection = perspective(fov, near, light.range);
    let culling_matrix = projection * view;
    Ok(Some(ShadowSplit {
        view,
        projection,
        culling_matrix,
        culling_sphere: Vec4::ZERO,
        planes: side_and_far_planes(&culling_matrix),
    }))
}
