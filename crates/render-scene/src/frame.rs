//! Encoding the per-frame uniform block the environment shader reads.
//!
//! The block is written by hand, field by field, because `unsafe_code` is
//! forbidden workspace-wide and so no struct can simply be reinterpreted as
//! bytes. That is a constraint worth accepting rather than working around: the
//! layout below is the one place where Rust and WGSL must agree exactly, and
//! writing it explicitly makes the agreement reviewable.
//!
//! # Which space this data is in
//!
//! Only [`view_projection`](FrameUniform::view_projection) performs the
//! Z-up-to-Y-up basis change. Every other field stays in native scene space:
//! the shader shades in Z-up, which is why its hemisphere term reads
//! `normal.z`. Encoding a Y-up sun direction here would light the world from a
//! quarter-turn away while leaving the geometry correctly placed — a
//! discrepancy that looks like a broken normal map rather than a wrong
//! uniform, so the distinction is worth stating.
//!
//! # Colour encoding
//!
//! [`SceneColor`] carries 8-bit sRGB, and the shader's lighting arithmetic is
//! linear. The conversion here is the piecewise sRGB transfer function rather
//! than a 2.2-power approximation, which differs by several percent in the
//! shadows where ambient light is the only contribution and is therefore most
//! visible.

pub mod metadata;
pub mod temporal;

use crate::view::SceneView;
use glam::{Mat3, Mat4, Vec3};
use thiserror::Error;
use worldrunner_render_lighting::{
    AmbientLight, DirectionSemantics, DirectionalLight, EnvironmentConfig, ExposureOwnership,
    LightingFrame,
};
use worldrunner_render_plan::{
    ROTATION_MICRO_SCALE, SceneAmbient, SceneColor, SceneEffects, SceneSky, SceneSun,
};

/// Size in bytes of the frame uniform block.
///
/// The original 160-byte prefix is followed by nine padded SH coefficients
/// and a lighting selection vector. All members are sixteen-byte aligned.
pub const FRAME_UNIFORM_BYTES: usize = 320;

/// Micro-unit scale shared by the effects domain.
const MICRO_SCALE: f32 = 1_000_000.0;

/// Threshold below which the sRGB transfer function is linear.
const SRGB_LINEAR_THRESHOLD: f32 = 0.040_45;

/// Divisor applied to the linear segment of the sRGB transfer function.
const SRGB_LINEAR_DIVISOR: f32 = 12.92;

/// Offset applied to the exponential segment of the sRGB transfer function.
const SRGB_OFFSET: f32 = 0.055;

/// Exponent of the sRGB transfer function's exponential segment.
const SRGB_EXPONENT: f32 = 2.4;

/// Largest value an eight-bit channel can hold.
const CHANNEL_MAX: f32 = 255.0;

/// A frame uniform could not be built from the governed scene.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum FrameError {
    /// An authored effect has no implementation in the stock renderer.
    #[error("stock renderer does not support requested effect: {0}")]
    UnsupportedEffect(&'static str),
    /// The decoded lighting environment cannot be represented safely.
    #[error(transparent)]
    Environment(#[from] worldrunner_render_lighting::EnvironmentError),
    /// Ambient light was requested from a sky that has no CPU-side colour.
    #[error(
        "a dome-texture sky carries no colour the CPU can read, so hemispheric \
         ambient cannot be derived without prefiltering the dome"
    )]
    AmbientFromDomeTexture,
}

