use glam::{Mat4, Vec3, Vec4};
use worldrunner_render_plan::SceneEffects;

use super::{DirectionalShadowSettings, ShadowError};
use crate::{draw::SceneGpu, view::SceneView};

pub(super) struct CascadeViews {
    pub matrices: Vec<Mat4>,
    pub no_receivers: bool,
    pub depth_bias: [f32; 4],
    pub splits: [f32; 4],
    pub blend: f32,
    /// Fraction of the shadow distance, ending at the last cascade, that fades to lit.
    pub fade_out: f32,
    pub recurrence: Option<Vec<worldrunner_render_shadow::CascadeInterval>>,
    pub multiplier: f32,
    pub camera_splits: bool,
    pub near: f32,
    pub view_depth: Vec4,
    /// World corners of each cascade's camera slice; none without camera splits.
    pub slices: Vec<Option<[Vec3; 8]>>,
    /// Texels the receiver filter reads beyond a sample's own texel.
    pub filter_texels: f32,
}

/// Texels added to a slice hull beyond the filter footprint, for rounding.
const SLICE_SAFETY_TEXELS: f32 = 2.0;

impl CascadeViews {
    /// Cascades for the declared splits, with casters fitted to them.
    pub fn new(
        gpu: &SceneGpu,
        view: &SceneView,
        effects: &SceneEffects,
        settings: DirectionalShadowSettings,
        sun: Vec3,
    ) -> Result<Self, ShadowError> {
        let mut value = Self::declared(view, effects, settings, sun)?;
        value.fit_casters(gpu, view, settings, sun)?;
        Ok(value)
    }

    /// Cascades refitted to a depth range. Casters are fitted once, against the final splits:
    /// the refitted ones when there are receivers, otherwise the declared ones.
    pub fn fitted(
        scene: (&SceneGpu, &SceneEffects),
        view: &SceneView,
        settings: DirectionalShadowSettings,
        sun: Vec3,
        fitting: (
            worldrunner_render_shadow::DepthRange,
            worldrunner_render_shadow::RangeFit,
        ),
    ) -> Result<Self, ShadowError> {
        let (gpu, effects) = scene;
        let mut value = Self::declared(view, effects, settings, sun)?;
        value.apply_depth_fit(view, fitting)?;
        value.fit_casters(gpu, view, settings, sun)?;
        Ok(value)
    }

    /// Declared splits and their validated receiver matrices. `matrices` holds receivers until
    /// [`Self::fit_casters`] replaces them with caster-fitted ones.
    fn declared(
        view: &SceneView,
        effects: &SceneEffects,
        settings: DirectionalShadowSettings,
        sun: Vec3,
    ) -> Result<Self, ShadowError> {
        let declared = effects.shadows();
        let mut value = Self {
            matrices: Vec::new(),
            recurrence: None,
            multiplier: 0.0,
            no_receivers: false,
            depth_bias: [0.0; 4],
            splits: [0.0; 4],
            camera_splits: declared.split_distances_millimeters().is_some(),
            near: view.clip_distances_meters()[0],
            slices: Vec::new(),
            filter_texels: match declared.sampling().filter {
                worldrunner_render_plan::SceneShadowFilter::Grid { radius } => {
                    f32::from(u16::try_from(radius).map_err(|_| ShadowError::InvalidSettings)?)
                        + 1.0
                }
                worldrunner_render_plan::SceneShadowFilter::Bilinear2 => 1.0,
            },
            blend: meters(declared.blend_band_micro())? / 1000.0,
            fade_out: meters(declared.fade_out_micro())? / 1000.0,
            view_depth: -view
                .resolved_view()
                .reconstruction_view_matrix()
                .transpose()
                .z_axis,
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
            value.matrices.push(valid(receiver)?);
        }
        Ok(value)
    }

    /// Refits cascades built by [`Self::new`], fitting casters a second time. Rendering uses
    /// [`Self::fitted`]; tests use this as the reference it must match.
    #[cfg(test)]
    pub fn refit_depth(
        &mut self,
        gpu: &SceneGpu,
        view: &SceneView,
        settings: DirectionalShadowSettings,
        sun: Vec3,
        fitting: (
            worldrunner_render_shadow::DepthRange,
            worldrunner_render_shadow::RangeFit,
        ),
    ) -> Result<(), ShadowError> {
        if self.apply_depth_fit(view, fitting)? {
            self.fit_casters(gpu, view, settings, sun)?;
        }
        Ok(())
    }

