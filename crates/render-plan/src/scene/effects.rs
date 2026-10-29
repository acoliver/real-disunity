//! Owned renderer-domain effects: sky, sun, ambient, shadows, SSAO, camera,
//! color grading, post stack, anti-aliasing, and optional water.
//!
//! Every quantity is an integer micro-unit, millimeter, or millidegree count
//! and every renderer-semantic choice is a closed variant, so one effects
//! value admits exactly one interpretation. A scene may omit this entire
//! environmental profile for unlit rendering. Within a profile, water and
//! SSAO are optional; sky, sun, ambient, cascades, camera, grading, the post
//! stack, and the anti-aliasing choice are explicit.

use super::{SceneSsao, SceneWaterBody};
use crate::scene::direction::SceneDirection;
use crate::scene::error::SceneValidationError;
use crate::scene::resource::TextureHandle;

/// Micro-unit scale shared by every effects ratio.
pub const EFFECTS_MICRO_SCALE: u64 = 1_000_000;

/// Smallest accepted TAA clip-box scale in micro-units.
pub const MINIMUM_TAA_CLIP_SCALE_MICRO: u32 = 500_000;

/// Largest accepted TAA clip-box scale in micro-units.
pub const MAXIMUM_TAA_CLIP_SCALE_MICRO: u32 = 2_000_000;

/// Clip-box scale in micro-units that keeps the neighborhood box as measured.
pub const DEFAULT_TAA_CLIP_SCALE_MICRO: u32 = 1_000_000;

/// Irradiance is an absolute scale rather than a ratio; accept up to ten units.
pub const MAXIMUM_SUN_IRRADIANCE_MICRO: u64 = 10 * EFFECTS_MICRO_SCALE;

/// One quantized sRGB color with whole byte channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneColor {
    /// Red channel in `0..=255`.
    pub red: u8,
    /// Green channel in `0..=255`.
    pub green: u8,
    /// Blue channel in `0..=255`.
    pub blue: u8,
}

/// Closed sky model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSky {
    /// A procedural hemisphere interpolated between two quantized colors.
    ProceduralHemisphere {
        /// Color at the zenith.
        zenith: SceneColor,
        /// Color at the horizon.
        horizon: SceneColor,
    },
    /// One dome texture sampled around the camera.
    DomeTexture {
        /// Texture handle of the dome imagery.
        texture: TextureHandle,
    },
}

/// One directional light with a unit direction and absolute irradiance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSun {
    /// Quantized unit direction toward the light source in the scene Z-up basis.
    pub direction: SceneDirection,
    /// Absolute irradiance scale in micro-units within
    /// `0..=MAXIMUM_SUN_IRRADIANCE_MICRO`.
    pub irradiance_micro: u64,
}

impl SceneSun {
    /// Validates irradiance bounds. The direction is valid by construction.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when irradiance exceeds
    /// `MAXIMUM_SUN_IRRADIANCE_MICRO`.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "sun irradiance_micro",
            self.irradiance_micro,
            0,
            MAXIMUM_SUN_IRRADIANCE_MICRO,
        )
    }
}

/// Hemispheric ambient light with separate sky and ground irradiance scales.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneAmbient {
    /// Sky hemisphere irradiance in micro-units within `0..=1_000_000`.
    pub sky_irradiance_micro: u64,
    /// Ground hemisphere irradiance in micro-units within `0..=1_000_000`.
    pub ground_irradiance_micro: u64,
}

impl SceneAmbient {
    /// Validates both irradiance bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when either scale leaves the unit
    /// interval.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "ambient sky_irradiance_micro",
            self.sky_irradiance_micro,
            0,
            EFFECTS_MICRO_SCALE,
        )?;
        require_range(
            "ambient ground_irradiance_micro",
            self.ground_irradiance_micro,
            0,
            EFFECTS_MICRO_SCALE,
        )
    }
}

/// Owned directional cascade policy with a unit-fraction cross-fade band.
/// `new` declares light-space coverage half-widths about a renderer-supplied focus.
/// `camera_splits` declares explicit camera-view depth boundaries instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneShadowCascades {
    extents_millimeters: Box<[u64]>,
    blend_band_micro: u64,
    camera_splits: bool,
    sampling: super::SceneShadowSampling,
}