/// Per-frame data, including the uniform block and CPU selection coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameUniform {
    /// Resolved raster orientation; CPU pipeline selection only, never encoded in legacy bytes.
    pub view_convention: worldrunner_camera::ViewConvention,
    /// Zero preserves the constant-distance background; otherwise camera-forward far depth.
    pub background_far_plane_meters: f32,
    /// Scene-wide specular BRDF selection, shared with environment preparation.
    pub specular_convention: worldrunner_render_plan::SceneSpecularConvention,
    /// Scene-space prepared SH9 diffuse, or the unchanged hemisphere default.
    pub diffuse_environment: worldrunner_render_plan::SceneDiffuseEnvironment,
    /// Optional exponential height fog. Packed into previously zero uniform lanes.
    pub height_fog: Option<worldrunner_render_lighting::atmosphere::HeightFog>,
    /// Clip-from-scene transform; the only field that leaves scene space.
    pub view_projection: Mat4,
    /// Right-handed reconstruction view for screen-space geometry, outside the legacy ABI.
    pub view: Mat4,
    /// Eye position in scene space, metres, Z-up.
    pub camera_position: Vec3,
    /// Authored eye position for CPU selection; not encoded in the GPU block.
    pub camera_position_millimeters: [i64; 3],
    /// Unit vector pointing from the surface toward the sun, scene space.
    pub sun_direction: Vec3,
    /// Linear sun radiance, already scaled by irradiance.
    pub sun_irradiance: Vec3,
    /// Untinted intensity when colour prevents recovering it from radiance.
    /// Neutral frames retain the original zero-filled lighting option lanes.
    pub sun_ao_intensity: Option<f32>,
    /// Linear colour independent of direct and disk intensity.
    pub sun_tint: Vec3,
    /// Linear radiance of the upper hemisphere.
    pub ambient_sky: Vec3,
    /// Linear radiance of the lower hemisphere.
    pub ambient_ground: Vec3,
    /// Linear exposure multiplier applied to outgoing radiance.
    ///
    /// Occupies its own lane rather than one of the existing padding slots so
    /// that the remaining grading terms, which are decoded but equally
    /// unplumbed, have somewhere to land without moving any field.
    pub exposure: f32,
    /// True when the scene explicitly omits its environmental profile.
    pub unlit: bool,
    /// Deterministic animation clock shared by every geometry pass.
    pub animation_time_milliseconds: u32,
    /// Previous-frame view data; present only while projection jitter is on.
    ///
    /// Not encoded in [`to_bytes`](Self::to_bytes). See [`temporal`] for the separate block.
    pub temporal: Option<temporal::TemporalView>,
}

impl FrameUniform {
    pub(crate) fn front_face(&self) -> wgpu::FrontFace {
        match self.view_convention.handedness {
            worldrunner_camera::ViewHandedness::RightHanded => wgpu::FrontFace::Ccw,
            worldrunner_camera::ViewHandedness::DirectXLeftHanded => wgpu::FrontFace::Cw,
        }
    }

    /// The clip-from-scene transform CPU selection uses: culling, level of detail, streaming
    /// and decal choice. It never carries the sub-pixel jitter that
    /// [`view_projection`](Self::view_projection) carries while temporal jitter is on.
    #[must_use]
    pub fn selection_view_projection(&self) -> Mat4 {
        self.temporal.map_or(self.view_projection, |temporal| {
            temporal.unjittered_view_projection
        })
    }

    /// A view-only frame for unlit runtime geometry, with no lighting authority required.
    /// Builds the frame uniform from a view and the governed effects.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError`] when hemispheric ambient is requested from a sky with no readable
    /// colour.
    pub fn new(view: &SceneView, effects: Option<&SceneEffects>) -> Result<Self, FrameError> {
        let Some(effects) = effects else {
            return Ok(Self::unlit(view));
        };
        validate_stock_effects(effects)?;
        let (sky, ground) = match effects.diffuse_environment() {
            worldrunner_render_plan::SceneDiffuseEnvironment::Hemisphere => {
                ambient_radiance(*effects.sky(), effects.ambient())?
            }
            worldrunner_render_plan::SceneDiffuseEnvironment::Sh9(_) => (Vec3::ZERO, Vec3::ZERO),
        };
        let environment = EnvironmentConfig {
            frame: LightingFrame {
                up: Vec3::Z,
                coefficient_from_world: Mat3::IDENTITY,
                direction_semantics: DirectionSemantics::TowardLight,
            },
            direct: DirectionalLight {
                direction: sun_direction(effects.sun()),
                radiance: Vec3::splat(micro_to_unit(effects.sun().irradiance_micro))
                    * sun_tint(effects),
            },
            shadow_direction: None,
            ambient: AmbientLight::Hemisphere { sky, ground },
            exposure: ExposureOwnership::SceneLinear(micro_to_unit(u64::from(
                effects.grading().exposure_micro,
            ))),
        }
        .prepare()?;
        Ok(Self {
            background_far_plane_meters: background_far_plane(view, effects),
            specular_convention: effects.specular_convention(),
            diffuse_environment: effects.diffuse_environment(),
            height_fog: effects
                .atmosphere()
                .and_then(|sky| sky.fog)
                .map(crate::sky::height_fog),
            view_convention: view.resolved_view().convention(),
            view_projection: view.view_projection_matrix(),
            view: view.resolved_view().reconstruction_view_matrix(),
            camera_position: view.eye_meters(),
            camera_position_millimeters: view.eye_millimeters(),
            sun_direction: environment.toward_light,
            sun_irradiance: environment.direct_radiance,
            sun_tint: sun_tint(effects),
            sun_ao_intensity: (effects.sun_tint() != worldrunner_render_plan::SceneTint::WHITE)
                .then(|| micro_to_unit(effects.sun().irradiance_micro)),
            ambient_sky: sky,
            ambient_ground: ground,
            exposure: environment.scene_exposure,
            unlit: false,
            animation_time_milliseconds: 0,
            temporal: None,
        })
    }

