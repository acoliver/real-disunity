use glam::{Mat4, Vec3, Vec4};
use worldrunner_render_plan::SceneEffects;

use super::{DirectionalShadowSettings, ShadowError};
use crate::{draw::SceneGpu, view::SceneView};

pub(super) struct CascadeViews {
    pub matrices: Vec<Mat4>,
    pub depth_bias: [f32; 4],
    pub splits: [f32; 4],
    pub blend: f32,
    pub camera_splits: bool,
    pub near: f32,
    pub view_depth: Vec4,
}

impl CascadeViews {
    pub fn new(
        gpu: &SceneGpu,
        view: &SceneView,
        effects: &SceneEffects,
        settings: DirectionalShadowSettings,
        sun: Vec3,
    ) -> Result<Self, ShadowError> {
        let declared = effects.shadows();
        let mut value = Self {
            matrices: Vec::new(),
            depth_bias: [0.0; 4],
            splits: [0.0; 4],
            camera_splits: declared.split_distances_millimeters().is_some(),
            near: view.clip_distances_meters()[0],
            blend: meters(declared.blend_band_micro())? / 1000.0,
            view_depth: -view.view_matrix().transpose().z_axis,
        };
        let far = view.clip_distances_meters()[1];
        let count = declared.extents_millimeters().len();
        for (index, &extent) in declared.extents_millimeters().iter().enumerate() {
            value.splits[index] = meters(extent)?;
            if value.camera_splits
                && (value.splits[index] > far
                    || value.splits[index] <= value.near
                    || (index > 0 && value.splits[index] <= value.splits[index - 1]))
            {
                return Err(ShadowError::InvalidSettings);
            }
            let receiver = value.receiver_matrix(index, count, view, settings, sun)?;
            let matrix = gpu.caster_matrix(receiver);
            if !matrix.is_finite()
                || !matrix.determinant().is_finite()
                || matrix.determinant() == 0.0
            {
                return Err(ShadowError::InvalidSettings);
            }
            value.depth_bias[index] = if count == 1 && !value.camera_splits {
                settings.depth_bias
            } else {
                normalized_depth_bias(settings, matrix)?
            };
            value.matrices.push(matrix);
        }
        Ok(value)
    }

    fn receiver_matrix(
        &self,
        index: usize,
        count: usize,
        view: &SceneView,
        settings: DirectionalShadowSettings,
        sun: Vec3,
    ) -> Result<Mat4, ShadowError> {
        if self.camera_splits {
            let start = if index == 0 {
                self.near
            } else {
                let previous_start = if index == 1 {
                    self.near
                } else {
                    self.splits[index - 2]
                };
                self.splits[index - 1] - (self.splits[index - 1] - previous_start) * self.blend
            };
            worldrunner_render_shadow::stable_frustum_fit(
                view.view_matrix(),
                view.projection_matrix(),
                start,
                self.splits[index],
                sun,
                settings.resolution,
            )
            .map_err(|_| ShadowError::InvalidSettings)
        } else if count == 1 {
            settings.matrix(sun)
        } else {
            coverage_matrix(settings, sun, self.splits[index])
        }
    }
}

fn normalized_depth_bias(
    settings: DirectionalShadowSettings,
    matrix: Mat4,
) -> Result<f32, ShadowError> {
    let resolution = f32::from(u16::try_from(settings.resolution).expect("bounded resolution"));
    let reference_texel = 2.0 * settings.half_extent_meters / resolution;
    let bias_texels =
        settings.depth_bias * (2.0 * settings.depth_half_extent_meters) / reference_texel;
    let rows = matrix.transpose();
    let world_units_per_texel = 2.0 / (rows.x_axis.truncate().length() * resolution);
    let world_to_depth = rows.z_axis.truncate().length();
    let bias = bias_texels * world_units_per_texel * world_to_depth;
    if !bias.is_finite() {
        return Err(ShadowError::InvalidSettings);
    }
    Ok(bias)
}

pub(super) fn meters(value: u64) -> Result<f32, ShadowError> {
    let value = u32::try_from(value).map_err(|_| ShadowError::InvalidSettings)?;
    Ok(crate::view::meters_scalar(value))
}

pub(super) fn coverage_matrix(
    settings: DirectionalShadowSettings,
    sun: Vec3,
    extent: f32,
) -> Result<Mat4, ShadowError> {
    DirectionalShadowSettings {
        half_extent_meters: extent,
        ..settings
    }
    .matrix(sun)
}
