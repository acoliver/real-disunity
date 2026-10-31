//! Optional water geometry and sampled surface inputs use the scene resource tables.
use super::{SceneValidationError, TextureHandle};

/// Surface inputs, separate from a body's location and optical coefficients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterSettings {
    /// Linear RG encodes east/north normal components in [0, 1]; Z is reconstructed.
    pub normal: Option<TextureHandle>,
    /// Repeating world-space tile extent along east/north, in millimetres.
    pub tile_millimeters: [u32; 2],
    /// Signed RG flow at unit encoding speed, added to constant body velocity.
    pub flow_speed_millimeters_per_second: u32,
    pub flow: Option<TextureHandle>,
    /// Sampling stage for raw flow seeds; Vertex requires a flow map.
    pub flow_seed_sampling: super::SceneWaterSeedSampling,
    /// Two-phase normal advection period. `None` preserves continuous scrolling.
    /// Requires a normal map and a positive period. Positive velocity advances
    /// sampling coordinates; the displacement per cycle is velocity times period.
    /// A positive period enables the half-period triangle blend described by
    /// Vlachos (2010), "Water Flow in Portal 2".
    pub flow_period_milliseconds: Option<u32>,
    /// Authored maximum refraction displacement in millimetres, including zero.
    /// Applied in the surface plane, scaled by camera-axis thickness clamped to one metre.
    /// `None` preserves stock procedural refraction without an authored bound.
    pub refraction_millimeters: Option<u32>,
    /// Screen-UV displacement per unit surface normal, in millionths (`0..=1_000_000`).
    /// Scaled by camera-axis thickness clamped to one metre. Zero disables displacement.
    /// Mutually exclusive with the world-plane refraction bound.
    pub screen_refraction_micro: Option<u32>,
    /// Lighting-only XY normal scale, in millionths (`0..=10_000_000`).
    pub lighting_normal_micro: Option<u32>,
    /// Metal fraction in millionths (`0..=1_000_000`).
    pub metallic_micro: Option<u32>,
    /// Use derivative-selected normal mip levels instead of explicit level zero.
    pub normal_mips: bool,
    /// Maximum footprint ratio for derivative-selected normal sampling (1, 2, 4, 8, 16).
    pub normal_anisotropy: super::TextureAnisotropy,
    /// Independent reflected environment amplitude and shoreline attenuation.
    pub environment: Option<super::SceneWaterEnvironment>,
    /// Compose the lit surface directly, without Beer-weighting it as medium scattering.
    pub direct_surface: bool,
    /// Finite solar highlight with an authored angular radius.
    pub sun_disk: Option<super::SceneWaterSunDisk>,
    /// Independent world-to-flow-seed texture coordinates.
    pub flow_coordinates: Option<super::SceneWaterFlowCoordinates>,
    /// Optional vertex-sampled scene wind driving the flow seeds and normal gain.
    pub wind: Option<super::SceneWaterWind>,
    /// Optional reflectance and additional scattering controls.
    pub optics: Option<super::SceneWaterOptics>,
    /// Optional depth-driven flow, normal and opacity shaping.
    pub shoreline: Option<super::SceneWaterShoreline>,
    /// Optional water-only directional-shadow receiver response.
    pub shadow_response: Option<super::SceneWaterShadowResponse>,
}
impl SceneWaterSettings {
    pub const STOCK: Self = Self {
        normal: None,
        tile_millimeters: [1000; 2],
        flow_speed_millimeters_per_second: 1000,
        flow: None,
        flow_seed_sampling: super::SceneWaterSeedSampling::Fragment,
        flow_period_milliseconds: None,
        refraction_millimeters: None,
        screen_refraction_micro: None,
        lighting_normal_micro: None,
        metallic_micro: None,
        normal_mips: false,
        normal_anisotropy: super::TextureAnisotropy::ISOTROPIC,
        environment: None,
        direct_surface: false,
        sun_disk: None,
        flow_coordinates: None,
        wind: None,
        optics: None,
        shoreline: None,
        shadow_response: None,
    };
    pub(super) fn validate_handles(
        self,
        textures: u64,
        _meshes: u64,
    ) -> Result<(), SceneValidationError> {
        for texture in [
            self.normal,
            self.flow,
            self.wind.map(|wind| wind.field.texture()),
        ]
        .into_iter()
        .flatten()
        {
            super::require_handle("water surface", texture.raw(), "texture", textures)?;
        }
        Ok(())
    }
    pub(super) fn validate_refraction(self) -> Result<(), SceneValidationError> {
        super::water_terms::validate(&self)?;
        if let Some(response) = self.shadow_response {
            response.validate()?;
        }
        if let Some(value) = self.metallic_micro {
            super::effects::require_range("water metallic", u64::from(value), 0, 1_000_000)?;
        }
        if self.normal_mips && self.normal.is_none() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water normal mips require a normal map",
            });
        }
        if let Some(value) = self.lighting_normal_micro {
            super::effects::require_range(
                "water lighting normal",
                u64::from(value),
                0,
                10_000_000,
            )?;
        }
        if let Some(value) = self.screen_refraction_micro {
            super::effects::require_range(
                "water screen refraction",
                u64::from(value),
                0,
                1_000_000,
            )?;
            if self.refraction_millimeters.is_some() {
                return Err(SceneValidationError::UnsupportedInput {
                    part: "water refraction coordinate systems are mutually exclusive",
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn water_settings_share_existing_resource_bounds() {
        assert!(SceneWaterSettings::STOCK.validate_handles(0, 0).is_ok());
        for settings in [
            SceneWaterSettings {
                normal: Some(TextureHandle::from_raw(1)),
                ..SceneWaterSettings::STOCK
            },
            SceneWaterSettings {
                flow: Some(TextureHandle::from_raw(1)),
                ..SceneWaterSettings::STOCK
            },
        ] {
            assert!(settings.validate_handles(1, 1).is_err());
            assert!(settings.validate_handles(2, 2).is_ok());
        }
    }
    #[test]
    fn two_phase_flow_requires_a_positive_period_and_normal_map() {
        let body = super::super::SceneWaterBody {
            surface: super::super::SceneWaterSurface::Rectangle,
            material: SceneWaterSettings::STOCK,
            flow_millimeters_per_second: [0; 2],
            height_millimeters: 0,
            color_micro: [0; 3],
            absorption_micro: [0; 3],
            roughness_micro: 0,
            east_min_millimeters: 0,
            north_min_millimeters: 0,
            east_extent_millimeters: 1000,
            north_extent_millimeters: 1000,
        };
        assert!(body.validate().is_ok());
        let mut authored = body;
        authored.material.flow_period_milliseconds = Some(1000);
        assert!(authored.validate().is_err());
        authored.material.normal = Some(TextureHandle::from_raw(0));
        assert!(authored.validate().is_ok());
        authored.material.flow_period_milliseconds = Some(0);
        assert!(authored.validate().is_err());
        authored.material.flow_period_milliseconds = Some(1);
        assert!(authored.validate().is_ok());
        authored.material.flow_period_milliseconds = Some(u32::MAX);
        assert!(authored.validate().is_ok());
    }
}

#[cfg(test)]
#[test]
fn water_normal_anisotropy_uses_validated_sampler_levels() {
    assert_eq!(SceneWaterSettings::STOCK.normal_anisotropy.level(), 1);
    for level in [1, 2, 4, 8, 16] {
        let mut surface = SceneWaterSettings::STOCK;
        surface.normal_anisotropy = super::TextureAnisotropy::new(level).unwrap();
        assert_eq!(surface.normal_anisotropy.level(), level);
        assert!(surface.validate_refraction().is_ok());
    }
    for level in [0, 3, 7, 17, u16::MAX] {
        assert!(super::TextureAnisotropy::new(level).is_err());
    }
}