    pub fn unlit(view: &SceneView) -> Self {
        Self {
            background_far_plane_meters: 0.0,
            specular_convention: worldrunner_render_plan::SceneSpecularConvention::default(),
            diffuse_environment: worldrunner_render_plan::SceneDiffuseEnvironment::Hemisphere,
            height_fog: None,
            view_convention: view.resolved_view().convention(),
            view_projection: view.view_projection_matrix(),
            view: view.resolved_view().reconstruction_view_matrix(),
            camera_position: view.eye_meters(),
            camera_position_millimeters: view.eye_millimeters(),
            sun_direction: Vec3::ZERO,
            sun_irradiance: Vec3::ZERO,
            sun_ao_intensity: None,
            sun_tint: Vec3::ONE,
            ambient_sky: Vec3::ZERO,
            ambient_ground: Vec3::ZERO,
            exposure: 1.0,
            unlit: true,
            animation_time_milliseconds: 0,
            temporal: None,
        }
    }

    /// Encodes the block in the layout the shader expects.
    #[must_use]
    pub fn to_bytes(self) -> [u8; FRAME_UNIFORM_BYTES] {
        let mut bytes = [0u8; FRAME_UNIFORM_BYTES];
        let mut offset = 0usize;
        for column in self.view_projection.to_cols_array() {
            write_f32(&mut bytes, &mut offset, column);
        }
        let fog = self.height_fog.map_or([0.0; 6], |fog| {
            if fog.density == 0.0 {
                return [0.0; 6];
            }
            let log_density =
                fog.density.ln() - fog.falloff * (self.camera_position.z - fog.base_height);
            [
                log_density,
                -(fog.falloff + 1.0),
                fog.start_distance,
                fog.radiance.x,
                fog.radiance.y,
                fog.radiance.z,
            ]
        });
        for (vector, extra) in [
            self.camera_position,
            self.sun_direction,
            self.sun_irradiance,
            self.ambient_sky,
            self.ambient_ground,
        ]
        .into_iter()
        .zip(fog)
        {
            write_f32(&mut bytes, &mut offset, vector.x);
            write_f32(&mut bytes, &mut offset, vector.y);
            write_f32(&mut bytes, &mut offset, vector.z);
            write_f32(&mut bytes, &mut offset, extra);
        }
        write_f32(&mut bytes, &mut offset, self.exposure);
        write_f32(&mut bytes, &mut offset, f32::from(u8::from(self.unlit)));
        bytes[offset..offset + 4].copy_from_slice(&self.animation_time_milliseconds.to_le_bytes());
        offset += 4;
        write_f32(&mut bytes, &mut offset, fog[5]);
        self.write_lighting(&mut bytes, &mut offset);
        debug_assert_eq!(offset, FRAME_UNIFORM_BYTES, "the block is written in full");
        bytes
    }
}

impl FrameUniform {
    #[expect(
        clippy::cast_precision_loss,
        reason = "bounded fixed-point lighting coefficients"
    )]
    fn write_lighting(self, bytes: &mut [u8; FRAME_UNIFORM_BYTES], offset: &mut usize) {
        use worldrunner_render_plan::SceneDiffuseEnvironment;
        let (coefficients, enabled) = match self.diffuse_environment {
            SceneDiffuseEnvironment::Hemisphere => ([[0; 3]; 9], 0.0),
            SceneDiffuseEnvironment::Sh9(sh) => (sh.coefficients_micro(), 1.0),
        };
        let mut padding = [0.0; 9];
        padding[0] = f32::from(u8::from(
            self.height_fog.is_some_and(|fog| fog.full_ray_density),
        ));
        padding[1] = self.background_far_plane_meters;
        if self.sun_ao_intensity.is_some() {
            padding[2..5].copy_from_slice(&self.sun_tint.to_array());
        }
        for (coefficient, extra) in coefficients.into_iter().zip(padding) {
            for channel in coefficient {
                write_f32(bytes, offset, channel as f32 / MICRO_SCALE);
            }
            write_f32(bytes, offset, extra);
        }
        let [sun_intensity, tinted] = self
            .sun_ao_intensity
            .map_or([0.0, 0.0], |intensity| [intensity, 1.0]);
        for value in [enabled, sun_intensity, tinted, 0.0] {
            write_f32(bytes, offset, value);
        }
    }
}

