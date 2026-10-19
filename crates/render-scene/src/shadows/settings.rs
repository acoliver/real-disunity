use glam::camera::rh::{proj::directx::orthographic, view::look_at_mat4};
use glam::{Mat4, Vec3};
use worldrunner_render_plan::{RenderScene, SceneEffects};

use super::ShadowError;

/// Hard allocation ceiling: one 4096-square depth map occupies 64 MiB.
pub const MAX_SHADOW_RESOLUTION: u32 = 4096;

/// Explicit fit and receiver bias for a single directional shadow map.
///
/// Coordinates are native Z-up scene metres. The caller must enclose both
/// receivers and casters in the light-space box. Outside it, surfaces are lit.
/// These settings, not the legacy effects cascade extent, own the map fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalShadowSettings {
    /// Centre of the light-space coverage box, in native scene metres.
    pub focus_meters: Vec3,
    /// Half-width and half-height of the light-space square, in metres.
    pub half_extent_meters: f32,
    /// Half-length of the depth range about the focus, in metres.
    pub depth_half_extent_meters: f32,
    /// Width and height of the depth texture, in texels.
    pub resolution: u32,
    /// Offset subtracted from comparison depth, in normalized depth units [0, 1).
    pub depth_bias: f32,
    /// Offset along the receiver shading normal, in scene metres; nonnegative.
    pub normal_bias_meters: f32,
}

impl DirectionalShadowSettings {
    /// Validates all fields and the bounded texture size before GPU allocation.
    /// Construction and updates additionally validate the fit against the active
    /// sun direction, since representability depends on the light orientation.
    pub fn validate(self, limits: &wgpu::Limits) -> Result<(), ShadowError> {
        let valid = self.focus_meters.is_finite()
            && self.half_extent_meters.is_finite()
            && self.half_extent_meters > 0.0
            && self.depth_half_extent_meters.is_finite()
            && self.depth_half_extent_meters > 0.0
            && self.depth_bias.is_finite()
            && (0.0..1.0).contains(&self.depth_bias)
            && self.normal_bias_meters.is_finite()
            && self.normal_bias_meters >= 0.0;
        if !valid {
            return Err(ShadowError::InvalidSettings);
        }
        let limit = limits.max_texture_dimension_2d.min(MAX_SHADOW_RESOLUTION);
        if self.resolution == 0 || self.resolution > limit {
            return Err(ShadowError::Resolution {
                requested: self.resolution,
                limit,
            });
        }
        // Positive finite input alone does not guarantee a representable transform.
        self.matrix(Vec3::Z)?;
        Ok(())
    }

    pub(super) fn validate_light(self, effects: &SceneEffects) -> Result<(), ShadowError> {
        self.matrix(crate::frame::sun_direction(effects.sun())?)?;
        Ok(())
    }

    pub(super) fn matrix(self, toward_sun: Vec3) -> Result<Mat4, ShadowError> {
        let up = if toward_sun.dot(Vec3::Z).abs() > 0.99 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        let eye = self.focus_meters + toward_sun * self.depth_half_extent_meters;
        let view = look_at_mat4(eye, self.focus_meters, up);
        let extent = self.half_extent_meters;
        let projection = orthographic(
            -extent,
            extent,
            -extent,
            extent,
            0.0,
            self.depth_half_extent_meters * 2.0,
        );
        let matrix = projection * view;
        if !matrix.is_finite() || !matrix.determinant().is_finite() || matrix.determinant() == 0.0 {
            return Err(ShadowError::InvalidSettings);
        }
        Ok(matrix)
    }
}

pub(super) fn validate_effects(effects: &SceneEffects) -> Result<(), ShadowError> {
    let cascades = effects.shadows();
    if cascades.extents_millimeters().len() != 1 || cascades.blend_band_micro() != 0 {
        return Err(ShadowError::UnsupportedCascades);
    }
    if effects.water().is_some() {
        return Err(ShadowError::UnsupportedFeature("water"));
    }
    Ok(())
}

pub(super) fn validate_scene(scene: &RenderScene) -> Result<(), ShadowError> {
    validate_effects(scene.effects())?;
    if scene.terrain().is_some() {
        return Err(ShadowError::UnsupportedFeature("terrain"));
    }
    Ok(())
}
