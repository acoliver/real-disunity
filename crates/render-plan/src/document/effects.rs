//! Authored effects recipe: water, sky, lighting, shadows, SSAO, and camera.
//!
//! Every intensity is a unit-scoped micro value, every length a whole
//! millimeter count, and every angle an exact millidegree count, so each
//! recipe admits exactly one interpretation under the document basis.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::numeric::MICRO_SCALE;
use crate::document::validate::{
    checked_add_i64, checked_sub_i64, require_positive_u32, require_positive_u64, require_range,
    require_sorted_unique, require_uri,
};
use crate::scene::SceneDirection;

const MAXIMUM_SUN_IRRADIANCE_MICRO: i64 = 10 * MICRO_SCALE;

/// Authored axis-aligned water plane rectangle.
///
/// The rectangle lives in the terrain-local east/north plane; its origin
/// corner is signed and its extents strictly positive. The water plane may
/// extend beyond the terrain rectangle; this layer imposes no containment
/// rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterPlaneRecipe {
    /// Surface elevation in scene millimetres.
    pub height_millimeters: i64,
    /// Linear scattering colour, in millionths per channel.
    pub color_micro: [u32; 3],
    /// Beer absorption coefficients per metre, in millionths.
    pub absorption_micro: [u32; 3],
    /// Surface roughness in millionths.
    pub roughness_micro: u32,

    /// Structural URI of the water surface asset.
    pub asset_uri: String,
    /// Signed minimum east corner in millimeters.
    pub east_min_millimeters: i64,
    /// Signed minimum north corner in millimeters.
    pub north_min_millimeters: i64,
    /// Strictly positive east extent in millimeters.
    pub east_extent_millimeters: u64,
    /// Strictly positive north extent in millimeters.
    pub north_extent_millimeters: u64,
}

impl WaterPlaneRecipe {
    /// Returns the structural URI of the water surface asset.
    #[must_use]
    pub fn asset_uri(&self) -> &str {
        &self.asset_uri
    }

    /// Returns the signed maximum east corner.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when the corner
    /// computation overflows.
    pub fn east_max_millimeters(&self) -> Result<i64, ContractDocumentError> {
        let extent = i64::try_from(self.east_extent_millimeters).map_err(|_| {
            ContractDocumentError::ArithmeticOverflow {
                field: "water plane east corner",
            }
        })?;
        checked_add_i64("water plane east corner", self.east_min_millimeters, extent)
    }

    /// Validates asset URI and rectangle geometry.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the URI is malformed, an
    /// extent is zero, or a corner computation overflows.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("water plane asset_uri", &self.asset_uri)?;
        require_range(
            "water roughness",
            0,
            1_000_000,
            i128::from(self.roughness_micro),
        )?;
        for color in self.color_micro {
            require_range("water colour", 0, 1_000_000, i128::from(color))?;
        }
        for absorption in self.absorption_micro {
            require_range("water absorption", 0, 10_000_000, i128::from(absorption))?;
        }
        require_positive_u64(
            "water plane east_extent_millimeters",
            self.east_extent_millimeters,
        )?;
        require_positive_u64(
            "water plane north_extent_millimeters",
            self.north_extent_millimeters,
        )?;
        self.east_max_millimeters()?;
        let extent = i64::try_from(self.north_extent_millimeters).map_err(|_| {
            ContractDocumentError::ArithmeticOverflow {
                field: "water plane north corner",
            }
        })?;
        checked_add_i64(
            "water plane north corner",
            self.north_min_millimeters,
            extent,
        )?;
        Ok(())
    }
}

/// Authored sky dome reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyRecipe {
    /// Structural URI of the sky dome asset.
    pub asset_uri: String,
}

impl SkyRecipe {
    /// Returns the structural URI of the sky dome asset.
    #[must_use]
    pub fn asset_uri(&self) -> &str {
        &self.asset_uri
    }

    /// Validates the asset URI.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the URI is malformed.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("sky asset_uri", &self.asset_uri)
    }
}