fn sun_tint(effects: &SceneEffects) -> Vec3 {
    Vec3::from_array(
        effects
            .sun_tint()
            .channels()
            .map(|v| micro_to_unit(u64::from(v))),
    )
}

fn background_far_plane(view: &SceneView, effects: &SceneEffects) -> f32 {
    if effects.atmosphere().is_some_and(|sky| {
        sky.background_depth_policy == worldrunner_render_plan::SceneBackgroundDepthPolicy::FarPlane
    }) {
        view.clip_distances_meters()[1]
    } else {
        0.0
    }
}

/// Rejects authored requests that the stock pipelines cannot execute.
pub(crate) fn validate_stock_effects(effects: &SceneEffects) -> Result<(), FrameError> {
    if effects.grading().contrast_micro != 1_000_000
        || effects.grading().saturation_micro != 1_000_000
    {
        return Err(FrameError::UnsupportedEffect("color grading"));
    }
    if let Some(settings) = effects.ssao() {
        crate::ssao::validate_settings(settings)
            .map_err(|_| FrameError::UnsupportedEffect("SSAO settings"))?;
    }
    crate::post::settings(effects).map_err(|_| FrameError::UnsupportedEffect("post-processing"))?;
    Ok(())
}

/// Writes one little-endian `f32` and advances the cursor.
fn write_f32(bytes: &mut [u8; FRAME_UNIFORM_BYTES], offset: &mut usize, value: f32) {
    bytes[*offset..*offset + 4].copy_from_slice(&value.to_le_bytes());
    *offset += 4;
}

/// Converts the validated scene-space direction to the shader's float format.
pub(crate) fn sun_direction(sun: &SceneSun) -> Vec3 {
    let [x, y, z] = sun.direction.components_micro().map(i64::from);
    Vec3::new(micro_component(x), micro_component(y), micro_component(z)).normalize()
}

/// Derives hemispheric ambient radiance from the sky and its irradiance scales.
///
/// The zenith drives the upper hemisphere and the horizon the lower one:
/// light reaching a surface from below has bounced off the ground, and the
/// horizon colour is the closest thing the procedural sky records to it.
fn ambient_radiance(sky: SceneSky, ambient: &SceneAmbient) -> Result<(Vec3, Vec3), FrameError> {
    match sky {
        SceneSky::ProceduralHemisphere { zenith, horizon } => Ok((
            linear_color(zenith) * micro_to_unit(ambient.sky_irradiance_micro),
            linear_color(horizon) * micro_to_unit(ambient.ground_irradiance_micro),
        )),
        SceneSky::DomeTexture { .. }
            if ambient.sky_irradiance_micro == 0 && ambient.ground_irradiance_micro == 0 =>
        {
            Ok((Vec3::ZERO, Vec3::ZERO))
        }
        SceneSky::DomeTexture { .. } => Err(FrameError::AmbientFromDomeTexture),
    }
}

/// Converts an 8-bit sRGB colour into linear radiance.
fn linear_color(color: SceneColor) -> Vec3 {
    Vec3::new(
        srgb_to_linear(channel(color.red)),
        srgb_to_linear(channel(color.green)),
        srgb_to_linear(channel(color.blue)),
    )
}

/// Applies the sRGB electro-optical transfer function to one channel.
fn srgb_to_linear(value: f32) -> f32 {
    if value <= SRGB_LINEAR_THRESHOLD {
        value / SRGB_LINEAR_DIVISOR
    } else {
        ((value + SRGB_OFFSET) / (1.0 + SRGB_OFFSET)).powf(SRGB_EXPONENT)
    }
}

/// Scales an eight-bit channel into the unit interval.
fn channel(value: u8) -> f32 {
    f32::from(value) / CHANNEL_MAX
}

/// Converts a governed micro-unit scale into a unit-interval float.
#[expect(
    clippy::cast_precision_loss,
    reason = "effects micro units are validated to 0..=1_000_000, well inside f32's exact integer range"
)]
fn micro_to_unit(value: u64) -> f32 {
    value as f32 / MICRO_SCALE
}

/// Converts one signed micro-unit component into a float.
#[expect(
    clippy::cast_precision_loss,
    reason = "a normalized component is bounded by ROTATION_MICRO_SCALE, well inside f32's exact integer range"
)]
fn micro_component(value: i64) -> f32 {
    value as f32 / ROTATION_MICRO_SCALE as f32
}

#[cfg(test)]
mod tests;