impl SceneShadowCascades {
    /// Adopts the cascade extents after validating ordering closure.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the cascade list is empty, one
    /// extent is zero, extents do not strictly increase, or the blend band
    /// leaves the unit interval.
    pub fn new(
        extents_millimeters: Vec<u64>,
        blend_band_micro: u64,
    ) -> Result<Self, SceneValidationError> {
        let shadows = Self {
            extents_millimeters: extents_millimeters.into_boxed_slice(),
            blend_band_micro,
            camera_splits: false,
            sampling: super::SceneShadowSampling::default(),
        };
        shadows.validate()?;
        Ok(shadows)
    }

    /// Uses explicit positive camera-view depth splits in millimetres, rather than
    /// light-space coverage boxes. The renderer checks these against camera planes.
    pub fn camera_splits(
        distances_millimeters: Vec<u64>,
        blend_band_micro: u64,
    ) -> Result<Self, SceneValidationError> {
        let mut value = Self::new(distances_millimeters, blend_band_micro)?;
        value.camera_splits = true;
        Ok(value)
    }

    /// Replaces the validated directional sampling policy.
    pub fn with_sampling(
        mut self,
        sampling: super::SceneShadowSampling,
    ) -> Result<Self, SceneValidationError> {
        sampling.validate()?;
        self.sampling = sampling;
        Ok(self)
    }

    pub const fn sampling(&self) -> super::SceneShadowSampling {
        self.sampling
    }

    /// Explicit camera-view far distance of each slice, or `None` for coverage boxes.
    #[must_use]
    pub fn split_distances_millimeters(&self) -> Option<&[u64]> {
        self.camera_splits.then_some(&self.extents_millimeters)
    }

    /// Returns strictly increasing coverage half-widths, or explicit depth splits
    /// when constructed with [`Self::camera_splits`], in millimetres.
    #[must_use]
    pub fn extents_millimeters(&self) -> &[u64] {
        &self.extents_millimeters
    }

    /// Returns the cross-fade band micro value.
    #[must_use]
    pub const fn blend_band_micro(&self) -> u64 {
        self.blend_band_micro
    }

    /// Validates extent presence, positivity, strict increase, and blend bound.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        const PART: &str = "shadow cascades";
        self.sampling.validate()?;
        if self.extents_millimeters.is_empty() {
            return Err(SceneValidationError::EmptyCollection { part: PART });
        }
        require_range(
            "shadow blend_band_micro",
            self.blend_band_micro,
            0,
            EFFECTS_MICRO_SCALE,
        )?;
        for (position, extent) in self.extents_millimeters.iter().enumerate() {
            require_range("shadow cascade extent_millimeters", *extent, 1, u64::MAX)?;
            if position > 0 && *extent <= self.extents_millimeters[position - 1] {
                return Err(SceneValidationError::CascadeNotIncreasing {
                    part: PART,
                    position,
                });
            }
        }
        Ok(())
    }
}

/// Primary camera projection intrinsics; projection only, no pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneCamera {
    /// Vertical field of view in millidegrees within `1..=179_999`.
    pub vertical_fov_millidegrees: u32,
    /// Near plane distance in millimeters; strictly positive.
    pub near_plane_millimeters: u32,
    /// Far plane distance in millimeters; strictly beyond the near plane.
    pub far_plane_millimeters: u32,
}

impl SceneCamera {
    /// Validates field-of-view and plane ordering bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the field of view is not a
    /// proper open-interval angle, a plane distance is zero, or the far
    /// plane does not lie strictly beyond the near plane.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "camera vertical_fov_millidegrees",
            u64::from(self.vertical_fov_millidegrees),
            1,
            179_999,
        )?;
        require_range(
            "camera near_plane_millimeters",
            u64::from(self.near_plane_millimeters),
            1,
            u64::from(u32::MAX),
        )?;
        require_range(
            "camera far_plane_millimeters",
            u64::from(self.far_plane_millimeters),
            1,
            u64::from(u32::MAX),
        )?;
        if self.far_plane_millimeters <= self.near_plane_millimeters {
            return Err(SceneValidationError::PlaneOrdering {
                part: "camera planes",
            });
        }
        Ok(())
    }
}