/// Authored directional (sun) light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectionalLightMicro {
    /// Quantized unit direction from a surface toward the light in the document's
    /// caller-specified coordinate basis. The composition factory must convert
    /// these XYZ components to scene Z-up before constructing a scene sun.
    pub direction: SceneDirection,
    /// Irradiance scale as a unit-interval micro value.
    pub irradiance_micro: u64,
}

impl DirectionalLightMicro {
    /// Returns the validated unit direction toward the light.
    #[must_use]
    pub const fn direction(&self) -> &SceneDirection {
        &self.direction
    }

    /// Returns the irradiance micro value.
    #[must_use]
    pub const fn irradiance_micro(&self) -> u64 {
        self.irradiance_micro
    }

    /// Validates irradiance bounds. The direction is valid by construction.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the irradiance exceeds `MAXIMUM_SUN_IRRADIANCE_MICRO`.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_range(
            "sun irradiance_micro",
            0,
            i128::from(MAXIMUM_SUN_IRRADIANCE_MICRO),
            i128::from(self.irradiance_micro),
        )
    }
}

/// Authored hemispheric ambient light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmbientLightMicro {
    /// Irradiance scale as a unit-interval micro value.
    pub irradiance_micro: u64,
}

impl AmbientLightMicro {
    /// Returns the irradiance micro value.
    #[must_use]
    pub const fn irradiance_micro(&self) -> u64 {
        self.irradiance_micro
    }

    /// Validates the irradiance bound.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the irradiance leaves the unit
    /// interval.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_range(
            "ambient irradiance_micro",
            0,
            i128::from(MICRO_SCALE),
            i128::from(self.irradiance_micro),
        )
    }
}

/// One authored shadow cascade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowCascade {
    /// Zero-based cascade index; zero is the nearest cascade.
    pub cascade_index: u8,
    /// Distance in millimeters this cascade covers; strictly positive.
    pub extent_millimeters: u64,
}

impl ShadowCascade {
    /// Returns the zero-based cascade index.
    #[must_use]
    pub const fn cascade_index(&self) -> u8 {
        self.cascade_index
    }

    /// Returns the covered distance in millimeters.
    #[must_use]
    pub const fn extent_millimeters(&self) -> u64 {
        self.extent_millimeters
    }
}

/// Authored shadow cascade set.
///
/// Cascades must be authored in strictly ascending, contiguous,
/// duplicate-free index order with strictly increasing extents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowCascadesRecipe {
    cascades: Vec<ShadowCascade>,
    /// Cross-fade band width as a unit-fraction micro value.
    pub blend_band_micro: u64,
}

impl ShadowCascadesRecipe {
    /// Validates and adopts the ordered cascades and blend band.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the cascade list violates
    /// ordering, contiguity, or extent rules, or the blend band is out of
    /// bounds.
    pub fn new(
        cascades: Vec<ShadowCascade>,
        blend_band_micro: u64,
    ) -> Result<Self, ContractDocumentError> {
        let shadows = Self {
            cascades,
            blend_band_micro,
        };
        shadows.validate()?;
        Ok(shadows)
    }

    /// Returns the strictly ordered cascades.
    #[must_use]
    pub fn cascades(&self) -> &[ShadowCascade] {
        &self.cascades
    }

    /// Returns the cross-fade band micro value.
    #[must_use]
    pub const fn blend_band_micro(&self) -> u64 {
        self.blend_band_micro
    }

