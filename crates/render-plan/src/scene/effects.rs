//! Owned renderer-domain effects: sky, sun, ambient, shadows, SSAO, camera,
//! color grading, post stack, anti-aliasing, and optional water.
//!
//! Every quantity is an integer micro-unit, millimeter, or millidegree count
//! and every renderer-semantic choice is a closed variant, so one effects
//! value admits exactly one interpretation. A scene may omit this entire
//! environmental profile for unlit rendering. Within a profile, water and
//! SSAO are optional; sky, sun, ambient, cascades, camera, grading, the post
//! stack, and the anti-aliasing choice are explicit.

use crate::scene::direction::SceneDirection;
use crate::scene::error::SceneValidationError;
use crate::scene::resource::TextureHandle;

/// Micro-unit scale shared by every effects ratio.
pub const EFFECTS_MICRO_SCALE: u64 = 1_000_000;

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

/// Screen-space ambient occlusion parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSsao {
    /// Number of deterministic hemisphere samples, within 1..=64.
    pub sample_count: u32,
    /// Surface self-occlusion bias in millimetres.
    pub bias_millimeters: u32,
    /// Occlusion sample radius in millimeters; strictly positive.
    pub radius_millimeters: u64,
    /// Occlusion strength in micro-units within `0..=2_000_000`.
    pub intensity_micro: u64,
    /// Sample horizon limit in millidegrees within `0..=180_000`.
    pub horizon_angle_millidegrees: u32,
}

impl SceneSsao {
    /// Validates radius, intensity, and horizon bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first out-of-bounds field.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range("ssao sample_count", u64::from(self.sample_count), 1, 64)?;
        require_range(
            "ssao radius_millimeters",
            self.radius_millimeters,
            1,
            u64::MAX,
        )?;
        require_range(
            "ssao intensity_micro",
            self.intensity_micro,
            0,
            2 * EFFECTS_MICRO_SCALE,
        )?;
        require_range(
            "ssao horizon_angle_millidegrees",
            u64::from(self.horizon_angle_millidegrees),
            0,
            180_000,
        )
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
    },
}

impl SceneAntiAlias {
    /// Validates the choice's own quantity bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when a weight leaves the unit
    /// interval.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        let weight = match self {
            Self::Disabled => return Ok(()),
            Self::Fxaa { subpixel_micro } => *subpixel_micro,
            Self::Taa { history_micro } => *history_micro,
        };
        require_range(
            "anti-alias weight",
            u64::from(weight),
            0,
            EFFECTS_MICRO_SCALE,
        )
    }
}

/// One axis-aligned water plane rectangle in the east/north plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterPlane {
    /// Signed minimum east corner in millimeters.
    pub east_min_millimeters: i64,
    /// Signed minimum north corner in millimeters.
    pub north_min_millimeters: i64,
    /// Strictly positive east extent in millimeters.
    pub east_extent_millimeters: u64,
    /// Strictly positive north extent in millimeters.
    pub north_extent_millimeters: u64,
}

impl SceneWaterPlane {
    /// Validates extents and corner arithmetic.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when an extent is zero or a maximum
    /// corner computation overflows.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_extent(
            self.east_extent_millimeters,
            "water east_extent_millimeters",
        )?;
        require_extent(
            self.north_extent_millimeters,
            "water north_extent_millimeters",
        )?;
        checked_corner(
            self.east_min_millimeters,
            self.east_extent_millimeters,
            "water east corner",
        )?;
        checked_corner(
            self.north_min_millimeters,
            self.north_extent_millimeters,
            "water north corner",
        )
    }
}