/// Color grading applied before the post stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneColorGrading {
    /// Linear exposure multiplier in micro-units within `0..=4_000_000`.
    pub exposure_micro: u32,
    /// Contrast strength in micro-units; only neutral `1_000_000` is supported.
    pub contrast_micro: u32,
    /// Saturation multiplier in micro-units; only neutral `1_000_000` is supported.
    pub saturation_micro: u32,
}

impl SceneColorGrading {
    /// Validates quantity bounds and rejects unimplemented grading controls.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] for out-of-bounds quantities or nonneutral
    /// contrast or saturation, which the governed shaders cannot consume.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "grading exposure_micro",
            u64::from(self.exposure_micro),
            0,
            4 * EFFECTS_MICRO_SCALE,
        )?;
        require_range(
            "grading contrast_micro",
            u64::from(self.contrast_micro),
            0,
            2 * EFFECTS_MICRO_SCALE,
        )?;
        require_range(
            "grading saturation_micro",
            u64::from(self.saturation_micro),
            0,
            2 * EFFECTS_MICRO_SCALE,
        )?;
        for (part, observed) in [
            ("grading contrast_micro", self.contrast_micro),
            ("grading saturation_micro", self.saturation_micro),
        ] {
            if u64::from(observed) != EFFECTS_MICRO_SCALE {
                return Err(SceneValidationError::UnsupportedColorGrading {
                    part,
                    observed,
                    neutral: EFFECTS_MICRO_SCALE,
                });
            }
        }
        Ok(())
    }
}

/// One closed post-processing stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScenePostStage {
    /// Selective bloom above one threshold.
    Bloom {
        /// Luminance threshold in micro-units within `0..=4_000_000`.
        threshold_micro: u32,
        /// Blend intensity in micro-units within `0..=2_000_000`.
        intensity_micro: u32,
    },
    /// Darkened frame corners by one unit-fraction strength.
    Vignette {
        /// Corner darkening in micro-units within `0..=1_000_000`.
        strength_micro: u32,
    },
}

impl ScenePostStage {
    /// Validates the stage's own quantity bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first out-of-bounds field.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        match self {
            Self::Bloom {
                threshold_micro,
                intensity_micro,
            } => {
                require_range(
                    "bloom threshold_micro",
                    u64::from(*threshold_micro),
                    0,
                    4 * EFFECTS_MICRO_SCALE,
                )?;
                require_range(
                    "bloom intensity_micro",
                    u64::from(*intensity_micro),
                    0,
                    2 * EFFECTS_MICRO_SCALE,
                )
            }
            Self::Vignette { strength_micro } => require_range(
                "vignette strength_micro",
                u64::from(*strength_micro),
                0,
                EFFECTS_MICRO_SCALE,
            ),
        }
    }
}

/// Closed anti-aliasing choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAntiAlias {
    /// Anti-aliasing is explicitly disabled.
    Disabled,
    /// One-pass edge-directional resolve with a unit-interval subpixel blend.
    Fxaa {
        /// Subpixel blend weight in micro-units within `0..=1_000_000`.
        subpixel_micro: u32,
    },
    /// Temporal accumulation with a unit-interval history weight.
    Taa {
        /// History blend weight in micro-units within `0..=1_000_000`.
        history_micro: u32,
        /// Scale of the neighborhood color box history is clipped to, in micro-units within
        /// `500_000..=2_000_000`. `1_000_000` is the engine default; smaller values reject
        /// more ghosting and larger ones admit more history.
        clip_scale_micro: u32,
    },
}

impl SceneAntiAlias {
    /// Temporal accumulation with the given history weight and the default clip-box scale.
    #[must_use]
    pub const fn taa(history_micro: u32) -> Self {
        Self::Taa {
            history_micro,
            clip_scale_micro: DEFAULT_TAA_CLIP_SCALE_MICRO,
        }
    }

    /// Validates the choice's own quantity bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when a weight leaves the unit
    /// interval.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        let (weight, clip_scale) = match self {
            Self::Disabled => return Ok(()),
            Self::Fxaa { subpixel_micro } => (*subpixel_micro, None),
            Self::Taa {
                history_micro,
                clip_scale_micro,
            } => (*history_micro, Some(*clip_scale_micro)),
        };
        require_range(
            "anti-alias weight",
            u64::from(weight),
            0,
            EFFECTS_MICRO_SCALE,
        )?;
        clip_scale.map_or(Ok(()), |scale| {
            require_range(
                "TAA clip scale",
                u64::from(scale),
                u64::from(MINIMUM_TAA_CLIP_SCALE_MICRO),
                u64::from(MAXIMUM_TAA_CLIP_SCALE_MICRO),
            )
        })
    }
}