    /// Moves the splits to a measured depth range. Returns whether there were receivers; with
    /// none, only `no_receivers` changes.
    fn apply_depth_fit(
        &mut self,
        view: &SceneView,
        fitting: (
            worldrunner_render_shadow::DepthRange,
            worldrunner_render_shadow::RangeFit,
        ),
    ) -> Result<bool, ShadowError> {
        use worldrunner_render_shadow::DepthRange;
        fitting
            .1
            .validate()
            .map_err(|_| ShadowError::InvalidSettings)?;
        let depth = fitting
            .1
            .clip(fitting.0, view.clip_distances_meters())
            .map_err(|_| ShadowError::InvalidSettings)?;
        let DepthRange::Receivers { near, far } = depth else {
            self.no_receivers = true;
            return Ok(false);
        };
        let count = u16::try_from(self.matrices.len()).expect("four cascades");
        if let worldrunner_render_shadow::SplitPolicy::OverlapRecurrence {
            multiplier,
            overlap,
        } = fitting.1.split_policy
        {
            self.recurrence = Some(
                fitting
                    .1
                    .split_policy
                    .intervals(near, far, count)
                    .map_err(|_| ShadowError::InvalidSettings)?,
            );
            self.blend = overlap;
            self.multiplier = multiplier;
        }
        let splits = fitting
            .1
            .splits(depth, count)
            .map_err(|_| ShadowError::InvalidSettings)?;
        self.near = near;
        self.camera_splits = true;
        self.no_receivers = false;
        for (index, split) in splits.into_iter().enumerate() {
            self.splits[index] = split;
        }
        Ok(true)
    }

    /// Scans shadow casters once per cascade against the current splits and stores the
    /// caster-fitted matrices and their depth biases.
    fn fit_casters(
        &mut self,
        gpu: &SceneGpu,
        view: &SceneView,
        settings: DirectionalShadowSettings,
        sun: Vec3,
    ) -> Result<(), ShadowError> {
        let count = self.matrices.len();
        self.slices.clear();
        for index in 0..count {
            let receiver = self.receiver_matrix(index, count, view, settings, sun)?;
            let matrix = valid(gpu.caster_matrix(receiver))?;
            self.depth_bias[index] = normalized_depth_bias(settings, matrix)?;
            self.matrices[index] = matrix;
            let slice = self.camera_splits.then(|| self.slice_corners(index, view));
            self.slices.push(slice);
        }
        Ok(())
    }

    /// Where camera split `index` starts: the previous split's blend band, or the near plane.
    fn slice_start(&self, index: usize) -> f32 {
        if let Some(intervals) = &self.recurrence {
            intervals[index].start
        } else if index == 0 {
            self.near
        } else {
            let previous_start = if index == 1 {
                self.near
            } else {
                self.splits[index - 2]
            };
            self.splits[index - 1] - (self.splits[index - 1] - previous_start) * self.blend
        }
    }

    /// World corners of the camera volume whose receivers sample cascade `index`. Receivers
    /// pick a cascade by view depth, and depth fitting lags the camera, so the first slice
    /// starts at the camera's near plane. Both ends widen by 1% to absorb rounding.
    fn slice_corners(&self, index: usize, view: &SceneView) -> [Vec3; 8] {
        let start = if index == 0 {
            view.clip_distances_meters()[0]
        } else {
            self.slice_start(index)
        };
        let resolved = view.resolved_view();
        let inverse = resolved.reconstruction_projection_matrix().inverse();
        let world = resolved.reconstruction_view_matrix().inverse();
        std::array::from_fn(|corner| {
            let x = if corner & 1 == 0 { -1.0 } else { 1.0 };
            let y = if corner & 2 == 0 { -1.0 } else { 1.0 };
            let ray = inverse.project_point3(Vec3::new(x, y, 0.0));
            let distance = if corner < 4 {
                start * 0.99
            } else {
                self.splits[index] * 1.01
            };
            world.transform_point3(ray * (distance / -ray.z))
        })
    }

    /// The light-space hull receivers of cascade `index` sample in a map drawn with `matrix`:
    /// the slice grown by the filter footprint and the receiver normal offset.
    pub fn slice_hull(
        &self,
        index: usize,
        matrix: Mat4,
        settings: DirectionalShadowSettings,
    ) -> Option<crate::draw::SliceHull> {
        let corners = self.slices.get(index).copied().flatten()?;
        let texel = 2.0 / f32::from(u16::try_from(settings.resolution).ok()?);
        let clip_per_meter = matrix.transpose().x_axis.truncate().length();
        let margin = (self.filter_texels + SLICE_SAFETY_TEXELS) * texel
            + settings.normal_bias_meters * clip_per_meter;
        Some(crate::draw::SliceHull::new(&corners, matrix, margin))
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
            let start = self.slice_start(index);
            worldrunner_render_shadow::stable_frustum_fit(
                view.resolved_view().reconstruction_view_matrix(),
                view.resolved_view().reconstruction_projection_matrix(),
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

fn valid(matrix: Mat4) -> Result<Mat4, ShadowError> {
    if !matrix.is_finite() || !matrix.determinant().is_finite() || matrix.determinant() == 0.0 {
        return Err(ShadowError::InvalidSettings);
    }
    Ok(matrix)
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
