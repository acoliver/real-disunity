use glam::{Mat4, Vec3};
use worldrunner_render_plan::{RenderScene, SceneEffects};

use super::ShadowError;

/// Hard allocation ceiling: one 4096-square depth map occupies 64 MiB.
pub const MAX_SHADOW_RESOLUTION: u32 = 4096;

/// Coverage focus, fallback single-map fit, resolution and receiver bias.
///
/// Coordinates are native Z-up scene metres. The caller encloses receivers
/// in the light-space box. Rendering extends its toward-light depth end to
/// include resident casters. XY coverage and the far depth end stay fixed.
/// A single coverage box uses this explicit fit. Multiple boxes use scene
/// extents as XY half-widths about this focus; camera splits fit the view instead.
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
    /// Authored comparison offset in normalized depth units [0, 1) of the
    /// unextended settings box. Cascades preserve its texel-relative world offset:
    /// `depth_bias * depth_half_extent_meters * resolution / half_extent_meters`
    /// texels, converted using each final caster-extended projection. A single
    /// coverage box instead subtracts this value directly, including after extension.
    pub depth_bias: f32,
    /// Offset along the receiver shading normal, in scene metres; nonnegative.
    pub normal_bias_meters: f32,
}

impl DirectionalShadowSettings {
    /// Single-layer receiver/caster policy; scene effects supply the cascade topology.
    pub fn configuration(self) -> worldrunner_render_shadow::DirectionalShadowConfig {
        use worldrunner_render_shadow::{
            DirectionalCoverage, DirectionalShadowConfig, ReceiverDimension, ShadowFilter,
        };
        DirectionalShadowConfig {
            coverage: DirectionalCoverage::Single,
            dimension: ReceiverDimension::D2Array,
            resolution: self.resolution,
            caster_compare: wgpu::CompareFunction::Less,
            receiver_compare: wgpu::CompareFunction::LessEqual,
            caster_bias: wgpu::DepthBiasState::default(),
            receiver_depth_bias: self.depth_bias,
            receiver_normal_bias: self.normal_bias_meters,
            filter: ShadowFilter::Grid {
                radius: 1,
                receiver_plane: true,
            },
        }
    }

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
        self.configuration()
            .validate(limits)
            .map_err(|_| ShadowError::InvalidSettings)?;
        // Positive finite input alone does not guarantee a representable transform.
        self.matrix(Vec3::Z)?;
        Ok(())
    }

    pub(super) fn validate_light(self, effects: &SceneEffects) -> Result<(), ShadowError> {
        let sun = crate::frame::sun_direction(effects.sun());
        self.matrix(sun)?;
        let cascades = effects.shadows();
        if cascades.split_distances_millimeters().is_some() {
            if self.resolution < 4 {
                return Err(ShadowError::InvalidSettings);
            }
        } else if cascades.extents_millimeters().len() > 1 {
            for &extent in cascades.extents_millimeters() {
                super::cascades::coverage_matrix(self, sun, super::cascades::meters(extent)?)?;
            }
        }
        Ok(())
    }

    pub(super) fn matrix(self, toward_sun: Vec3) -> Result<Mat4, ShadowError> {
        worldrunner_render_shadow::DirectionalFit {
            center: self.focus_meters,
            toward_light: toward_sun,
            up: if toward_sun.dot(Vec3::Z).abs() > 0.99 {
                Vec3::Y
            } else {
                Vec3::Z
            },
            half_extent: self.half_extent_meters,
            depth_half_extent: self.depth_half_extent_meters,
        }
        .matrix()
        .map_err(|_| ShadowError::InvalidSettings)
    }
}

pub(super) fn validate_effects(effects: &SceneEffects) -> Result<(), ShadowError> {
    crate::frame::validate_stock_effects(effects)?;
    let cascades = effects.shadows();
    if cascades.extents_millimeters().len() > 4 {
        return Err(ShadowError::UnsupportedCascades);
    }
    let mut previous = 0.0;
    for &extent in cascades.extents_millimeters() {
        let extent = super::cascades::meters(extent)?;
        if extent <= previous {
            return Err(ShadowError::InvalidSettings);
        }
        previous = extent;
    }
    if let Some(splits) = cascades.split_distances_millimeters() {
        let near = u64::from(effects.camera().near_plane_millimeters);
        let far = u64::from(effects.camera().far_plane_millimeters);
        if splits.iter().any(|&split| split <= near || split > far) {
            return Err(ShadowError::InvalidSettings);
        }
    }
    Ok(())
}

pub(super) fn validate_scene(scene: &RenderScene) -> Result<(), ShadowError> {
    validate_effects(scene.effects().ok_or(ShadowError::UnsupportedFeature(
        "shadows require an environmental profile",
    ))?)?;
    Ok(())
}