/// Authored parts adopted by [`SceneEffects::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneEffectsParts {
    /// Optional water surface rectangle.
    pub water: Option<SceneWaterBody>,
    /// Mandatory sky model.
    pub sky: SceneSky,
    /// Mandatory directional light.
    pub sun: SceneSun,
    /// Mandatory hemispheric ambient light.
    pub ambient: SceneAmbient,
    /// Mandatory shadow cascade set.
    pub shadows: SceneShadowCascades,
    /// Optional screen-space ambient occlusion request.
    pub ssao: Option<SceneSsao>,
    /// Mandatory camera projection intrinsics.
    pub camera: SceneCamera,
    /// Mandatory color grading.
    pub grading: SceneColorGrading,
    /// Mandatory ordered post-processing stack.
    pub post: Vec<ScenePostStage>,
    /// Mandatory anti-aliasing choice.
    pub anti_alias: SceneAntiAlias,
}

/// Complete owned effects state of one scene.
///
/// An optional scene profile with explicit lighting and grading policy. Water
/// and SSAO may be absent within a profile; the other fields are explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneEffects {
    pub(super) shadow_direction: Option<SceneDirection>,
    pub(super) sun_tint: super::SceneTint,
    pub(super) specular_convention: super::SceneSpecularConvention,
    pub(super) diffuse_environment: super::SceneDiffuseEnvironment,
    pub(super) depth_fitting: Option<super::SceneDepthFitting>,
    atmosphere: Option<super::SceneAtmosphere>,
    water_settings: Option<super::SceneWaterSettings>,
    hdr_post: Option<super::SceneHdrPost>,
    ssr: Option<super::SceneSsr>,
    water: Box<[SceneWaterBody]>,
    sky: SceneSky,
    sun: SceneSun,
    ambient: SceneAmbient,
    shadows: SceneShadowCascades,
    ssao: Option<SceneSsao>,
    camera: SceneCamera,
    grading: SceneColorGrading,
    post: Box<[ScenePostStage]>,
    anti_alias: SceneAntiAlias,
}

impl SceneEffects {
    /// Opts into atmospheric background rendering without changing existing scenes.
    pub fn with_atmosphere(
        mut self,
        settings: super::SceneAtmosphere,
    ) -> Result<Self, SceneValidationError> {
        settings.validate()?;
        self.atmosphere = Some(settings);
        Ok(self)
    }
    pub const fn atmosphere(&self) -> Option<&super::SceneAtmosphere> {
        self.atmosphere.as_ref()
    }