/// Authored parts adopted by [`SceneEffects::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneEffectsParts {
    /// Optional water surface rectangle.
    pub water: Option<SceneWaterPlane>,
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
    water: Option<SceneWaterPlane>,
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
    /// Adopts every effects part after validating all quantity bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn new(parts: SceneEffectsParts) -> Result<Self, SceneValidationError> {
        let effects = Self {
            water: parts.water,
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

    /// Returns the authored water rectangle, when the scene carries one.
    #[must_use]
    pub const fn water(&self) -> Option<&SceneWaterPlane> {
        self.water.as_ref()
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
        if let Some(water) = self.water {
            water.validate()?;
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
fn require_range(
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

/// Requires one strictly positive extent.
fn require_extent(extent: u64, part: &'static str) -> Result<(), SceneValidationError> {
    require_range(part, extent, 1, u64::MAX)
}

/// Requires one minimum corner plus extent to stay inside `i64`.
fn checked_corner(
    minimum: i64,
    extent: u64,
    part: &'static str,
) -> Result<(), SceneValidationError> {
    let extent =
        i64::try_from(extent).map_err(|_| SceneValidationError::ArithmeticOverflow { part })?;
    minimum
        .checked_add(extent)
        .ok_or(SceneValidationError::ArithmeticOverflow { part })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::direction::SceneDirection;

    fn parts() -> SceneEffectsParts {
        SceneEffectsParts {
            water: Some(SceneWaterPlane {
                east_min_millimeters: -1_000,
                north_min_millimeters: -1_000,
                east_extent_millimeters: 2_000,
                north_extent_millimeters: 2_000,
            }),
            sky: SceneSky::ProceduralHemisphere {
                zenith: SceneColor {
                    red: 96,
                    green: 148,
                    blue: 220,
                },
                horizon: SceneColor {
                    red: 208,
                    green: 226,
                    blue: 240,
                },
            },
            sun: SceneSun {
                direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
                irradiance_micro: 800_000,
            },
            ambient: SceneAmbient {
                sky_irradiance_micro: 200_000,
                ground_irradiance_micro: 80_000,
            },
            shadows: SceneShadowCascades::new(vec![8_000, 32_000], 10_000)
                .expect("synthetic cascades validate"),
            ssao: Some(SceneSsao {
                sample_count: 16,
                bias_millimeters: 10,
                radius_millimeters: 1_200,
                intensity_micro: 1_000_000,
                horizon_angle_millidegrees: 90_000,
            }),
            camera: SceneCamera {
                vertical_fov_millidegrees: 60_000,
                near_plane_millimeters: 100,
                far_plane_millimeters: 2_000_000,
            },
            grading: SceneColorGrading {
                exposure_micro: 1_000_000,
                contrast_micro: 1_000_000,
                saturation_micro: 1_000_000,
            },
            post: vec![ScenePostStage::Bloom {
                threshold_micro: 900_000,
                intensity_micro: 250_000,
            }],
            anti_alias: SceneAntiAlias::Fxaa {
                subpixel_micro: 250_000,
            },
        }
    }

    #[test]
    fn nonneutral_grading_cannot_construct_scene_effects() {
        for (contrast, saturation, part, observed) in [
            (999_999, 1_000_000, "grading contrast_micro", 999_999),
            (1_000_000, 1_000_001, "grading saturation_micro", 1_000_001),
            (0, 2_000_000, "grading contrast_micro", 0),
        ] {
            let mut staged = parts();
            staged.grading.contrast_micro = contrast;
            staged.grading.saturation_micro = saturation;
            assert_eq!(
                SceneEffects::new(staged),
                Err(SceneValidationError::UnsupportedColorGrading {
                    part,
                    observed,
                    neutral: EFFECTS_MICRO_SCALE,
                })
            );
        }
    }

    #[test]
    fn grading_quantity_bounds_still_fail_before_capability_checks() {
        for (contrast, saturation, part) in [
            (2_000_001, 1_000_000, "grading contrast_micro"),
            (1_000_000, u32::MAX, "grading saturation_micro"),
        ] {
            let mut staged = parts();
            staged.grading.contrast_micro = contrast;
            staged.grading.saturation_micro = saturation;
            assert!(matches!(
                SceneEffects::new(staged),
                Err(SceneValidationError::RangeViolation { part: actual, .. }) if actual == part
            ));
        }
    }

    #[test]
    fn neutral_grading_preserves_the_exposure_domain() {
        for exposure in [0, 1_000_000, 1_750_000, 4_000_000] {
            let mut staged = parts();
            staged.grading.exposure_micro = exposure;
            let expected = staged.grading;
            let effects = SceneEffects::new(staged).expect("neutral grading is supported");
            assert_eq!(*effects.grading(), expected);
        }
        let mut staged = parts();
        staged.grading.exposure_micro = 4_000_001;
        assert!(matches!(
            SceneEffects::new(staged),
            Err(SceneValidationError::RangeViolation {
                part: "grading exposure_micro",
                ..
            })
        ));
    }

    #[test]
    fn valid_effects_compose_with_water() {
        let effects = SceneEffects::new(parts()).expect("synthetic effects validate");
        assert!(effects.water().is_some());
        assert_eq!(effects.shadows().extents_millimeters(), &[8_000, 32_000]);
        assert_eq!(effects.post().len(), 1);
    }

    #[test]
    fn cascade_extents_must_strictly_increase() {
        assert!(matches!(
            SceneShadowCascades::new(vec![8_000, 8_000], 0),
            Err(SceneValidationError::CascadeNotIncreasing { position: 1, .. })
        ));
        assert!(matches!(
            SceneShadowCascades::new(Vec::new(), 0),
            Err(SceneValidationError::EmptyCollection { .. })
        ));
    }

    #[test]
    fn camera_planes_must_be_ordered() {
        let mut staged = parts();
        staged.camera.far_plane_millimeters = staged.camera.near_plane_millimeters;
        assert!(matches!(
            SceneEffects::new(staged),
            Err(SceneValidationError::PlaneOrdering { .. })
        ));
    }

    #[test]
    fn quantity_bounds_fail_closed() {
        // The sun carries its own, higher ceiling; see
        // MAXIMUM_SUN_IRRADIANCE_MICRO for why it is not a unit ratio.
        let mut staged = parts();
        staged.sun.irradiance_micro = MAXIMUM_SUN_IRRADIANCE_MICRO + 1;
        assert!(matches!(
            SceneEffects::new(staged),
            Err(SceneValidationError::RangeViolation {
                observed: 10_000_001,
                ..
            })
        ));
        let mut staged = parts();
        staged.anti_alias = SceneAntiAlias::Taa {
            history_micro: 1_000_001,
        };
        assert!(matches!(
            SceneEffects::new(staged),
            Err(SceneValidationError::RangeViolation { .. })
        ));
        let mut staged = parts();
        staged.water = Some(SceneWaterPlane {
            east_min_millimeters: i64::MAX,
            north_min_millimeters: 0,
            east_extent_millimeters: 2_000,
            north_extent_millimeters: 2_000,
        });
        assert!(matches!(
            SceneEffects::new(staged),
            Err(SceneValidationError::ArithmeticOverflow { .. })
        ));
    }

    #[test]
    fn water_and_ssao_can_be_omitted_within_a_profile() {
        let mut staged = parts();
        staged.water = None;
        staged.ssao = None;
        let effects = SceneEffects::new(staged).expect("waterless effects validate");
        assert!(effects.water().is_none());
        assert!(effects.ssao().is_none());
    }
}

#[cfg(test)]
#[path = "cascade_tests.rs"]
mod cascade_tests;