    /// Validates cascade ordering, extents, and blend bound.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        if self.cascades.is_empty() {
            return Err(ContractDocumentError::EmptyCollection {
                field: "shadow cascades",
            });
        }
        require_range(
            "shadow blend_band_micro",
            0,
            i128::from(MICRO_SCALE),
            i128::from(self.blend_band_micro),
        )?;
        require_sorted_unique(
            "shadow cascades",
            self.cascades
                .iter()
                .map(|cascade| u64::from(cascade.cascade_index)),
        )?;
        for (position, cascade) in self.cascades.iter().enumerate() {
            require_positive_u64(
                "shadow cascade extent_millimeters",
                cascade.extent_millimeters,
            )?;
            if u64::from(cascade.cascade_index) != u64::try_from(position).unwrap_or(u64::MAX) {
                return Err(ContractDocumentError::ViolatedInvariant {
                    field: "shadow cascades",
                    reason: "cascade indices must be contiguous and start at zero",
                });
            }
            if position > 0
                && cascade.extent_millimeters <= self.cascades[position - 1].extent_millimeters
            {
                return Err(ContractDocumentError::ViolatedInvariant {
                    field: "shadow cascade extent_millimeters",
                    reason: "cascade extents must strictly increase with cascade index",
                });
            }
        }
        Ok(())
    }
}

/// Authored screen-space ambient occlusion parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SsaoRecipe {
    /// Number of deterministic hemisphere samples, within 1..=64.
    pub sample_count: u32,
    /// Surface self-occlusion bias in millimetres.
    pub bias_millimeters: u32,
    /// Occlusion sample radius in millimeters; strictly positive.
    pub radius_millimeters: u64,
    /// Occlusion strength micro value in `[0, 2_000_000]`.
    pub intensity_micro: u64,
    /// Sample horizon limit in millidegrees in `[0, 180_000]`.
    pub horizon_angle_millidegrees: u32,
}

impl SsaoRecipe {
    /// Returns the occlusion sample radius in millimeters.
    #[must_use]
    pub const fn radius_millimeters(&self) -> u64 {
        self.radius_millimeters
    }

    /// Returns the occlusion strength micro value.
    #[must_use]
    pub const fn intensity_micro(&self) -> u64 {
        self.intensity_micro
    }

    /// Returns the sample horizon limit in millidegrees.
    #[must_use]
    pub const fn horizon_angle_millidegrees(&self) -> u32 {
        self.horizon_angle_millidegrees
    }

    /// Validates radius, intensity, and horizon bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first out-of-bounds field.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_range("ssao sample_count", 1, 64, i128::from(self.sample_count))?;
        require_positive_u64("ssao radius_millimeters", self.radius_millimeters)?;
        require_range(
            "ssao intensity_micro",
            0,
            i128::from(2 * MICRO_SCALE),
            i128::from(self.intensity_micro),
        )?;
        require_range(
            "ssao horizon_angle_millidegrees",
            0,
            180_000,
            i128::from(self.horizon_angle_millidegrees),
        )
    }
}

/// Authored primary camera projection intrinsics.
///
/// Projection intrinsics only: this layer carries no camera pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraIntrinsics {
    /// Vertical field of view in millidegrees in `(0, 180_000)`.
    pub vertical_fov_millidegrees: u32,
    /// Near plane distance in millimeters; at least one millimeter.
    pub near_plane_millimeters: u32,
    /// Far plane distance in millimeters; strictly beyond the near plane.
    pub far_plane_millimeters: u32,
}

impl CameraIntrinsics {
    /// Returns the vertical field of view in millidegrees.
    #[must_use]
    pub const fn vertical_fov_millidegrees(&self) -> u32 {
        self.vertical_fov_millidegrees
    }

    /// Returns the near plane distance in millimeters.
    #[must_use]
    pub const fn near_plane_millimeters(&self) -> u32 {
        self.near_plane_millimeters
    }

    /// Returns the far plane distance in millimeters.
    #[must_use]
    pub const fn far_plane_millimeters(&self) -> u32 {
        self.far_plane_millimeters
    }

    /// Validates field-of-view and plane ordering bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the field of view is not an
    /// open-interval proper angle, a plane distance is zero, or the far plane
    /// does not lie strictly beyond the near plane.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_range(
            "camera vertical_fov_millidegrees",
            1,
            179_999,
            i128::from(self.vertical_fov_millidegrees),
        )?;
        require_positive_u32("camera near_plane_millimeters", self.near_plane_millimeters)?;
        require_positive_u32("camera far_plane_millimeters", self.far_plane_millimeters)?;
        let gap = checked_sub_i64(
            "camera plane gap",
            i64::from(self.far_plane_millimeters),
            i64::from(self.near_plane_millimeters),
        )?;
        if gap <= 0 {
            return Err(ContractDocumentError::ViolatedInvariant {
                field: "camera plane gap",
                reason: "far plane must lie strictly beyond the near plane",
            });
        }
        Ok(())
    }
}