    pub fn with_water_settings(
        mut self,
        settings: super::SceneWaterSettings,
    ) -> Result<Self, SceneValidationError> {
        if self.water.is_empty() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water settings require a body",
            });
        }
        for body in &mut self.water {
            body.material = settings;
            body.validate()?;
        }
        self.water_settings = Some(settings);
        Ok(self)
    }
    pub const fn water_settings(&self) -> Option<super::SceneWaterSettings> {
        self.water_settings
    }

    /// Installs a complete optional HDR post policy.
    pub fn with_hdr_post(
        mut self,
        settings: super::SceneHdrPost,
    ) -> Result<Self, SceneValidationError> {
        settings.validate()?;
        self.hdr_post = Some(settings);
        Ok(self)
    }
    pub const fn hdr_post(&self) -> Option<&super::SceneHdrPost> {
        self.hdr_post.as_ref()
    }

    /// Installs explicitly validated screen-space reflection settings.
    pub fn with_ssr(mut self, settings: super::SceneSsr) -> Result<Self, SceneValidationError> {
        settings.validate()?;
        self.ssr = Some(settings);
        Ok(self)
    }
    /// Returns the authored reflection policy, including an explicit disabled request.
    pub const fn ssr(&self) -> Option<&super::SceneSsr> {
        self.ssr.as_ref()
    }

    /// Adopts every effects part after validating all quantity bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn new(parts: SceneEffectsParts) -> Result<Self, SceneValidationError> {
        let effects = Self {
            shadow_direction: None,
            sun_tint: super::SceneTint::WHITE,
            specular_convention: super::SceneSpecularConvention::default(),
            diffuse_environment: super::SceneDiffuseEnvironment::Hemisphere,
            depth_fitting: None,
            atmosphere: None,
            water_settings: None,
            hdr_post: None,
            ssr: None,
            water: parts.water.into_iter().collect(),
            sky: parts.sky,
            sun: parts.sun,
            ambient: parts.ambient,
            shadows: parts.shadows,
            ssao: parts.ssao,
            camera: parts.camera,
            grading: parts.grading,
            post: parts.post.into_boxed_slice(),
            anti_alias: parts.anti_alias,
        };
        effects.validate()?;
        Ok(effects)
    }

    /// Returns the first body of any surface type. Use `water_bodies()` for all bodies.
    #[must_use]
    pub fn water(&self) -> Option<&SceneWaterBody> {
        self.water.first()
    }

    /// Replaces the complete body set. Empty sets disable water.
    pub fn with_water_bodies(
        mut self,
        bodies: Vec<SceneWaterBody>,
    ) -> Result<Self, SceneValidationError> {
        for body in &bodies {
            body.validate()?;
        }
        if bodies.is_empty() && self.water_settings.is_some() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water settings require a body",
            });
        }
        self.water = bodies.into_boxed_slice();
        Ok(self)
    }

    pub fn water_bodies(&self) -> &[SceneWaterBody] {
        &self.water
    }

    /// Returns the mandatory sky model.
    #[must_use]
    pub const fn sky(&self) -> &SceneSky {
        &self.sky
    }

    /// Returns the mandatory directional light.
    #[must_use]
    pub const fn sun(&self) -> &SceneSun {
        &self.sun
    }

    /// Returns the mandatory ambient light.
    #[must_use]
    pub const fn ambient(&self) -> &SceneAmbient {
        &self.ambient
    }

    /// Returns the mandatory shadow cascades.
    #[must_use]
    pub const fn shadows(&self) -> &SceneShadowCascades {
        &self.shadows
    }

    /// Returns the optional SSAO request.
    #[must_use]
    pub const fn ssao(&self) -> Option<&SceneSsao> {
        self.ssao.as_ref()
    }

    /// Returns the mandatory camera intrinsics.
    #[must_use]
    pub const fn camera(&self) -> &SceneCamera {
        &self.camera
    }

    /// Returns the mandatory color grading.
    #[must_use]
    pub const fn grading(&self) -> &SceneColorGrading {
        &self.grading
    }

    /// Returns the mandatory ordered post-processing stack.
    #[must_use]
    pub fn post(&self) -> &[ScenePostStage] {
        &self.post
    }

    /// Returns the mandatory anti-aliasing choice.
    #[must_use]
    pub const fn anti_alias(&self) -> &SceneAntiAlias {
        &self.anti_alias
    }

    /// Validates every effects part.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        if let Some(settings) = self.depth_fitting {
            settings.validate()?;
        }
        if let Some(atmosphere) = &self.atmosphere {
            atmosphere.validate()?;
        }
        for water in &self.water {
            water.validate()?;
        }
        if let Some(post) = self.hdr_post {
            post.validate()?;
        }
        if let Some(ssr) = self.ssr {
            ssr.validate()?;
        }
        self.sun.validate()?;
        self.ambient.validate()?;
        self.shadows.validate()?;
        if let Some(ssao) = self.ssao {
            ssao.validate()?;
        }
        self.camera.validate()?;
        self.grading.validate()?;
        for stage in &self.post {
            stage.validate()?;
        }
        self.anti_alias.validate()
    }
}

/// Requires one inclusive integer range.
pub(super) fn require_range(
    part: &'static str,
    observed: u64,
    minimum: u64,
    maximum: u64,
) -> Result<(), SceneValidationError> {
    if observed >= minimum && observed <= maximum {
        Ok(())
    } else {
        Err(SceneValidationError::RangeViolation {
            part,
            minimum,
            maximum,
            observed,
        })
    }
}

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cascade_tests.rs"]
mod cascade_tests;
