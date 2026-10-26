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
    /// Two-phase normal advection period. `None` preserves continuous scrolling.
    /// Requires a normal map and a positive period. Positive velocity advances
    /// sampling coordinates; the displacement per cycle is velocity times period.
    /// A positive period enables the half-period triangle blend described by
    /// Vlachos (2010), "Water Flow in Portal 2".
    pub flow_period_milliseconds: Option<u32>,
    /// Authored maximum refraction displacement in millimetres, including zero.
    /// `None` preserves stock procedural refraction without an authored bound.
    /// Stock rendering rejects every `Some` bound until displacement limiting is implemented.
    pub refraction_millimeters: Option<u32>,
}
impl SceneWaterSettings {
    pub const STOCK: Self = Self {
        normal: None,
        tile_millimeters: [1000; 2],
        flow_speed_millimeters_per_second: 1000,
        flow: None,
        flow_period_milliseconds: None,
        refraction_millimeters: None,
    };
    pub(super) fn validate_handles(
        self,
        textures: u64,
        _meshes: u64,
    ) -> Result<(), SceneValidationError> {
        for texture in [self.normal, self.flow].into_iter().flatten() {
            super::require_handle("water surface", texture.raw(), "texture", textures)?;
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