/// Authored parts adopted by [`EffectsRecipe::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectsRecipeParts {
    /// Optional water surface recipe.
    pub water: Option<WaterPlaneRecipe>,
    /// Sky resource and algorithm state.
    pub sky: SkyRecipe,
    /// Primary directional light.
    pub sun: DirectionalLightMicro,
    /// Ambient irradiance.
    pub ambient: AmbientLightMicro,
    /// Shadow cascade state.
    pub shadows: ShadowCascadesRecipe,
    /// Screen-space ambient occlusion state.
    pub ssao: SsaoRecipe,
    /// Camera projection intrinsics; no pose is authored in this layer.
    pub camera: CameraIntrinsics,
}

/// Authored effects recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectsRecipe {
    #[serde(skip_serializing_if = "Option::is_none")]
    ssr: Option<crate::SceneSsr>,
    water: Option<WaterPlaneRecipe>,
    sky: SkyRecipe,
    sun: DirectionalLightMicro,
    ambient: AmbientLightMicro,
    shadows: ShadowCascadesRecipe,
    ssao: SsaoRecipe,
    camera: CameraIntrinsics,
}

impl EffectsRecipe {
    /// Adopts the optional authored reflection policy after validating its quantities.
    pub fn with_ssr(mut self, settings: crate::SceneSsr) -> Result<Self, ContractDocumentError> {
        self.ssr = Some(settings);
        self.validate()?;
        Ok(self)
    }
    /// Returns optional screen-space reflection settings.
    pub const fn ssr(&self) -> Option<&crate::SceneSsr> {
        self.ssr.as_ref()
    }

    /// Validates and adopts every authored effect part.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn new(parts: EffectsRecipeParts) -> Result<Self, ContractDocumentError> {
        let effects = Self {
            ssr: None,
            water: parts.water,
            sky: parts.sky,
            sun: parts.sun,
            ambient: parts.ambient,
            shadows: parts.shadows,
            ssao: parts.ssao,
            camera: parts.camera,
        };
        effects.validate()?;
        Ok(effects)
    }

    /// Returns the authored water plane, when the scene carries one.
    #[must_use]
    pub const fn water(&self) -> Option<&WaterPlaneRecipe> {
        self.water.as_ref()
    }

    /// Returns the authored sky dome.
    #[must_use]
    pub const fn sky(&self) -> &SkyRecipe {
        &self.sky
    }

    /// Returns the authored directional light.
    #[must_use]
    pub const fn sun(&self) -> &DirectionalLightMicro {
        &self.sun
    }

    /// Returns the authored ambient light.
    #[must_use]
    pub const fn ambient(&self) -> &AmbientLightMicro {
        &self.ambient
    }

    /// Returns the authored shadow cascades.
    #[must_use]
    pub const fn shadows(&self) -> &ShadowCascadesRecipe {
        &self.shadows
    }

    /// Returns the authored SSAO parameters.
    #[must_use]
    pub const fn ssao(&self) -> &SsaoRecipe {
        &self.ssao
    }

    /// Returns the authored camera intrinsics.
    #[must_use]
    pub const fn camera(&self) -> &CameraIntrinsics {
        &self.camera
    }

    /// Validates every authored effect part.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        if let Some(water) = &self.water {
            water.validate()?;
        }
        if let Some(ssr) = self.ssr {
            ssr.validate()
                .map_err(|_| ContractDocumentError::ViolatedInvariant {
                    field: "ssr",
                    reason: "unsupported reflection settings",
                })?;
        }
        self.sky.validate()?;
        self.sun.validate()?;
        self.ambient.validate()?;
        self.shadows.validate()?;
        self.ssao.validate()?;
        self.camera.validate()
    }
}
